//! Tool-call repair pipeline — `DeepSeek`-specific failure-mode recovery.
//!
//! `DeepSeek` R1/V4 models have specific tool-call failure modes that this
//! module addresses:
//!
//! - **Scavenge**: R1 emits valid tool-call JSON inside `<think>` blocks
//!   but forgets to put it in the declared `tool_calls` field.
//! - **Flatten**: `DeepSeek` drops arguments when JSON schemas have >10 leaf
//!   params or nesting depth >2. Flatten to dot-notation and re-nest.
//! - **Storm**: Model calls the same tool with identical args in a loop.
//!   Suppress after N identical calls within a sliding window.
//! - **Truncation**: `max_tokens` clips JSON mid-structure → unbalanced
//!   braces. Repair by closing them.

use serde_json::Value;
use std::collections::{BTreeMap, VecDeque};

// ---------------------------------------------------------------------------
// Scavenge
// ---------------------------------------------------------------------------

/// A tool call recovered from `reasoning_content` by the scavenger.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScavengedCall {
    pub name: String,
    pub arguments: String,
    pub source: ScavengeSource,
}

/// Where a scavenged call was found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScavengeSource {
    /// From a `{name, arguments}` JSON object in `reasoning_content`.
    RawJson,
    /// From an OpenAI-style `{type: "function", function: {name, arguments}}`.
    OpenAiStyle,
    /// From an R1 free-form `{tool_name, tool_args}` object.
    R1Freeform,
}

/// Scavenge tool calls from `reasoning_content` that the model forgot to
/// put in the declared `tool_calls` field.
///
/// Supports three patterns:
/// 1. Raw JSON: `{"name": "read_file", "arguments": {"path": "/x"}}`
/// 2. OpenAI-style: `{"type": "function", "function": {"name": "search", "arguments": "..."}}`
/// 3. R1 free-form: `{"tool_name": "web_search", "tool_args": {"query": "hello"}}`
#[must_use]
pub fn scavenge_tool_calls(reasoning_content: &str, allowed_names: &[&str]) -> Vec<ScavengedCall> {
    if reasoning_content.is_empty() {
        return Vec::new();
    }

    let allowed: std::collections::BTreeSet<&str> = allowed_names.iter().copied().collect();
    let mut results = Vec::new();
    // Bounded search — skip unreasonably large content
    let max_len = std::cmp::min(reasoning_content.len(), 100_000);
    let content = &reasoning_content[..max_len];

    // Find all JSON objects in the text
    let mut i = 0;
    while i < content.len() {
        if results.len() >= 8 {
            break;
        }
        if content.as_bytes()[i] == b'{' {
            if let Some(end) = find_matching_brace(content, i) {
                let candidate = &content[i..=end];
                if let Some(call) = try_parse_call(candidate, &allowed) {
                    results.push(call);
                }
                i = end + 1;
                continue;
            }
        }
        i += 1;
    }

    results
}

fn find_matching_brace(text: &str, start: usize) -> Option<usize> {
    let bytes = text.as_bytes();
    if start >= bytes.len() || bytes[start] != b'{' {
        return None;
    }
    let mut depth = 1u32;
    let mut in_string = false;
    let mut escaped = false;
    for (i, c) in bytes.iter().copied().enumerate().skip(start + 1) {
        if escaped {
            escaped = false;
            continue;
        }
        if c == b'\\' && in_string {
            escaped = true;
            continue;
        }
        if c == b'"' {
            in_string = !in_string;
            continue;
        }
        if !in_string {
            if c == b'{' {
                depth += 1;
            } else if c == b'}' {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
        }
    }
    None
}

fn try_parse_call(
    json_str: &str,
    allowed: &std::collections::BTreeSet<&str>,
) -> Option<ScavengedCall> {
    let parsed: Value = serde_json::from_str(json_str).ok()?;
    let obj = parsed.as_object()?;

    // Pattern 1: {name, arguments}
    if let (Some(Value::String(name)), Some(args_val)) = (obj.get("name"), obj.get("arguments")) {
        if allowed.contains(name.as_str()) {
            let args = match args_val {
                Value::String(s) => s.clone(),
                other => other.to_string(),
            };
            return Some(ScavengedCall {
                name: name.clone(),
                arguments: args,
                source: ScavengeSource::RawJson,
            });
        }
    }

    // Pattern 2: {type: "function", function: {name, arguments}}
    if obj.get("type") == Some(&Value::String("function".to_string())) {
        if let Some(func) = obj.get("function").and_then(|v| v.as_object()) {
            if let Some(Value::String(name)) = func.get("name") {
                if allowed.contains(name.as_str()) {
                    let args = func.get("arguments").map_or_else(
                        || "{}".to_string(),
                        |v| match v {
                            Value::String(s) => s.clone(),
                            other => other.to_string(),
                        },
                    );
                    return Some(ScavengedCall {
                        name: name.clone(),
                        arguments: args,
                        source: ScavengeSource::OpenAiStyle,
                    });
                }
            }
        }
    }

    // Pattern 3: {tool_name, tool_args}
    if let Some(Value::String(name)) = obj.get("tool_name") {
        if allowed.contains(name.as_str()) {
            let args = obj
                .get("tool_args")
                .map_or_else(|| "{}".to_string(), std::string::ToString::to_string);
            return Some(ScavengedCall {
                name: name.clone(),
                arguments: args,
                source: ScavengeSource::R1Freeform,
            });
        }
    }

    None
}

// ---------------------------------------------------------------------------
// Flame (Schema flattening)
// ---------------------------------------------------------------------------

/// Decision about whether a schema should be flattened.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FlattenDecision {
    pub should_flatten: bool,
    pub leaf_count: usize,
    pub max_depth: usize,
}

/// Analyze a JSON schema and decide if it needs flattening.
/// `DeepSeek` drops arguments when a schema has >10 leaf params or depth >2.
#[must_use]
pub fn analyze_schema(schema: &Value) -> FlattenDecision {
    let mut leaf_count = 0usize;
    let mut max_depth = 0usize;
    walk_schema(schema, 0, &mut leaf_count, &mut max_depth);
    FlattenDecision {
        should_flatten: leaf_count > 10 || max_depth > 2,
        leaf_count,
        max_depth,
    }
}

fn walk_schema(schema: &Value, depth: usize, leaf_count: &mut usize, max_depth: &mut usize) {
    if depth > *max_depth {
        *max_depth = depth;
    }
    match schema.get("type").and_then(Value::as_str) {
        Some("object") => {
            if let Some(props) = schema.get("properties").and_then(Value::as_object) {
                for child in props.values() {
                    walk_schema(child, depth + 1, leaf_count, max_depth);
                }
            } else {
                *leaf_count += 1;
            }
        }
        _ => {
            *leaf_count += 1;
        }
    }
}

/// Flatten a deep JSON schema to dot-notation form.
/// `{user: {address: {city: "string"}}}` → `{"user.address.city": {"type": "string"}}`
#[must_use]
pub fn flatten_schema(schema: &Value) -> Value {
    let mut flat_props = BTreeMap::new();
    let mut required = Vec::new();
    collect_flat("", schema, &mut flat_props, &mut required, true);

    let mut root = serde_json::Map::new();
    root.insert("type".to_string(), Value::String("object".to_string()));
    root.insert(
        "properties".to_string(),
        Value::Object(flat_props.into_iter().collect()),
    );
    if !required.is_empty() {
        root.insert(
            "required".to_string(),
            Value::Array(required.into_iter().map(Value::String).collect()),
        );
    }
    Value::Object(root)
}

fn collect_flat(
    prefix: &str,
    schema: &Value,
    out: &mut BTreeMap<String, Value>,
    required: &mut Vec<String>,
    is_root_required: bool,
) {
    if schema.get("type") == Some(&Value::String("object".to_string())) {
        if let Some(props) = schema.get("properties").and_then(Value::as_object) {
            let req_set: std::collections::BTreeSet<&str> = schema
                .get("required")
                .and_then(Value::as_array)
                .map(|a| a.iter().filter_map(|v| v.as_str()).collect())
                .unwrap_or_default();

            for (key, child) in props {
                let next_prefix = if prefix.is_empty() {
                    key.clone()
                } else {
                    format!("{prefix}.{key}")
                };
                let child_required = is_root_required && req_set.contains(key.as_str());
                collect_flat(&next_prefix, child, out, required, child_required);
            }
            return;
        }
    }
    // Leaf (non-object, or object without properties)
    out.insert(prefix.to_string(), schema.clone());
    if is_root_required {
        required.push(prefix.to_string());
    }
}

/// Re-nest dot-notation arguments back to nested form after dispatch.
/// `{"user.address.city": "NYC"}` → `{"user": {"address": {"city": "NYC"}}}`
#[must_use]
pub fn nest_arguments(flat_args: &Value) -> Value {
    let Some(obj) = flat_args.as_object() else {
        return flat_args.clone();
    };

    let mut result = serde_json::Map::new();
    for (key, value) in obj {
        let parts: Vec<&str> = key.split('.').collect();
        set_nested(&mut result, &parts, value.clone());
    }
    Value::Object(result)
}

fn set_nested(target: &mut serde_json::Map<String, Value>, parts: &[&str], value: Value) {
    if parts.len() == 1 {
        target.insert(parts[0].to_string(), value);
        return;
    }
    let key = parts[0].to_string();
    let child = target
        .entry(key)
        .or_insert_with(|| Value::Object(serde_json::Map::new()));
    if let Value::Object(ref mut child_map) = child {
        set_nested(child_map, &parts[1..], value);
    }
}

// ---------------------------------------------------------------------------
// Storm breaker
// ---------------------------------------------------------------------------

/// Verdict returned by [`StormBreaker::inspect`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StormVerdict {
    /// If true, this call should be suppressed (repeat-loop guard tripped).
    pub suppress: bool,
    /// Human-readable reason for suppression.
    pub reason: Option<String>,
}

/// Tracks `(name, args)` repeats; suppresses identical calls after a
/// configurable threshold within a sliding window.
pub struct StormBreaker {
    window_size: usize,
    threshold: usize,
    recent: VecDeque<RecentEntry>,
}

#[derive(Debug, Clone)]
struct RecentEntry {
    name: String,
    args: String,
    read_only: bool,
}

impl StormBreaker {
    /// Create a new storm breaker.
    ///
    /// - `window_size`: max number of recent calls to track.
    /// - `threshold`: suppress when this many identical calls are seen.
    #[must_use]
    pub fn new(window_size: usize, threshold: usize) -> Self {
        Self {
            window_size,
            threshold,
            recent: VecDeque::with_capacity(window_size),
        }
    }

    /// Inspect a tool call and return a verdict. When the verdict is
    /// `suppress: true`, the caller should skip executing the call and
    /// instead inject a reflection turn.
    ///
    /// When `mutating` is `true`, prior read-only entries are cleared
    /// (a file edit resets the storm window for subsequent reads).
    pub fn inspect(&mut self, name: &str, args: &str, mutating: bool) -> StormVerdict {
        if name.is_empty() {
            return StormVerdict {
                suppress: false,
                reason: None,
            };
        }

        // Mutating calls clear prior read-only entries
        if mutating {
            self.recent.retain(|e| !e.read_only);
        }

        let count = self
            .recent
            .iter()
            .filter(|e| e.name == name && e.args == args)
            .count();

        if count >= self.threshold.saturating_sub(1) {
            return StormVerdict {
                suppress: true,
                reason: Some(format!(
                    "{name} called with identical args {count_plus_1} times — repeat-loop guard tripped",
                    count_plus_1 = count + 1,
                )),
            };
        }

        self.recent.push_back(RecentEntry {
            name: name.to_string(),
            args: args.to_string(),
            read_only: !mutating,
        });
        while self.recent.len() > self.window_size {
            self.recent.pop_front();
        }

        StormVerdict {
            suppress: false,
            reason: None,
        }
    }

    /// Reset the storm window (call at start of each user turn).
    pub fn reset(&mut self) {
        self.recent.clear();
    }
}

// ---------------------------------------------------------------------------
// Truncation repair
// ---------------------------------------------------------------------------

/// Result of repairing a truncated JSON string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TruncationRepair {
    /// Whether any repair was attempted.
    pub changed: bool,
    /// Whether all repair strategies failed (hard fallback).
    pub fallback: bool,
    /// The repaired JSON string (same as input if unchanged).
    pub repaired: String,
    /// Diagnostic notes.
    pub notes: Vec<String>,
}

/// Attempt to repair a truncated JSON string by closing unclosed braces,
/// brackets, strings, and commas.
///
/// If repair fails, falls back to returning input unchanged with
/// `fallback: true`.
#[must_use]
pub fn repair_truncated_json(json: &str) -> TruncationRepair {
    let input = json.trim();
    if input.is_empty() || is_valid_json(input) {
        return TruncationRepair {
            changed: false,
            fallback: false,
            repaired: input.to_string(),
            notes: Vec::new(),
        };
    }

    // Build a combined repair: first close strings, then close braces,
    // then handle trailing comma + braces. This handles cases where
    // multiple issues exist simultaneously (e.g. unclosed string AND
    // unclosed braces).
    let mut notes = Vec::new();
    let mut current = input.to_string();
    let mut changed = false;

    // Phase 1: close trailing string if needed
    if let Some(fixed) = close_trailing_string(&current) {
        current = fixed;
        changed = true;
    }
    // Phase 2: remove trailing comma then close braces
    if let Some(fixed) = remove_trailing_comma_then_close(&current) {
        current = fixed;
        changed = true;
    }
    // Phase 3: close remaining unclosed braces/brackets
    if let Some(fixed) = close_braces(&current) {
        current = fixed;
        changed = true;
    }

    if changed && is_valid_json(&current) {
        notes.push("repaired truncated JSON".to_string());
        return TruncationRepair {
            changed: true,
            fallback: false,
            repaired: current,
            notes,
        };
    }

    // Try each strategy independently as a fallback
    let strategies: &[fn(&str) -> Option<String>] = &[
        close_braces,
        close_trailing_string,
        remove_trailing_comma_then_close,
    ];

    for strategy in strategies {
        if let Some(repaired) = strategy(input) {
            if is_valid_json(&repaired) {
                notes.push("repaired truncated JSON".to_string());
                return TruncationRepair {
                    changed: true,
                    fallback: false,
                    repaired,
                    notes,
                };
            }
        }
    }

    // Hard fallback
    notes.push("unrecoverable truncation — all repair strategies failed".to_string());
    TruncationRepair {
        changed: true,
        fallback: true,
        repaired: input.to_string(),
        notes,
    }
}

fn is_valid_json(s: &str) -> bool {
    serde_json::from_str::<Value>(s).is_ok()
}

fn close_trailing_string(s: &str) -> Option<String> {
    // Check if the string ends with an unclosed string (odd number of quotes)
    let mut in_string = false;
    let mut escaped = false;
    for c in s.chars() {
        if escaped {
            escaped = false;
            continue;
        }
        if c == '\\' {
            escaped = true;
            continue;
        }
        if c == '"' {
            in_string = !in_string;
        }
    }
    if in_string {
        Some(format!("{s}\""))
    } else {
        None
    }
}

fn close_braces(s: &str) -> Option<String> {
    let mut open_braces = 0i32;
    let mut open_brackets = 0i32;
    let mut in_string = false;
    let mut escaped = false;

    for c in s.chars() {
        if escaped {
            escaped = false;
            continue;
        }
        if c == '\\' {
            escaped = true;
            continue;
        }
        if c == '"' {
            in_string = !in_string;
            continue;
        }
        if in_string {
            continue;
        }
        match c {
            '{' => open_braces += 1,
            '}' => open_braces -= 1,
            '[' => open_brackets += 1,
            ']' => open_brackets -= 1,
            _ => {}
        }
    }

    let mut result = s.to_string();
    while open_brackets > 0 {
        result.push(']');
        open_brackets -= 1;
    }
    while open_braces > 0 {
        result.push('}');
        open_braces -= 1;
    }

    if result == s {
        None
    } else {
        Some(result)
    }
}

fn remove_trailing_comma_then_close(s: &str) -> Option<String> {
    let trimmed = s.trim_end();
    // Handle `...,}` — remove the trailing comma before the closing brace
    let clean = if let Some(stripped) = trimmed.strip_suffix(",}") {
        stripped.to_string() + "}"
    } else if let Some(stripped) = trimmed.strip_suffix(",]") {
        stripped.to_string() + "]"
    } else if let Some(stripped) = trimmed.strip_suffix(',') {
        stripped.to_string()
    } else {
        return None;
    };
    // Then try closing braces
    close_braces(&clean).or(Some(clean))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fmt::Write as _;

    // ====================================================================
    // Scavenge tests
    // ====================================================================

    #[test]
    fn scavenge_empty_content_returns_empty() {
        assert!(scavenge_tool_calls("", &["read_file"]).is_empty());
    }

    #[test]
    fn scavenge_raw_json_name_arguments() {
        let content = r#"I need to read a file.
{
  "name": "read_file",
  "arguments": {
    "path": "/src/main.rs"
  }
}"#;
        let calls = scavenge_tool_calls(content, &["read_file"]);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name, "read_file");
        assert!(calls[0].arguments.contains("main.rs"));
        assert_eq!(calls[0].source, ScavengeSource::RawJson);
    }

    #[test]
    fn scavenge_openai_style() {
        let content = r#"{
  "type": "function",
  "function": {
    "name": "search_content",
    "arguments": "{\"pattern\": \"TODO\"}"
  }
}"#;
        let calls = scavenge_tool_calls(content, &["search_content"]);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name, "search_content");
        assert_eq!(calls[0].source, ScavengeSource::OpenAiStyle);
    }

    #[test]
    fn scavenge_r1_freeform() {
        let content = r#"{
  "tool_name": "web_search",
  "tool_args": {
    "query": "deepseek api"
  }
}"#;
        let calls = scavenge_tool_calls(content, &["web_search"]);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name, "web_search");
        assert_eq!(calls[0].source, ScavengeSource::R1Freeform);
    }

    #[test]
    fn scavenge_ignores_unknown_tools() {
        let content = r#"{"name": "delete_everything", "arguments": {}}"#;
        let calls = scavenge_tool_calls(content, &["read_file"]);
        assert!(calls.is_empty());
    }

    #[test]
    fn scavenge_limits_results() {
        // Create 12 JSON objects — the function caps at 8 found calls
        let mut content = String::new();
        for i in 0..12 {
            // Each is a compact JSON object: {"name":"read_file","arguments":"{\"path\":\"/i\"}"}
            let _ = write!(
                content,
                r#"{{"name":"read_file","arguments":"{{\"path\":\"/{i}\"}}}}"#
            );
        }
        let calls = scavenge_tool_calls(&content, &["read_file"]);
        assert!(
            calls.len() <= 8,
            "should limit to <=8 scavenged calls, got {}",
            calls.len()
        );
    }

    // ====================================================================
    // Flatten tests
    // ====================================================================

    #[test]
    fn analyze_flat_schema_does_not_flatten() {
        let schema = serde_json::json!({
            "type": "object",
            "properties": {
                "path": {"type": "string"},
                "pattern": {"type": "string"}
            }
        });
        let decision = analyze_schema(&schema);
        assert!(!decision.should_flatten);
        assert_eq!(decision.leaf_count, 2);
    }

    #[test]
    fn analyze_deep_schema_should_flatten() {
        let schema = serde_json::json!({
            "type": "object",
            "properties": {
                "user": {
                    "type": "object",
                    "properties": {
                        "address": {
                            "type": "object",
                            "properties": {
                                "city": {"type": "string"},
                                "zip": {"type": "string"},
                                "geo": {
                                    "type": "object",
                                    "properties": {
                                        "lat": {"type": "number"},
                                        "lon": {"type": "number"}
                                    }
                                }
                            }
                        }
                    }
                }
            }
        });
        let decision = analyze_schema(&schema);
        assert!(decision.should_flatten);
    }

    #[test]
    fn flatten_and_nest_round_trip() {
        let schema = serde_json::json!({
            "type": "object",
            "properties": {
                "user": {
                    "type": "object",
                    "properties": {
                        "address": {
                            "type": "object",
                            "properties": {
                                "city": {"type": "string"}
                            }
                        }
                    }
                }
            }
        });

        let flat = flatten_schema(&schema);
        let flat_props = flat["properties"].as_object().unwrap();
        assert!(flat_props.contains_key("user.address.city"));

        let flat_args = serde_json::json!({
            "user.address.city": "NYC"
        });
        let nested = nest_arguments(&flat_args);
        assert_eq!(
            nested,
            serde_json::json!({"user": {"address": {"city": "NYC"}}})
        );
    }

    // ====================================================================
    // Storm breaker tests
    // ====================================================================

    #[test]
    fn storm_passes_unique_calls() {
        let mut breaker = StormBreaker::new(6, 3);
        assert!(
            !breaker
                .inspect("read_file", r#"{"path":"/a"}"#, false)
                .suppress
        );
        assert!(
            !breaker
                .inspect("read_file", r#"{"path":"/b"}"#, false)
                .suppress
        );
        assert!(
            !breaker
                .inspect("read_file", r#"{"path":"/c"}"#, false)
                .suppress
        );
    }

    #[test]
    fn storm_suppresses_identical_calls() {
        let mut breaker = StormBreaker::new(6, 3);
        let args = r#"{"path":"/x"}"#;
        assert!(!breaker.inspect("read_file", args, false).suppress);
        assert!(!breaker.inspect("read_file", args, false).suppress);
        // Third identical call is suppressed
        assert!(breaker.inspect("read_file", args, false).suppress);
    }

    #[test]
    fn storm_mutating_clears_read_only_window() {
        let mut breaker = StormBreaker::new(6, 3);
        let args = r#"{"path":"/x"}"#;
        breaker.inspect("read_file", args, false);
        breaker.inspect("read_file", args, false);
        // Mutating call clears prior read-only entries
        breaker.inspect("edit_file", r#"{"path":"/x"}"#, true);
        // Read again after mutation — should NOT be suppressed
        assert!(!breaker.inspect("read_file", args, false).suppress);
    }

    #[test]
    fn storm_reset_clears_window() {
        let mut breaker = StormBreaker::new(6, 3);
        let args = r#"{"path":"/x"}"#;
        breaker.inspect("read_file", args, false);
        breaker.inspect("read_file", args, false);
        breaker.reset();
        // After reset, window is clean — first two pass, third hits threshold
        assert!(!breaker.inspect("read_file", args, false).suppress);
        assert!(!breaker.inspect("read_file", args, false).suppress);
        // Third identical call after reset hits threshold (threshold=3 → 2 identical → suppress)
        assert!(breaker.inspect("read_file", args, false).suppress);
    }

    // ====================================================================
    // Truncation repair tests
    // ====================================================================

    #[test]
    fn truncation_leaves_valid_json_unchanged() {
        let result = repair_truncated_json(r#"{"path":"/foo","pattern":"bar"}"#);
        assert!(!result.changed);
        assert!(!result.fallback);
    }

    #[test]
    fn truncation_closes_unclosed_braces() {
        let result = repair_truncated_json(r#"{"path":"/foo","pattern":"bar""#);
        assert!(result.changed);
        assert!(!result.fallback);
        assert!(result.repaired.contains('}'));
        assert!(is_valid_json(&result.repaired));
    }

    #[test]
    fn truncation_closes_unclosed_string() {
        let result = repair_truncated_json(r#"{"path":"/foo","pattern":"ba"#);
        assert!(result.changed);
        assert!(!result.fallback);
        assert!(is_valid_json(&result.repaired));
    }

    #[test]
    fn truncation_handles_trailing_comma() {
        let result = repair_truncated_json(r#"{"path":"/foo","pattern":"bar",}"#);
        assert!(result.changed);
        assert!(is_valid_json(&result.repaired));
    }

    #[test]
    fn truncation_unrecoverable_falls_back() {
        let result = repair_truncated_json(r#"{"path":"#);
        // May or may not recover — should not panic
        assert!(result.changed);
    }
}
