use substrate_types::*;
use uuid::Uuid;

#[test]
fn manifest_ids_appear_in_result() {
    let yaml = include_str!("manifest_fixtures/valid_minimal.yaml");
    let manifest: FactoryCellManifest = serde_yaml::from_str(yaml).unwrap();

    let result = FactoryCellResult {
        cell_id: manifest.metadata.cell_id,
        mission_id: manifest.metadata.mission_id,
        task_id: manifest.metadata.task_id,
        tenant_id: manifest.metadata.tenant_id,
        agent_id: AgentId::new(),
        status: CellStatus::Succeeded,
        verification: VerificationResult {
            commands: vec![],
            all_passed: true,
            patch_file_count: 1,
            has_test_changes: true,
        },
        cost: CostSummary {
            input_tokens: 1000,
            output_tokens: 500,
            cached_tokens: 0,
            model_calls: 1,
            estimated_cost_usd: 0.01,
            runtime_seconds: 60,
            retries: 0,
        },
        artifacts: CellArtifacts {
            patch_bundle: None,
            pr_ref: None,
            report: None,
        },
        event_log_ref: "/cells/events.jsonl".into(),
        replay_ref: None,
        completed_at: chrono::Utc::now(),
    };

    assert_eq!(result.cell_id, manifest.metadata.cell_id);
    assert_eq!(result.mission_id, manifest.metadata.mission_id);
    assert_eq!(result.task_id, manifest.metadata.task_id);
    assert_eq!(result.tenant_id, manifest.metadata.tenant_id);
}

#[test]
fn manifest_ids_appear_in_events() {
    let yaml = include_str!("manifest_fixtures/valid_minimal.yaml");
    let manifest: FactoryCellManifest = serde_yaml::from_str(yaml).unwrap();

    let event = SubstrateEvent {
        event_id: Uuid::now_v7(),
        cell_id: manifest.metadata.cell_id,
        mission_id: manifest.metadata.mission_id,
        task_id: manifest.metadata.task_id,
        tenant_id: manifest.metadata.tenant_id,
        agent_id: AgentId::new(),
        timestamp: chrono::Utc::now(),
        kind: EventKind::Lifecycle(LifecycleEvent::Starting),
    };

    assert_eq!(event.cell_id, manifest.metadata.cell_id);
    assert_eq!(event.mission_id, manifest.metadata.mission_id);
    assert_eq!(event.task_id, manifest.metadata.task_id);
    assert_eq!(event.tenant_id, manifest.metadata.tenant_id);
}

#[test]
fn full_lifecycle_manifest_to_result_to_events() {
    let yaml = include_str!("manifest_fixtures/valid_full.yaml");
    let manifest: FactoryCellManifest = serde_yaml::from_str(yaml).unwrap();
    ManifestValidation::validate(&manifest).unwrap();

    let agent_id = manifest
        .metadata
        .agent_id
        .expect("valid_full fixture should contain metadata.agent_id");

    let events: Vec<SubstrateEvent> = vec![
        SubstrateEvent {
            event_id: Uuid::now_v7(),
            cell_id: manifest.metadata.cell_id,
            mission_id: manifest.metadata.mission_id,
            task_id: manifest.metadata.task_id,
            tenant_id: manifest.metadata.tenant_id,
            agent_id,
            timestamp: chrono::Utc::now(),
            kind: EventKind::Lifecycle(LifecycleEvent::Scheduled),
        },
        SubstrateEvent {
            event_id: Uuid::now_v7(),
            cell_id: manifest.metadata.cell_id,
            mission_id: manifest.metadata.mission_id,
            task_id: manifest.metadata.task_id,
            tenant_id: manifest.metadata.tenant_id,
            agent_id,
            timestamp: chrono::Utc::now(),
            kind: EventKind::Lifecycle(LifecycleEvent::Starting),
        },
        SubstrateEvent {
            event_id: Uuid::now_v7(),
            cell_id: manifest.metadata.cell_id,
            mission_id: manifest.metadata.mission_id,
            task_id: manifest.metadata.task_id,
            tenant_id: manifest.metadata.tenant_id,
            agent_id,
            timestamp: chrono::Utc::now(),
            kind: EventKind::Verification(VerificationEvent::CommandPassed {
                command: "cargo test --workspace".into(),
                duration_ms: 5000,
            }),
        },
        SubstrateEvent {
            event_id: Uuid::now_v7(),
            cell_id: manifest.metadata.cell_id,
            mission_id: manifest.metadata.mission_id,
            task_id: manifest.metadata.task_id,
            tenant_id: manifest.metadata.tenant_id,
            agent_id,
            timestamp: chrono::Utc::now(),
            kind: EventKind::Lifecycle(LifecycleEvent::Completed),
        },
    ];

    let jsonl: String = events
        .iter()
        .map(|e| serde_json::to_string(e).unwrap())
        .collect::<Vec<_>>()
        .join("\n");

    for line in jsonl.lines() {
        let parsed: SubstrateEvent = serde_json::from_str(line).unwrap();
        assert_eq!(parsed.cell_id, manifest.metadata.cell_id);
        assert_eq!(parsed.mission_id, manifest.metadata.mission_id);
        assert_eq!(parsed.task_id, manifest.metadata.task_id);
        assert_eq!(parsed.tenant_id, manifest.metadata.tenant_id);
        assert_eq!(parsed.agent_id, agent_id);
    }

    let result = FactoryCellResult {
        cell_id: manifest.metadata.cell_id,
        mission_id: manifest.metadata.mission_id,
        task_id: manifest.metadata.task_id,
        tenant_id: manifest.metadata.tenant_id,
        agent_id,
        status: CellStatus::Succeeded,
        verification: VerificationResult {
            commands: vec![CommandResult {
                command: "cargo test --workspace".into(),
                exit_code: 0,
                passed: true,
                duration_ms: 5000,
                stdout_tail: None,
                stderr_tail: None,
            }],
            all_passed: true,
            patch_file_count: 5,
            has_test_changes: true,
        },
        cost: CostSummary {
            input_tokens: 50000,
            output_tokens: 20000,
            cached_tokens: 10000,
            model_calls: 12,
            estimated_cost_usd: 0.15,
            runtime_seconds: 300,
            retries: 1,
        },
        artifacts: CellArtifacts {
            patch_bundle: Some(PatchBundle {
                base_ref: "main".into(),
                head_ref: "factory/payment-webhook".into(),
                diff_stat: "5 files changed, 200 insertions(+), 30 deletions(-)".into(),
                files_changed: 5,
                insertions: 200,
                deletions: 30,
            }),
            pr_ref: Some(PrRef {
                provider: "kizuna".into(),
                url: "https://forge.substrate.dev/acme/payments-service/pulls/17".into(),
                number: 17,
                state: PrState::Draft,
            }),
            report: Some("Payment webhook handler implemented with exponential retry.".into()),
        },
        event_log_ref: "/cells/cel_xxx/events.jsonl".into(),
        replay_ref: Some("/cells/cel_xxx/replay".into()),
        completed_at: chrono::Utc::now(),
    };

    let json = serde_json::to_string(&result).unwrap();
    let back: FactoryCellResult = serde_json::from_str(&json).unwrap();
    assert_eq!(back.cell_id, manifest.metadata.cell_id);
    assert_eq!(back.mission_id, manifest.metadata.mission_id);
    assert_eq!(back.task_id, manifest.metadata.task_id);
    assert_eq!(back.tenant_id, manifest.metadata.tenant_id);
    assert_eq!(back.agent_id, agent_id);
    assert_eq!(back.status, CellStatus::Succeeded);
}
