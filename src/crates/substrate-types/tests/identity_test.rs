use substrate_types::*;

#[test]
fn mission_id_display_has_prefix() {
    let id = MissionId::new();
    assert!(id.to_string().starts_with("mis_"));
}

#[test]
fn task_id_display_has_prefix() {
    let id = TaskId::new();
    assert!(id.to_string().starts_with("tsk_"));
}

#[test]
fn project_id_display_has_prefix() {
    let id = ProjectId::new();
    assert!(id.to_string().starts_with("prj_"));
}

#[test]
fn tenant_id_display_has_prefix() {
    let id = TenantId::new();
    assert!(id.to_string().starts_with("tnt_"));
}

#[test]
fn cell_id_display_has_prefix() {
    let id = CellId::new();
    assert!(id.to_string().starts_with("cel_"));
}

#[test]
fn agent_id_display_has_prefix() {
    let id = AgentId::new();
    assert!(id.to_string().starts_with("agt_"));
}

#[test]
fn mission_id_serde_roundtrip_is_plain_uuid() {
    let id = MissionId::new();
    let json = serde_json::to_string(&id).unwrap();
    assert!(!json.contains("mis_"), "wire format should be plain UUID");
    let back: MissionId = serde_json::from_str(&json).unwrap();
    assert_eq!(id, back);
}

#[test]
fn task_id_serde_roundtrip() {
    let id = TaskId::new();
    let json = serde_json::to_string(&id).unwrap();
    let back: TaskId = serde_json::from_str(&json).unwrap();
    assert_eq!(id, back);
}

#[test]
fn cell_id_serde_roundtrip() {
    let id = CellId::new();
    let json = serde_json::to_string(&id).unwrap();
    let back: CellId = serde_json::from_str(&json).unwrap();
    assert_eq!(id, back);
}

#[test]
fn agent_id_serde_roundtrip() {
    let id = AgentId::new();
    let json = serde_json::to_string(&id).unwrap();
    let back: AgentId = serde_json::from_str(&json).unwrap();
    assert_eq!(id, back);
}

#[test]
fn uuidv7_ids_are_time_ordered() {
    let a = MissionId::new();
    std::thread::sleep(std::time::Duration::from_millis(2));
    let b = MissionId::new();
    assert!(a.as_uuid() < b.as_uuid());
}

#[test]
fn different_id_types_equality_is_independent() {
    let m = MissionId::new();
    let json = serde_json::to_string(&m).unwrap();
    let t: TaskId = serde_json::from_str(&json).unwrap();
    assert_eq!(m.as_uuid(), t.as_uuid(), "same UUID value");
}
