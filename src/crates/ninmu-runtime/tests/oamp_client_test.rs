use ninmu_runtime::oamp_client::{OampClient, Provenance};
use substrate_types::{AgentId, MemoryGrant, MissionId, SensitivityClass, TaskId};

#[test]
fn oamp_client_builds_recall_request_with_sensitivity_ceiling() {
    let grant = MemoryGrant {
        sensitivity_ceiling: SensitivityClass::Internal,
        labels: vec!["project/*".into()],
    };

    let request = OampClient::build_recall_request("what is the auth pattern?", &grant);

    assert_eq!(request.sensitivity_ceiling, "internal");
    assert_eq!(request.label_scope, vec!["project/*"]);
}

#[test]
fn oamp_client_recall_request_includes_query() {
    let grant = MemoryGrant {
        sensitivity_ceiling: SensitivityClass::Public,
        labels: vec![],
    };

    let request = OampClient::build_recall_request("how does auth work?", &grant);

    assert_eq!(request.query, "how does auth work?");
}

#[test]
fn oamp_client_builds_write_request_with_provenance() {
    let agent_id = AgentId::new();
    let mission_id = MissionId::new();
    let task_id = TaskId::new();

    let request = OampClient::build_write_request(
        "learned that auth uses JWT",
        &Provenance {
            agent_id,
            mission_id,
            task_id,
        },
        SensitivityClass::Internal,
    );

    assert_eq!(request.content, "learned that auth uses JWT");
    assert_eq!(request.provenance.agent_id, agent_id.to_string());
    assert_eq!(request.provenance.mission_id, mission_id.to_string());
}

#[tokio::test]
async fn oamp_client_handles_connection_failure() {
    let client = OampClient::new(
        "http://127.0.0.1:1/nonexistent",
        MemoryGrant {
            sensitivity_ceiling: SensitivityClass::Internal,
            labels: vec![],
        },
    );

    let result = client.recall("test query").await;

    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("connection"));
}

#[test]
fn oamp_client_rejects_write_above_sensitivity_ceiling() {
    let grant = MemoryGrant {
        sensitivity_ceiling: SensitivityClass::Internal,
        labels: vec!["project/*".into()],
    };

    let result = OampClient::validate_write_sensitivity(SensitivityClass::Confidential, &grant);

    assert!(result.is_err());
}
