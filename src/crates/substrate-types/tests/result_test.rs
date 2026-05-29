use substrate_types::*;

fn make_result(status: CellStatus) -> FactoryCellResult {
    FactoryCellResult {
        cell_id: CellId::new(),
        mission_id: MissionId::new(),
        task_id: TaskId::new(),
        tenant_id: TenantId::new(),
        agent_id: AgentId::new(),
        status,
        verification: VerificationResult {
            commands: vec![CommandResult {
                command: "cargo test".into(),
                exit_code: 0,
                passed: true,
                duration_ms: 12345,
                stdout_tail: None,
                stderr_tail: None,
            }],
            all_passed: true,
            patch_file_count: 3,
            has_test_changes: true,
        },
        cost: CostSummary {
            input_tokens: 5000,
            output_tokens: 2000,
            cached_tokens: 1000,
            model_calls: 4,
            estimated_cost_usd: 0.03,
            runtime_seconds: 120,
            retries: 0,
        },
        artifacts: CellArtifacts {
            patch_bundle: Some(PatchBundle {
                base_ref: "main".into(),
                head_ref: "factory/auth-timeout".into(),
                diff_stat: "3 files changed, 42 insertions(+), 7 deletions(-)".into(),
                files_changed: 3,
                insertions: 42,
                deletions: 7,
            }),
            pr_ref: None,
            report: Some("Task completed successfully.".into()),
        },
        event_log_ref: "/cells/cel_xxx/events.jsonl".into(),
        replay_ref: Some("/cells/cel_xxx/replay".into()),
        completed_at: chrono::Utc::now(),
    }
}

#[test]
fn result_succeeded_roundtrip() {
    let result = make_result(CellStatus::Succeeded);
    let json = serde_json::to_string(&result).unwrap();
    let back: FactoryCellResult = serde_json::from_str(&json).unwrap();
    assert_eq!(back.status, CellStatus::Succeeded);
    assert!(back.verification.all_passed);
    assert_eq!(back.cost.model_calls, 4);
}

#[test]
fn result_failed_carries_reason() {
    let result = make_result(CellStatus::Failed {
        reason: "tests failed".into(),
    });
    let json = serde_json::to_string(&result).unwrap();
    let back: FactoryCellResult = serde_json::from_str(&json).unwrap();
    match back.status {
        CellStatus::Failed { reason } => assert_eq!(reason, "tests failed"),
        _ => panic!("expected Failed"),
    }
}

#[test]
fn result_timeout_roundtrip() {
    let result = make_result(CellStatus::Timeout);
    let json = serde_json::to_string(&result).unwrap();
    let back: FactoryCellResult = serde_json::from_str(&json).unwrap();
    assert_eq!(back.status, CellStatus::Timeout);
}

#[test]
fn result_approval_denied_carries_action_and_reason() {
    let result = make_result(CellStatus::ApprovalDenied {
        action: "deploy.production".into(),
        reason: "not approved by operator".into(),
    });
    let json = serde_json::to_string(&result).unwrap();
    let back: FactoryCellResult = serde_json::from_str(&json).unwrap();
    match back.status {
        CellStatus::ApprovalDenied { action, reason } => {
            assert_eq!(action, "deploy.production");
            assert_eq!(reason, "not approved by operator");
        }
        _ => panic!("expected ApprovalDenied"),
    }
}

#[test]
fn result_with_pr_ref() {
    let mut result = make_result(CellStatus::Succeeded);
    result.artifacts.pr_ref = Some(PrRef {
        provider: "kizuna".into(),
        url: "https://forge.substrate.dev/acme/backend/pulls/42".into(),
        number: 42,
        state: PrState::Draft,
    });
    let json = serde_json::to_string(&result).unwrap();
    let back: FactoryCellResult = serde_json::from_str(&json).unwrap();
    let pr = back.artifacts.pr_ref.unwrap();
    assert_eq!(pr.number, 42);
    assert!(matches!(pr.state, PrState::Draft));
}

#[test]
fn result_patch_bundle_stats() {
    let result = make_result(CellStatus::Succeeded);
    let bundle = result.artifacts.patch_bundle.as_ref().unwrap();
    assert_eq!(bundle.files_changed, 3);
    assert_eq!(bundle.insertions, 42);
    assert_eq!(bundle.deletions, 7);
}

#[test]
fn result_cost_summary_tracks_tokens() {
    let result = make_result(CellStatus::Succeeded);
    assert_eq!(result.cost.input_tokens, 5000);
    assert_eq!(result.cost.output_tokens, 2000);
    assert_eq!(result.cost.cached_tokens, 1000);
    assert!(result.cost.estimated_cost_usd > 0.0);
}
