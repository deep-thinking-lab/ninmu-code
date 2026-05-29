use ninmu_runtime::event_mapper::{EventContext, EventMapper, HarnessEvent};
use substrate_types::{
    AgentEvent, AgentId, CellId, CostEvent, EventKind, LifecycleEvent, MissionId, SubstrateEvent,
    TaskId, TenantId, VerificationEvent,
};

#[test]
fn maps_turn_start_to_prompt_sent() {
    let substrate_event = EventMapper::map(
        &HarnessEvent::TurnStart {
            prompt_tokens: 4096,
        },
        &event_context(),
    )
    .expect("event should map");

    match &substrate_event.kind {
        EventKind::Agent(AgentEvent::PromptSent { tokens }) => assert_eq!(tokens, &4096),
        other => panic!("expected PromptSent, got {other:?}"),
    }
}

#[test]
fn maps_tool_call_to_tool_call_started() {
    let substrate_event = EventMapper::map(
        &HarnessEvent::ToolCallStart {
            tool_name: "bash".into(),
            call_id: "call_abc123".into(),
        },
        &event_context(),
    )
    .expect("event should map");

    match &substrate_event.kind {
        EventKind::Agent(AgentEvent::ToolCallStarted {
            tool,
            idempotency_key,
        }) => {
            assert_eq!(tool, "bash");
            assert_eq!(idempotency_key, "call_abc123");
        }
        other => panic!("expected ToolCallStarted, got {other:?}"),
    }
}

#[test]
fn maps_tool_result_to_tool_call_completed() {
    let substrate_event = EventMapper::map(
        &HarnessEvent::ToolCallEnd {
            tool_name: "bash".into(),
            success: true,
            duration_ms: 1234,
        },
        &event_context(),
    )
    .expect("event should map");

    match &substrate_event.kind {
        EventKind::Agent(AgentEvent::ToolCallCompleted {
            tool,
            success,
            duration_ms,
        }) => {
            assert_eq!(tool, "bash");
            assert!(*success);
            assert_eq!(duration_ms, &1234);
        }
        other => panic!("expected ToolCallCompleted, got {other:?}"),
    }
}

#[test]
fn maps_test_pass_to_verification_command_passed() {
    let substrate_event = EventMapper::map(
        &HarnessEvent::AcceptanceTestResult {
            command: "cargo test --workspace".into(),
            exit_code: 0,
            duration_ms: 5000,
            stderr_tail: String::new(),
        },
        &event_context(),
    )
    .expect("event should map");

    match &substrate_event.kind {
        EventKind::Verification(VerificationEvent::CommandPassed {
            command,
            duration_ms,
        }) => {
            assert_eq!(command, "cargo test --workspace");
            assert_eq!(duration_ms, &5000);
        }
        other => panic!("expected CommandPassed, got {other:?}"),
    }
}

#[test]
fn maps_test_fail_to_verification_command_failed() {
    let substrate_event = EventMapper::map(
        &HarnessEvent::AcceptanceTestResult {
            command: "cargo test --workspace".into(),
            exit_code: 1,
            duration_ms: 3000,
            stderr_tail: "thread 'test' panicked".into(),
        },
        &event_context(),
    )
    .expect("event should map");

    match &substrate_event.kind {
        EventKind::Verification(VerificationEvent::CommandFailed {
            command,
            exit_code,
            stderr_tail,
        }) => {
            assert_eq!(command, "cargo test --workspace");
            assert_eq!(exit_code, &1);
            assert!(stderr_tail.contains("panicked"));
        }
        other => panic!("expected CommandFailed, got {other:?}"),
    }
}

#[test]
fn maps_token_usage_to_cost_event() {
    let substrate_event = EventMapper::map(
        &HarnessEvent::TokenUsage {
            input: 4096,
            output: 1024,
            cached: 2048,
            model: "anthropic/claude-sonnet-4".into(),
        },
        &event_context(),
    )
    .expect("event should map");

    match &substrate_event.kind {
        EventKind::Cost(CostEvent::ModelTokensUsed {
            input,
            output,
            cached,
            model,
        }) => {
            assert_eq!(input, &4096);
            assert_eq!(output, &1024);
            assert_eq!(cached, &2048);
            assert_eq!(model, "anthropic/claude-sonnet-4");
        }
        other => panic!("expected ModelTokensUsed, got {other:?}"),
    }
}

#[test]
fn maps_task_complete_to_lifecycle_completed() {
    let substrate_event = EventMapper::map(
        &HarnessEvent::TaskComplete {
            status: "completed".into(),
        },
        &event_context(),
    )
    .expect("event should map");

    match &substrate_event.kind {
        EventKind::Lifecycle(LifecycleEvent::Completed) => {}
        other => panic!("expected Completed, got {other:?}"),
    }
}

#[test]
fn mapped_events_carry_cell_context() {
    let ctx = event_context();
    let substrate_event = EventMapper::map(&HarnessEvent::TurnStart { prompt_tokens: 100 }, &ctx)
        .expect("event should map");

    assert_eq!(substrate_event.cell_id, ctx.cell_id);
    assert_eq!(substrate_event.mission_id, ctx.mission_id);
    assert_eq!(substrate_event.task_id, ctx.task_id);
    assert_eq!(substrate_event.tenant_id, ctx.tenant_id);
    assert_eq!(substrate_event.agent_id, ctx.agent_id);
}

#[test]
fn mapped_events_are_jsonl_compatible() {
    let events = [
        HarnessEvent::TurnStart { prompt_tokens: 100 },
        HarnessEvent::ToolCallStart {
            tool_name: "bash".into(),
            call_id: "c1".into(),
        },
        HarnessEvent::TaskComplete {
            status: "completed".into(),
        },
    ];
    let ctx = event_context();
    let jsonl = events
        .iter()
        .map(|event| {
            let substrate_event = EventMapper::map(event, &ctx).expect("event should map");
            serde_json::to_string(&substrate_event).expect("event should serialize")
        })
        .collect::<Vec<_>>()
        .join("\n");

    for line in jsonl.lines() {
        let _: SubstrateEvent = serde_json::from_str(line).expect("line should parse");
    }
}

fn event_context() -> EventContext {
    EventContext {
        cell_id: CellId::new(),
        mission_id: MissionId::new(),
        task_id: TaskId::new(),
        tenant_id: TenantId::new(),
        agent_id: AgentId::new(),
    }
}
