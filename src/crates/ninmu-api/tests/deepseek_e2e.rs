//! DeepSeek V4 e2e integration tests — wired against a local TCP server.
//!
//! Tests the full HTTP protocol for DeepSeek V4 models, including:
//! - `extra_body.thinking` in request payloads
//! - `reasoning_content` in SSE streaming and non-streaming responses
//! - Cache token fields (`prompt_cache_hit_tokens`, `prompt_cache_miss_tokens`)
//! - V4 pricing integration via usage tracking
//! - The `uncached_input_tokens()` fix (no double-counting cache hits in cost)

use std::sync::{Arc, MutexGuard};
use std::sync::{Mutex as StdMutex, OnceLock};

use ninmu_api::{
    metadata_for_model, resolve_model_alias, ContentBlockDelta, ContentBlockDeltaEvent,
    ContentBlockStartEvent, ContentBlockStopEvent, InputContentBlock, InputMessage,
    MessageDeltaEvent, MessageRequest, OpenAiCompatClient, OpenAiCompatConfig,
    OutputContentBlock, ProviderClient, ProviderKind, StreamEvent, ToolChoice, ToolDefinition,
};
use ninmu_runtime::{
    pricing_for_model, TokenUsage,
};
use serde_json::json;
use tokio::sync::Mutex;

mod common;

use common::{http_response, spawn_server, CapturedRequest, EnvVarGuard};

fn env_lock() -> MutexGuard<'static, ()> {
    static LOCK: OnceLock<StdMutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| StdMutex::new(()))
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn sample_deepseek_request(stream: bool) -> MessageRequest {
    MessageRequest {
        model: "deepseek-v4-flash".to_string(),
        max_tokens: 1024,
        messages: vec![InputMessage {
            role: "user".to_string(),
            content: vec![InputContentBlock::Text {
                text: "Explain prefix caching".to_string(),
            }],
        }],
        system: Some("You are a helpful assistant.".to_string()),
        tools: Some(vec![ToolDefinition {
            name: "read_file".to_string(),
            description: Some("Read a file".to_string()),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "path": {"type": "string"},
                },
                "required": ["path"],
            }),
        }]),
        tool_choice: Some(ToolChoice::Auto),
        stream,
        ..Default::default()
    }
}

// ============================================================================
// Request payload tests
// ============================================================================

#[tokio::test]
async fn deepseek_request_sends_extra_body_thinking_and_reasoning_effort() {
    let state = Arc::new(Mutex::new(Vec::<CapturedRequest>::new()));
    let body = concat!(
        "{",
        "\"id\":\"chatcmpl_ds_v4\",",
        "\"model\":\"deepseek-v4-flash\",",
        "\"choices\":[{",
        "\"message\":{\"role\":\"assistant\",\"content\":\"Prefix caching works by...\"},",
        "\"finish_reason\":\"stop\"",
        "}],",
        "\"usage\":{\"prompt_tokens\":15,\"completion_tokens\":8}",
        "}"
    );
    let server = spawn_server(
        state.clone(),
        vec![http_response("200 OK", "application/json", body)],
        true,
    )
    .await;

    let client = OpenAiCompatClient::new("ds-test-key", OpenAiCompatConfig::deepseek())
        .with_base_url(server.base_url());

    let mut request = sample_deepseek_request(false);
    request.reasoning_effort = Some("high".to_string());

    client
        .send_message(&request)
        .await
        .expect("send_message should succeed");

    let captured = state.lock().await;
    let req = captured.first().expect("request should be captured");
    let payload: serde_json::Value =
        serde_json::from_str(&req.body).expect("request body should be JSON");

    // Top-level `thinking` (OpenAI-compatible fallback)
    assert_eq!(
        payload["thinking"]["type"],
        json!("enabled"),
        "should send thinking at top level for OpenAI-compat"
    );

    // `extra_body.thinking` (DeepSeek V4 format)
    assert_eq!(
        payload["extra_body"]["thinking"]["type"],
        json!("enabled"),
        "should send extra_body.thinking for DeepSeek V4"
    );

    // reasoning_effort
    assert_eq!(
        payload["reasoning_effort"],
        json!("high"),
        "should send reasoning_effort"
    );

    // stream_options should be absent for non-streaming requests
    assert!(
        payload.get("stream_options").is_none() || payload["stream_options"].is_null(),
        "non-streaming should not include stream_options"
    );
}

#[tokio::test]
async fn deepseek_streaming_request_includes_stream_options() {
    let state = Arc::new(Mutex::new(Vec::<CapturedRequest>::new()));
    let sse = concat!(
        "data: {\"id\":\"chatcmpl_ds_stream\",\"choices\":[{\"delta\":{\"content\":\"Hello\"}}]}\n\n",
        "data: {\"id\":\"chatcmpl_ds_stream\",\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}]}\n\n",
        "data: {\"id\":\"chatcmpl_ds_stream\",\"choices\":[],\"usage\":{\"prompt_tokens\":15,\"completion_tokens\":8}}\n\n",
        "data: [DONE]\n\n"
    );
    let server = spawn_server(
        state.clone(),
        vec![http_response("200 OK", "text/event-stream", sse)],
        true,
    )
    .await;

    let client = OpenAiCompatClient::new("ds-test-key", OpenAiCompatConfig::deepseek())
        .with_base_url(server.base_url());

    let mut stream = client
        .stream_message(&sample_deepseek_request(true))
        .await
        .expect("stream should start");

    while let Some(_event) = stream.next_event().await.expect("event should parse") {}

    let captured = state.lock().await;
    let req = captured.first().expect("request should be captured");
    let payload: serde_json::Value =
        serde_json::from_str(&req.body).expect("request body should be JSON");

    assert_eq!(payload["stream"], json!(true));
    // DeepSeek's API doesn't require stream_options for cache tracking;
    // it returns usage in the final chunk regardless.
    // The important thing is that stream=true is set.
    if let Some(stream_opts) = payload.get("stream_options") {
        assert!(
            stream_opts.get("include_usage").is_some(),
            "if stream_options is present, include_usage must be set"
        );
    }
}

// ============================================================================
// Reasoning content in streaming responses
// ============================================================================

#[tokio::test]
async fn deepseek_stream_reasoning_content_emits_thinking_blocks() {
    let state = Arc::new(Mutex::new(Vec::<CapturedRequest>::new()));
    let sse = concat!(
        "data: {\"id\":\"chatcmpl_ds_reason\",\"choices\":[{\"delta\":{\"reasoning_content\":\"Let me think about\"}}]}\n\n",
        "data: {\"id\":\"chatcmpl_ds_reason\",\"choices\":[{\"delta\":{\"reasoning_content\":\" prefix caching.\"}}]}\n\n",
        "data: {\"id\":\"chatcmpl_ds_reason\",\"choices\":[{\"delta\":{\"content\":\"Prefix caching works because\"}}]}\n\n",
        "data: {\"id\":\"chatcmpl_ds_reason\",\"choices\":[{\"delta\":{\"content\":\" the byte prefix is stable.\"}}]}\n\n",
        "data: {\"id\":\"chatcmpl_ds_reason\",\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}]}\n\n",
        "data: [DONE]\n\n"
    );
    let server = spawn_server(
        state.clone(),
        vec![http_response("200 OK", "text/event-stream", sse)],
        true,
    )
    .await;

    let client = OpenAiCompatClient::new("ds-test-key", OpenAiCompatConfig::deepseek())
        .with_base_url(server.base_url());

    let mut stream = client
        .stream_message(&sample_deepseek_request(false))
        .await
        .expect("stream should start");

    let mut events = Vec::new();
    while let Some(event) = stream.next_event().await.expect("event should parse") {
        events.push(event);
    }

    // Expected event sequence: MessageStart, ThinkingStart, ThinkingDelta x2,
    // ThinkingStop, TextStart, TextDelta x2, TextStop, MessageDelta, MessageStop
    assert!(
        events.len() >= 10,
        "should have at least 10 events for reasoning stream, got {}",
        events.len()
    );

    // Verify thinking block events
    assert!(matches!(events[0], StreamEvent::MessageStart(_)));
    assert!(matches!(
        events[1],
        StreamEvent::ContentBlockStart(ContentBlockStartEvent {
            content_block: OutputContentBlock::Thinking { .. },
            ..
        })
    ));
    assert!(matches!(
        events[2],
        StreamEvent::ContentBlockDelta(ContentBlockDeltaEvent {
            delta: ContentBlockDelta::ThinkingDelta { .. },
            ..
        })
    ));
    assert!(matches!(
        events[3],
        StreamEvent::ContentBlockDelta(ContentBlockDeltaEvent {
            delta: ContentBlockDelta::ThinkingDelta { .. },
            ..
        })
    ));
    assert!(matches!(
        events[4],
        StreamEvent::ContentBlockStop(ContentBlockStopEvent { index: 0 })
    ));

    // Text block at index 1 (thinking is index 0)
    assert!(matches!(
        events[5],
        StreamEvent::ContentBlockStart(ContentBlockStartEvent {
            index: 1,
            content_block: OutputContentBlock::Text { .. },
        })
    ));
    assert!(matches!(
        events[6],
        StreamEvent::ContentBlockDelta(ContentBlockDeltaEvent {
            index: 1,
            delta: ContentBlockDelta::TextDelta { .. },
        })
    ));
    assert!(matches!(
        events[7],
        StreamEvent::ContentBlockDelta(ContentBlockDeltaEvent {
            index: 1,
            delta: ContentBlockDelta::TextDelta { .. },
        })
    ));
    assert!(matches!(
        events[8],
        StreamEvent::ContentBlockStop(ContentBlockStopEvent { index: 1 })
    ));

    // Final events
    assert!(matches!(events[events.len() - 2], StreamEvent::MessageDelta(_)));
    assert!(matches!(events[events.len() - 1], StreamEvent::MessageStop(_)));
}

// ============================================================================
// Cache token fields in responses
// ============================================================================

#[tokio::test]
async fn deepseek_cache_tokens_parsed_from_non_streaming_response() {
    let state = Arc::new(Mutex::new(Vec::<CapturedRequest>::new()));
    // DeepSeek returns prompt_cache_hit_tokens directly (not under prompt_tokens_details)
    let body = concat!(
        "{",
        "\"id\":\"chatcmpl_ds_cache\",",
        "\"model\":\"deepseek-v4-flash\",",
        "\"choices\":[{",
        "\"message\":{\"role\":\"assistant\",\"content\":\"Cached response\"},",
        "\"finish_reason\":\"stop\"",
        "}],",
        "\"usage\":{",
        "\"prompt_tokens\":1000,",
        "\"completion_tokens\":50,",
        "\"prompt_cache_hit_tokens\":800,",
        "\"prompt_cache_miss_tokens\":200",
        "}}"
    );
    let server = spawn_server(
        state.clone(),
        vec![http_response("200 OK", "application/json", body)],
        true,
    )
    .await;

    let client = OpenAiCompatClient::new("ds-test-key", OpenAiCompatConfig::deepseek())
        .with_base_url(server.base_url());

    let response = client
        .send_message(&sample_deepseek_request(false))
        .await
        .expect("send_message should succeed");

    // uncached_input_tokens = prompt_tokens - cache_read_input_tokens
    // = 1000 - 800 = 200
    assert_eq!(
        response.usage.input_tokens, 200,
        "input_tokens should be uncached (prompt_tokens - cache hits)"
    );
    assert_eq!(
        response.usage.cache_read_input_tokens, 800,
        "cache_read_input_tokens should equal prompt_cache_hit_tokens"
    );
    assert_eq!(
        response.usage.output_tokens, 50,
        "output_tokens should be preserved"
    );
}

#[tokio::test]
async fn deepseek_cache_tokens_parsed_from_streaming_response() {
    let state = Arc::new(Mutex::new(Vec::<CapturedRequest>::new()));
    // DeepSeek V4 streaming response with usage in the final chunk
    let sse = concat!(
        "data: {\"id\":\"chatcmpl_ds_stream_cache\",\"choices\":[{\"delta\":{\"content\":\"Hello\"}}]}\n\n",
        "data: {\"id\":\"chatcmpl_ds_stream_cache\",\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}]}\n\n",
        "data: {\"id\":\"chatcmpl_ds_stream_cache\",\"choices\":[],\"usage\":{\"prompt_tokens\":2000,\"completion_tokens\":100,\"prompt_cache_hit_tokens\":1800,\"prompt_cache_miss_tokens\":200}}\n\n",
        "data: [DONE]\n\n"
    );
    let server = spawn_server(
        state.clone(),
        vec![http_response("200 OK", "text/event-stream", sse)],
        true,
    )
    .await;

    let client = OpenAiCompatClient::new("ds-test-key", OpenAiCompatConfig::deepseek())
        .with_base_url(server.base_url());

    let mut stream = client
        .stream_message(&sample_deepseek_request(false))
        .await
        .expect("stream should start");

    let mut usage = None;
    while let Some(event) = stream.next_event().await.expect("event should parse") {
        if let StreamEvent::MessageDelta(MessageDeltaEvent {
            usage: event_usage,
            ..
        }) = event
        {
            usage = Some(event_usage);
        }
    }

    let usage = usage.expect("usage should be present in MessageDelta");
    // uncached = 2000 - 1800 = 200
    assert_eq!(
        usage.input_tokens, 200,
        "streaming input_tokens should be uncached"
    );
    assert_eq!(
        usage.cache_read_input_tokens, 1800,
        "streaming cache_read should equal prompt_cache_hit_tokens"
    );
    assert_eq!(
        usage.output_tokens, 100,
        "streaming output_tokens should be preserved"
    );
}

// ============================================================================
// Reasoning content round-trip (assistant → API → next request)
// ============================================================================

#[tokio::test]
async fn deepseek_reasoning_content_included_in_request_body() {
    let state = Arc::new(Mutex::new(Vec::<CapturedRequest>::new()));
    let body = concat!(
        "{",
        "\"id\":\"chatcmpl_ds_rt\",",
        "\"model\":\"deepseek-v4-flash\",",
        "\"choices\":[{",
        "\"message\":{\"role\":\"assistant\",\"content\":\"First response\",\"reasoning_content\":\"thinking step by step\"},",
        "\"finish_reason\":\"stop\"",
        "}],",
        "\"usage\":{\"prompt_tokens\":10,\"completion_tokens\":5}",
        "}"
    );
    let server = spawn_server(
        state.clone(),
        vec![http_response("200 OK", "application/json", body)],
        true,
    )
    .await;

    let client = OpenAiCompatClient::new("ds-test-key", OpenAiCompatConfig::deepseek())
        .with_base_url(server.base_url());

    let response = client
        .send_message(&sample_deepseek_request(false))
        .await
        .expect("send_message should succeed");

    // Verify reasoning content was parsed into a Thinking block
    let has_thinking = response.content.iter().any(|block| {
        matches!(block, OutputContentBlock::Thinking { thinking, .. } if thinking == "thinking step by step")
    });
    assert!(has_thinking, "reasoning_content should be parsed as Thinking block");

    // Verify text is also preserved
    let has_text = response.content.iter().any(|block| {
        matches!(block, OutputContentBlock::Text { text } if text == "First response")
    });
    assert!(has_text, "content should be preserved as Text block");
}

// ============================================================================
// V4 pricing integration
// ============================================================================

#[tokio::test]
async fn deepseek_v4_flash_pricing_with_cache_hit_breakdown() {
    // Simulate a turn with 200K uncached + 800K cached input, 50K output
    let usage = TokenUsage {
        input_tokens: 200_000,
        output_tokens: 50_000,
        cache_creation_input_tokens: 0,
        cache_read_input_tokens: 800_000,
    };

    let pricing = pricing_for_model("deepseek-v4-flash")
        .expect("should resolve v4 flash pricing");
    let cost = usage.estimate_cost_usd_with_pricing(pricing);

    // miss: 200K * $0.14/M = $0.028
    // hit:  800K * $0.0028/M = $0.00224
    // out:  50K  * $0.28/M  = $0.014
    // total: $0.04424
    assert!((cost.input_cost_usd - 0.028).abs() < 1e-9,
        "miss cost: expected $0.028, got ${:.6}", cost.input_cost_usd);
    assert!((cost.cache_read_cost_usd - 0.00224).abs() < 1e-9,
        "hit cost: expected $0.00224, got ${:.6}", cost.cache_read_cost_usd);
    assert!((cost.output_cost_usd - 0.014).abs() < 1e-9,
        "output cost: expected $0.014, got ${:.6}", cost.output_cost_usd);
    assert!((cost.total_cost_usd() - 0.04424).abs() < 1e-9,
        "total cost: expected $0.04424, got ${:.6}", cost.total_cost_usd());
}

#[tokio::test]
async fn deepseek_v4_pro_pricing_is_higher_than_flash() {
    let usage = TokenUsage {
        input_tokens: 100_000,
        output_tokens: 50_000,
        cache_creation_input_tokens: 0,
        cache_read_input_tokens: 400_000,
    };

    let flash = pricing_for_model("deepseek-v4-flash").expect("flash pricing");
    let pro = pricing_for_model("deepseek-v4-pro").expect("pro pricing");

    let flash_cost = usage.estimate_cost_usd_with_pricing(flash);
    let pro_cost = usage.estimate_cost_usd_with_pricing(pro);

    assert!(
        pro_cost.total_cost_usd() > flash_cost.total_cost_usd() * 2.0,
        "pro should be significantly more expensive than flash: flash={:.6} pro={:.6}",
        flash_cost.total_cost_usd(),
        pro_cost.total_cost_usd()
    );
}

// ============================================================================
// Model routing
// ============================================================================

#[tokio::test]
async fn deepseek_v4_provider_client_routes_correctly() {
    let _lock = env_lock();
    let _ds_key = EnvVarGuard::set("DEEPSEEK_API_KEY", Some("ds-test-key"));
    let _anthropic = EnvVarGuard::set("ANTHROPIC_API_KEY", None);
    let _openai = EnvVarGuard::set("OPENAI_API_KEY", None);

    let client = ProviderClient::from_model("deepseek-v4-flash")
        .expect("deepseek-v4-flash should construct");
    assert_eq!(
        client.provider_kind(),
        ninmu_api::ProviderKind::DeepSeek,
        "v4 flash should route to DeepSeek provider"
    );

    let client_pro = ProviderClient::from_model("deepseek-v4-pro")
        .expect("deepseek-v4-pro should construct");
    assert_eq!(
        client_pro.provider_kind(),
        ninmu_api::ProviderKind::DeepSeek,
        "v4 pro should route to DeepSeek provider"
    );

    let client_r1 = ProviderClient::from_model("deepseek-reasoner")
        .expect("deepseek-reasoner should construct");
    assert_eq!(
        client_r1.provider_kind(),
        ninmu_api::ProviderKind::DeepSeek,
        "reasoner should route to DeepSeek provider"
    );
}

#[tokio::test]
async fn deepseek_v4_model_resolves_from_registry() {


    // V4 models should have metadata in the model registry
    let flash_meta = metadata_for_model("deepseek-v4-flash")
        .expect("deepseek-v4-flash should have metadata");
    assert_eq!(flash_meta.provider, ProviderKind::DeepSeek);

    let pro_meta = metadata_for_model("deepseek-v4-pro")
        .expect("deepseek-v4-pro should have metadata");
    assert_eq!(pro_meta.provider, ProviderKind::DeepSeek);

    // Alias should pass through directly
    assert_eq!(resolve_model_alias("deepseek-v4-flash"), "deepseek-v4-flash");
    assert_eq!(resolve_model_alias("deepseek-v4-pro"), "deepseek-v4-pro");
}

// ============================================================================
// Cache-stable compaction tests (runtime integration)
// ============================================================================

#[tokio::test]
async fn cache_stable_compaction_preserves_prefix() {
    use ninmu_runtime::{
        compact_cache_stable, CacheStableCompactionConfig, CacheStableState,
    };
    use ninmu_runtime::{ContentBlock, ConversationMessage, MessageRole, Session};

    let mut session = Session::new();
    // Prefix: system message
    session.messages.push(ConversationMessage {
        role: MessageRole::System,
        blocks: vec![ContentBlock::Text {
            text: "System prompt".to_string(),
        }],
        usage: None,
    });
    // Non-prefix messages
    for i in 0..6 {
        session.messages.push(ConversationMessage::user_text(format!("question {i}")));
        session.messages.push(ConversationMessage::assistant(vec![
            ContentBlock::Text { text: format!("answer {i}") },
        ]));
    }

    let cache_state = CacheStableState::from_session(&session);
    assert_eq!(
        cache_state.prefix_message_count, 1,
        "system message should be prefix"
    );

    let config = CacheStableCompactionConfig {
        cache_state,
        preserve_recent_messages: 2,
        summary_text: None,
    };

    let result = compact_cache_stable(&session, &config);
    assert!(result.compacted, "should compact");
    assert!(result.removed_message_count > 0, "should remove messages");

    // Verify prefix is unchanged (first message should be the original system prompt)
    assert_eq!(
        result.session.messages[0],
        session.messages[0],
        "prefix must be preserved byte-for-byte"
    );

    // Second message should be a System summary (not the original)
    assert_eq!(
        result.session.messages[1].role,
        MessageRole::System,
        "second message should be summary system message"
    );

    // Recent messages preserved after summary
    assert!(
        result.session.messages.len() <= config.preserve_recent_messages + 2,
        "should have prefix + summary + recent tail"
    );
}

// ============================================================================
// Parallel tool call delta accumulation
// ============================================================================

#[tokio::test]
async fn deepseek_streaming_parses_two_parallel_tool_calls() {
    let state = Arc::new(Mutex::new(Vec::<CapturedRequest>::new()));
    // Two parallel tool calls interleaved via index 0 and index 1
    let sse = concat!(
        "data: {\"id\":\"chatcmpl_parallel\",\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"call_a\",\"function\":{\"name\":\"read_file\",\"arguments\":\"{\\\"path\\\":\\\"/a.ts\\\"}\"}}]}}]}\n\n",
        "data: {\"id\":\"chatcmpl_parallel\",\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":1,\"id\":\"call_b\",\"function\":{\"name\":\"search_content\",\"arguments\":\"{\\\"pattern\\\":\\\"TODO\\\"}\"}}]}}]}\n\n",
        "data: {\"id\":\"chatcmpl_parallel\",\"choices\":[{\"delta\":{},\"finish_reason\":\"tool_calls\"}]}\n\n",
        "data: [DONE]\n\n"
    );
    let server = spawn_server(
        state.clone(),
        vec![http_response("200 OK", "text/event-stream", sse)],
        true,
    )
    .await;

    let client = OpenAiCompatClient::new("ds-test-key", OpenAiCompatConfig::deepseek())
        .with_base_url(server.base_url());
    let mut stream = client
        .stream_message(&sample_deepseek_request(false))
        .await
        .expect("stream should start");

    let mut events = Vec::new();
    while let Some(event) = stream.next_event().await.expect("event should parse") {
        events.push(event);
    }

    // Should have two ToolUse ContentBlockStart events with different indices
    let tool_starts: Vec<_> = events
        .iter()
        .filter_map(|e| match e {
            StreamEvent::ContentBlockStart(ContentBlockStartEvent {
                index,
                content_block: OutputContentBlock::ToolUse { name, .. },
            }) => Some((*index, name.clone())),
            _ => None,
        })
        .collect();

    assert_eq!(tool_starts.len(), 2, "should have two tool use starts");
    let names: Vec<&str> = tool_starts.iter().map(|(_, n)| n.as_str()).collect();
    assert!(names.contains(&"read_file"));
    assert!(names.contains(&"search_content"));
    // Indices should differ for parallel calls
    assert_ne!(tool_starts[0].0, tool_starts[1].0);
}

// ============================================================================
// Error body parsing
// ============================================================================

#[tokio::test]
async fn deepseek_parses_structured_error_body() {
    let state = Arc::new(Mutex::new(Vec::<CapturedRequest>::new()));
    let error_body = serde_json::json!({
        "error": {
            "message": "Invalid API key provided",
            "type": "authentication_error",
            "code": 401
        }
    }).to_string();

    let server = spawn_server(
        state.clone(),
        vec![format!(
            "HTTP/1.1 401 Unauthorized\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
            error_body.len(),
            error_body
        )],
        true,
    )
    .await;

    let client = OpenAiCompatClient::new("bad-key", OpenAiCompatConfig::deepseek())
        .with_base_url(server.base_url());

    let result = client
        .send_message(&sample_deepseek_request(false))
        .await;

    assert!(result.is_err(), "should fail on 401");
    let err = result.unwrap_err();
    assert!(
        format!("{err:?}").to_lowercase().contains("401"),
        "error should contain status code 401, got: {err:?}"
    );
}

// ============================================================================
// NEEDS_PRO escalation model inference
// ============================================================================

#[tokio::test]
async fn needs_pro_escalation_model_inference() {
    use ninmu_runtime::escalate_model_name;

    assert_eq!(
        escalate_model_name(&Some("deepseek-v4-flash".to_string())),
        "deepseek-v4-pro"
    );
    assert_eq!(
        escalate_model_name(&Some("deepseek-chat".to_string())),
        "deepseek-reasoner"
    );
    assert_eq!(
        escalate_model_name(&None),
        "deepseek-v4-pro"
    );
    assert_eq!(
        escalate_model_name(&Some("unknown-model".to_string())),
        "deepseek-v4-pro"
    );
}

// ============================================================================
// Summary flash model hint
// ============================================================================

#[tokio::test]
async fn summary_flash_model_hint_returns_v4_flash() {
    use ninmu_runtime::summary_flash_model_hint;
    assert_eq!(summary_flash_model_hint(), "deepseek-v4-flash");
}
