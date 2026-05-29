//! Cache-stable session wrapper for DeepSeek prefix-cache preservation.
//!
//! DeepSeek's automatic prefix caching keys on the exact byte sequence of
//! the request. This module provides an append-only message log that (1)
//! never rewrites the immutable prefix (system prompt + tool specs), and
//! (2) compacts by appending summaries rather than replacing the message
//! list — so the prefix survives the compaction.

use crate::session::{ContentBlock, ConversationMessage, MessageRole, Session};

/// Marker prefix inserted into compacted system messages to identify them
/// as cache-stable summaries (as opposed to initial system prompts).
const CACHE_STABLE_PREFIX_MARKER: &str = "[CACHE-STABLE SUMMARY — prefix preserved]\n\n";

/// Tracks the immutable prefix region of a session for DeepSeek cache stability.
///
/// The first N messages (system prompt + tool specs + few-shots) are frozen
/// and NEVER rewritten. This ensures DeepSeek's byte-addressable prefix cache
/// hits every turn.
#[derive(Debug, Clone)]
pub struct CacheStableState {
    /// Number of prefix messages that are immutable (never rewritten).
    pub prefix_message_count: usize,
    /// Total bytes of the prefix region (for diagnostic display).
    pub prefix_estimated_tokens: usize,
}

impl CacheStableState {
    /// Create a new cache-stable state from a session.
    /// The first system message is treated as the immutable prefix.
    #[must_use]
    pub fn from_session(session: &Session) -> Self {
        let prefix_count = session
            .messages
            .iter()
            .take_while(|m| m.role == MessageRole::System)
            .count()
            .max(1); // Always preserve at least position 0

        let prefix_estimated = session
            .messages
            .iter()
            .take(prefix_count)
            .map(|m| {
                m.blocks
                    .iter()
                    .map(|b| match b {
                        ContentBlock::Text { text } => text.len() / 4,
                        _ => 0,
                    })
                    .sum::<usize>()
            })
            .sum();

        Self {
            prefix_message_count: prefix_count,
            prefix_estimated_tokens: prefix_estimated,
        }
    }

    /// Returns the first mutable message index (everything from here can
    /// be compacted away without breaking the prefix cache).
    #[must_use]
    pub fn first_mutable_index(&self) -> usize {
        self.prefix_message_count
    }

    /// Returns `true` if the session is empty or only has prefix messages.
    #[must_use]
    pub fn is_pure_prefix(&self, session: &Session) -> bool {
        session.messages.len() <= self.prefix_message_count
    }

    /// Display the cache-hit stability state.
    #[must_use]
    pub fn summary(&self) -> String {
        format!(
            "cache-stable: {} prefix messages (est. {} tokens), preserved across compactions",
            self.prefix_message_count, self.prefix_estimated_tokens,
        )
    }
}

/// Append-only compaction that preserves the immutable prefix.
///
/// Unlike `compact_session` (which replaces the entire message list),
/// cache-stable compaction appends a System message with the summary
/// BEFORE the preserved tail, and then drops old messages from index
/// `prefix_count..remove_up_to`. This ensures:
///
/// - Messages[0..prefix_count] are NEVER rewritten — the prefix cache
///   key remains unchanged across compactions.
/// - The summary is appended as a new System message (which the model
///   treats as context, not instruction).
/// - Only messages after the prefix are removed.
#[must_use]
pub fn compact_cache_stable(
    session: &Session,
    config: &CacheStableCompactionConfig,
) -> CacheStableResult {
    let prefix_count = config.cache_state.first_mutable_index();
    let total = session.messages.len();

    if total <= prefix_count + config.preserve_recent_messages {
        return CacheStableResult {
            compacted: false,
            session: session.clone(),
            removed_message_count: 0,
        };
    }

    let remove_up_to = total.saturating_sub(config.preserve_recent_messages);
    let preserved_prefix = session.messages[..prefix_count].to_vec();
    let to_compact = &session.messages[prefix_count..remove_up_to];
    let recent_tail = session.messages[remove_up_to..].to_vec();

    if to_compact.is_empty() {
        return CacheStableResult {
            compacted: false,
            session: session.clone(),
            removed_message_count: 0,
        };
    }

    let summary = if let Some(ref custom) = config.summary_text {
        custom.clone()
    } else {
        build_cache_stable_summary(to_compact)
    };

    // Build the new message list preserving the immutable prefix:
    // [prefix (unchanged)] + [summary system message] + [recent tail]
    let mut new_messages = preserved_prefix;

    new_messages.push(ConversationMessage {
        role: MessageRole::System,
        blocks: vec![ContentBlock::Text {
            text: format!("{CACHE_STABLE_PREFIX_MARKER}{summary}"),
        }],
        usage: None,
    });
    new_messages.extend(recent_tail);

    let mut compacted = session.clone();
    compacted.messages = new_messages;
    compacted.record_compaction(summary.clone(), to_compact.len());

    CacheStableResult {
        compacted: true,
        session: compacted,
        removed_message_count: to_compact.len(),
    }
}

/// Configuration for cache-stable compaction.
#[derive(Debug, Clone)]
pub struct CacheStableCompactionConfig {
    /// Current cache-stable state (tracks prefix).
    pub cache_state: CacheStableState,
    /// Number of recent messages to preserve after the summary.
    pub preserve_recent_messages: usize,
    /// Optional custom summary text (if None, auto-generated).
    pub summary_text: Option<String>,
}

impl Default for CacheStableCompactionConfig {
    fn default() -> Self {
        Self {
            cache_state: CacheStableState {
                prefix_message_count: 1,
                prefix_estimated_tokens: 0,
            },
            preserve_recent_messages: 6,
            summary_text: None,
        }
    }
}

/// Result of a cache-stable compaction operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CacheStableResult {
    /// Whether any messages were actually compacted.
    pub compacted: bool,
    /// The compacted session.
    pub session: Session,
    /// Number of messages removed.
    pub removed_message_count: usize,
}

/// Builds a compact summary of messages for cache-stable compaction.
/// Uses a lightweight structural summary (no LLM call needed).
fn build_cache_stable_summary(messages: &[ConversationMessage]) -> String {
    let user_count = messages
        .iter()
        .filter(|m| m.role == MessageRole::User)
        .count();
    let assistant_count = messages
        .iter()
        .filter(|m| m.role == MessageRole::Assistant)
        .count();
    let tool_count = messages
        .iter()
        .filter(|m| m.role == MessageRole::Tool)
        .count();

    let tool_names: Vec<String> = messages
        .iter()
        .flat_map(|m| m.blocks.iter())
        .filter_map(|b| match b {
            ContentBlock::ToolUse { name, .. } => Some(name.clone()),
            ContentBlock::ToolResult { tool_name: name, .. } => Some(name.clone()),
            _ => None,
        })
        .collect::<std::collections::BTreeSet<String>>()
        .into_iter()
        .collect();

    let mut lines = vec![
        "Earlier conversation context (compacted for space efficiency):".to_string(),
        format!("- {} earlier turns ({} user, {} assistant, {} tool)", 
            messages.len(), user_count, assistant_count, tool_count),
    ];

    if !tool_names.is_empty() {
        lines.push(format!("- Tools invoked: {}", tool_names.join(", ")));
    }

    // Extract the last few user requests as context
    let recent_requests: Vec<String> = messages
        .iter()
        .rev()
        .filter(|m| m.role == MessageRole::User)
        .take(3)
        .filter_map(|m| {
            m.blocks.iter().find_map(|b| match b {
                ContentBlock::Text { text } if !text.trim().is_empty() => {
                    let truncated = if text.len() > 120 {
                        format!("{}…", &text[..117])
                    } else {
                        text.clone()
                    };
                    Some(truncated)
                }
                _ => None,
            })
        })
        .collect();

    if !recent_requests.is_empty() {
        lines.push("- Recent user requests:".to_string());
        for req in &recent_requests {
            lines.push(format!("  - \"{req}\""));
        }
    }

    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::{ContentBlock, ConversationMessage, Session};

    fn make_session_with_prefix(prefix_count: usize, total_messages: usize) -> Session {
        let mut session = Session::new();
        // Add prefix messages (system)
        for i in 0..prefix_count {
            session.messages.push(ConversationMessage {
                role: MessageRole::System,
                blocks: vec![ContentBlock::Text {
                    text: format!("System prompt {i}"),
                }],
                usage: None,
            });
        }
        // Add non-prefix messages
        for _ in 0..(total_messages - prefix_count) {
            session.messages.push(ConversationMessage::user_text("hello"));
            session.messages.push(ConversationMessage::assistant(vec![
                ContentBlock::Text {
                    text: "ok".to_string(),
                },
            ]));
        }
        session
    }

    #[test]
    fn cache_stable_state_detects_prefix() {
        let session = make_session_with_prefix(2, 8);
        let state = CacheStableState::from_session(&session);
        assert_eq!(state.prefix_message_count, 2);
        assert_eq!(state.first_mutable_index(), 2);
    }

    #[test]
    fn cache_stable_state_ensures_at_least_one_prefix() {
        let session = Session::new();
        let state = CacheStableState::from_session(&session);
        assert_eq!(state.prefix_message_count, 1);
    }

    #[test]
    fn compact_cache_stable_preserves_prefix() {
        let session = make_session_with_prefix(1, 10);
        let cache_state = CacheStableState::from_session(&session);
        let config = CacheStableCompactionConfig {
            cache_state: cache_state.clone(),
            preserve_recent_messages: 2,
            summary_text: None,
        };

        let result = compact_cache_stable(&session, &config);
        assert!(result.compacted);
        assert!(result.removed_message_count > 0);

        // The first message (prefix) should be unchanged
        assert_eq!(
            result.session.messages[0],
            session.messages[0],
            "prefix must be preserved byte-for-byte"
        );

        // Second message should be the summary (System role)
        assert_eq!(
            result.session.messages[1].role,
            MessageRole::System,
            "second message should be a system summary"
        );
        assert!(
            result.session.messages[1]
                .blocks
                .first()
                .map_or(false, |b| matches!(b, ContentBlock::Text { text } if text.starts_with(CACHE_STABLE_PREFIX_MARKER))),
            "summary should start with cache-stable marker"
        );

        // Recent messages should be preserved after the summary
        assert!(
            result.session.messages.len() > 2,
            "should have recent messages after summary"
        );
    }

    #[test]
    fn compact_cache_stable_noop_for_small_sessions() {
        let session = make_session_with_prefix(1, 3);
        let cache_state = CacheStableState::from_session(&session);
        let config = CacheStableCompactionConfig {
            cache_state,
            preserve_recent_messages: 4,
            summary_text: None,
        };

        let result = compact_cache_stable(&session, &config);
        assert!(
            !result.compacted,
            "should not compact when total <= prefix + preserve"
        );
        assert_eq!(result.removed_message_count, 0);
    }

    #[test]
    fn cache_stable_summary_is_deterministic() {
        let session1 = make_session_with_prefix(1, 6);
        let session2 = make_session_with_prefix(1, 6);

        let config1 = CacheStableCompactionConfig {
            cache_state: CacheStableState { prefix_message_count: 1, prefix_estimated_tokens: 0 },
            preserve_recent_messages: 2,
            summary_text: None,
        };
        let config2 = CacheStableCompactionConfig {
            cache_state: CacheStableState { prefix_message_count: 1, prefix_estimated_tokens: 0 },
            preserve_recent_messages: 2,
            summary_text: None,
        };

        let result1 = compact_cache_stable(&session1, &config1);
        let result2 = compact_cache_stable(&session2, &config2);
        assert_eq!(result1.session.messages, result2.session.messages);
    }
}
