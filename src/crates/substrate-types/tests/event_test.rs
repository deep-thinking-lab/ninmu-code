use substrate_types::*;
use uuid::Uuid;

fn make_event(kind: EventKind) -> SubstrateEvent {
    SubstrateEvent {
        event_id: Uuid::now_v7(),
        cell_id: CellId::new(),
        mission_id: MissionId::new(),
        task_id: TaskId::new(),
        tenant_id: TenantId::new(),
        agent_id: AgentId::new(),
        timestamp: chrono::Utc::now(),
        kind,
    }
}

#[test]
fn lifecycle_event_roundtrip() {
    let event = make_event(EventKind::Lifecycle(LifecycleEvent::Starting));
    let json = serde_json::to_string(&event).unwrap();
    let back: SubstrateEvent = serde_json::from_str(&json).unwrap();
    assert!(matches!(
        back.kind,
        EventKind::Lifecycle(LifecycleEvent::Starting)
    ));
}

#[test]
fn agent_event_tool_call_has_idempotency_key() {
    let event = make_event(EventKind::Agent(AgentEvent::ToolCallStarted {
        tool: "shell.test".into(),
        idempotency_key: "idk_abc123".into(),
    }));
    let json = serde_json::to_string(&event).unwrap();
    assert!(json.contains("idk_abc123"));
    let back: SubstrateEvent = serde_json::from_str(&json).unwrap();
    match back.kind {
        EventKind::Agent(AgentEvent::ToolCallStarted {
            idempotency_key, ..
        }) => {
            assert_eq!(idempotency_key, "idk_abc123");
        }
        _ => panic!("wrong event kind"),
    }
}

#[test]
fn governance_event_denied_has_reason() {
    let event = make_event(EventKind::Governance(GovernanceEvent::Denied {
        action: "deploy.production".into(),
        reason: "policy violation".into(),
    }));
    let json = serde_json::to_string(&event).unwrap();
    assert!(json.contains("policy violation"));
}

#[test]
fn memory_event_recall_filtered_counts() {
    let event = make_event(EventKind::Memory(MemoryEvent::RecallFiltered {
        total: 50,
        returned: 30,
        filtered_by_sensitivity: 20,
    }));
    let json = serde_json::to_string(&event).unwrap();
    let back: SubstrateEvent = serde_json::from_str(&json).unwrap();
    match back.kind {
        EventKind::Memory(MemoryEvent::RecallFiltered {
            total,
            returned,
            filtered_by_sensitivity,
        }) => {
            assert_eq!(total, 50);
            assert_eq!(returned, 30);
            assert_eq!(filtered_by_sensitivity, 20);
        }
        _ => panic!("wrong event kind"),
    }
}

#[test]
fn scm_event_pr_opened() {
    let event = make_event(EventKind::Scm(ScmEvent::PrOpened {
        number: 42,
        url: "https://forge.substrate.dev/acme/backend/pulls/42".into(),
    }));
    let json = serde_json::to_string(&event).unwrap();
    let back: SubstrateEvent = serde_json::from_str(&json).unwrap();
    match back.kind {
        EventKind::Scm(ScmEvent::PrOpened { number, url }) => {
            assert_eq!(number, 42);
            assert!(url.contains("pulls/42"));
        }
        _ => panic!("wrong event kind"),
    }
}

#[test]
fn cost_event_model_tokens() {
    let event = make_event(EventKind::Cost(CostEvent::ModelTokensUsed {
        input: 5000,
        output: 2000,
        cached: 1000,
        model: "claude-sonnet-4-6".into(),
    }));
    let json = serde_json::to_string(&event).unwrap();
    let back: SubstrateEvent = serde_json::from_str(&json).unwrap();
    match back.kind {
        EventKind::Cost(CostEvent::ModelTokensUsed {
            input,
            output,
            cached,
            model,
        }) => {
            assert_eq!(input, 5000);
            assert_eq!(output, 2000);
            assert_eq!(cached, 1000);
            assert_eq!(model, "claude-sonnet-4-6");
        }
        _ => panic!("wrong event kind"),
    }
}

#[test]
fn events_are_jsonl_compatible() {
    let events = [
        make_event(EventKind::Lifecycle(LifecycleEvent::Scheduled)),
        make_event(EventKind::Lifecycle(LifecycleEvent::Starting)),
        make_event(EventKind::Agent(AgentEvent::PromptSent { tokens: 1000 })),
        make_event(EventKind::Verification(VerificationEvent::CommandStarted {
            command: "cargo test".into(),
        })),
        make_event(EventKind::Lifecycle(LifecycleEvent::Completed)),
    ];
    let jsonl: String = events
        .iter()
        .map(|e| serde_json::to_string(e).unwrap())
        .collect::<Vec<_>>()
        .join("\n");
    for line in jsonl.lines() {
        let _: SubstrateEvent = serde_json::from_str(line).unwrap();
    }
}

#[test]
fn verification_event_failed_carries_stderr() {
    let event = make_event(EventKind::Verification(VerificationEvent::CommandFailed {
        command: "cargo test".into(),
        exit_code: 1,
        stderr_tail: "thread 'main' panicked".into(),
    }));
    let json = serde_json::to_string(&event).unwrap();
    assert!(json.contains("panicked"));
}
