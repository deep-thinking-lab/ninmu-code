use ninmu_runtime::oamp_client::{MemoryEntry, OampClient, Provenance};
use ninmu_tools::{RecallMemoryTool, StoreMemoryTool};
use serde_json::json;
use substrate_types::{AgentId, MemoryGrant, MissionId, SensitivityClass, TaskId};

#[tokio::test]
async fn recall_memory_tool_returns_entries() {
    let tool = RecallMemoryTool::new(mock_oamp_client());

    let output = tool
        .execute(json!({"query": "auth timeout pattern"}))
        .await
        .expect("recall should succeed");

    assert!(output.contains("auth"));
}

#[tokio::test]
async fn recall_memory_tool_includes_provenance() {
    let tool = RecallMemoryTool::new(mock_oamp_client());

    let output = tool
        .execute(json!({"query": "auth pattern"}))
        .await
        .expect("recall should succeed");

    assert!(output.contains("provenance"));
}

#[tokio::test]
async fn store_memory_tool_writes_with_provenance() {
    let tool = StoreMemoryTool::new(mock_oamp_client(), test_provenance());

    let result = tool
        .execute(json!({
            "content": "learned that auth uses JWT with 30min expiry",
            "sensitivity": "internal"
        }))
        .await;

    assert!(result.is_ok());
}

#[tokio::test]
async fn store_memory_tool_rejects_above_ceiling() {
    let tool = StoreMemoryTool::new(
        oamp_client_with_grant(SensitivityClass::Internal),
        test_provenance(),
    );

    let result = tool
        .execute(json!({
            "content": "secret key: sk-test",
            "sensitivity": "confidential"
        }))
        .await;

    assert!(result.is_err());
}

fn mock_oamp_client() -> OampClient {
    OampClient::mock(
        MemoryGrant {
            sensitivity_ceiling: SensitivityClass::Internal,
            labels: vec!["project/*".into()],
        },
        vec![MemoryEntry {
            content: "auth timeout pattern uses retry jitter".to_string(),
            provenance: json!({"source": "test"}),
        }],
    )
}

fn oamp_client_with_grant(sensitivity_ceiling: SensitivityClass) -> OampClient {
    OampClient::mock(
        MemoryGrant {
            sensitivity_ceiling,
            labels: vec![],
        },
        vec![],
    )
}

fn test_provenance() -> Provenance {
    Provenance {
        agent_id: AgentId::new(),
        mission_id: MissionId::new(),
        task_id: TaskId::new(),
    }
}
