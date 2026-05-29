use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::run_task::EventFormat;
use ninmu_runtime::event_mapper::{EventContext, EventMapper};
use ninmu_runtime::harness_contract::{
    HarnessEvent, HarnessEventKind, HarnessProtocolVersion, HarnessTaskRequest,
    MAX_EVENT_PAYLOAD_BYTES,
};
use serde_json::{json, Value};
use substrate_types::{
    AgentId, CellId, EventKind, LifecycleEvent, MissionId, SubstrateEvent, TaskId, TenantId,
};

pub(crate) struct TaskEventSink {
    writer: Option<File>,
    event_log: Option<PathBuf>,
    sequence: u64,
    format: EventFormat,
    substrate_context: Option<EventContext>,
}

impl TaskEventSink {
    pub(crate) fn disabled() -> Self {
        Self {
            writer: None,
            event_log: None,
            sequence: 0,
            format: EventFormat::Native,
            substrate_context: None,
        }
    }

    pub(crate) fn file(
        path: PathBuf,
        format: EventFormat,
        request: &HarnessTaskRequest,
    ) -> io::Result<Self> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        Ok(Self {
            writer: Some(File::create(&path)?),
            event_log: Some(path),
            sequence: 0,
            format,
            substrate_context: extract_event_context(request),
        })
    }

    pub(crate) fn emit(
        &mut self,
        mission_id: &str,
        task_id: &str,
        kind: &str,
        payload: Value,
    ) -> io::Result<()> {
        if self.writer.is_none() {
            return Ok(());
        }
        self.sequence += 1;
        let payload = self.bound_payload(task_id, kind, payload)?;
        if self.format == EventFormat::Substrate && self.substrate_context.is_some() {
            return self.emit_substrate(kind, payload);
        }
        let event = HarnessEvent {
            protocol: HarnessProtocolVersion::V1Alpha1,
            mission_id: mission_id.to_string(),
            task_id: task_id.to_string(),
            event_id: format!("{task_id}-event-{}", self.sequence),
            sequence: self.sequence,
            timestamp: unix_timestamp_string(),
            kind: HarnessEventKind::new(kind.to_string()),
            payload,
        };
        let writer = self.writer.as_mut().expect("writer checked");
        serde_json::to_writer(&mut *writer, &event)?;
        writer.write_all(b"\n")
    }

    fn emit_substrate(&mut self, kind: &str, payload: Value) -> io::Result<()> {
        let Some(ctx) = &self.substrate_context else {
            return Ok(());
        };
        let event = match map_native_to_substrate(kind, &payload, ctx) {
            Some(event) => event,
            None => return Ok(()),
        };
        let writer = self.writer.as_mut().expect("writer checked");
        serde_json::to_writer(&mut *writer, &event)?;
        writer.write_all(b"\n")
    }

    fn bound_payload(&self, task_id: &str, kind: &str, payload: Value) -> io::Result<Value> {
        let size = serde_json::to_vec(&payload)?.len();
        if size <= MAX_EVENT_PAYLOAD_BYTES {
            return Ok(payload);
        }
        let Some(event_log) = &self.event_log else {
            return Ok(json!({"omitted": "payload exceeded inline event limit"}));
        };
        let artifact_dir = event_log.with_extension("artifacts");
        fs::create_dir_all(&artifact_dir)?;
        let artifact_path = artifact_dir.join(format!(
            "{}-{}-{}.json",
            safe_filename_part(task_id),
            kind.replace('.', "-"),
            self.sequence
        ));
        fs::write(&artifact_path, serde_json::to_vec(&payload)?)?;
        Ok(json!({
            "artifact": {
                "path": display_path(&artifact_path),
                "kind": "event_payload",
                "description": "event payload exceeded inline limit"
            }
        }))
    }
}

fn map_native_to_substrate(
    kind: &str,
    payload: &Value,
    ctx: &EventContext,
) -> Option<SubstrateEvent> {
    let mapped = match kind {
        "task.started" => {
            return Some(substrate_event(
                ctx,
                EventKind::Lifecycle(LifecycleEvent::Starting),
            ));
        }
        "turn.started" => ninmu_runtime::event_mapper::HarnessEvent::TurnStart {
            prompt_tokens: payload
                .get("prompt_tokens")
                .and_then(Value::as_u64)
                .unwrap_or(0),
        },
        "tool.started" => ninmu_runtime::event_mapper::HarnessEvent::ToolCallStart {
            tool_name: payload
                .get("name")
                .or_else(|| payload.get("tool"))
                .and_then(Value::as_str)
                .unwrap_or("unknown")
                .to_string(),
            call_id: payload
                .get("id")
                .or_else(|| payload.get("call_id"))
                .and_then(Value::as_str)
                .unwrap_or("unknown")
                .to_string(),
        },
        "tool.completed" => ninmu_runtime::event_mapper::HarnessEvent::ToolCallEnd {
            tool_name: payload
                .get("name")
                .or_else(|| payload.get("tool"))
                .and_then(Value::as_str)
                .unwrap_or("unknown")
                .to_string(),
            success: payload
                .get("success")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            duration_ms: payload
                .get("duration_ms")
                .and_then(Value::as_u64)
                .unwrap_or(0),
        },
        "test.completed" => {
            let exit_code = payload
                .get("exit_code")
                .and_then(Value::as_i64)
                .unwrap_or_else(|| {
                    i64::from(payload.get("status").and_then(Value::as_str) != Some("passed"))
                });
            ninmu_runtime::event_mapper::HarnessEvent::AcceptanceTestResult {
                command: payload
                    .get("command")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
                exit_code: i32::try_from(exit_code).unwrap_or(1),
                duration_ms: payload
                    .get("duration_ms")
                    .and_then(Value::as_u64)
                    .unwrap_or(0),
                stderr_tail: payload
                    .get("stderr_tail")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
            }
        }
        "task.completed" => ninmu_runtime::event_mapper::HarnessEvent::TaskComplete {
            status: "completed".to_string(),
        },
        "task.failed" => ninmu_runtime::event_mapper::HarnessEvent::TaskComplete {
            status: "failed".to_string(),
        },
        "task.cancelled" => ninmu_runtime::event_mapper::HarnessEvent::TaskComplete {
            status: "cancelled".to_string(),
        },
        _ => return None,
    };
    EventMapper::map(&mapped, ctx).ok()
}

fn substrate_event(ctx: &EventContext, kind: EventKind) -> SubstrateEvent {
    SubstrateEvent {
        event_id: uuid::Uuid::now_v7(),
        cell_id: ctx.cell_id,
        mission_id: ctx.mission_id,
        task_id: ctx.task_id,
        tenant_id: ctx.tenant_id,
        agent_id: ctx.agent_id,
        timestamp: chrono::Utc::now(),
        kind,
    }
}

fn extract_event_context(request: &HarnessTaskRequest) -> Option<EventContext> {
    let substrate = request
        .project_profile
        .as_ref()?
        .get("substrate")?
        .as_object()?;
    Some(EventContext {
        cell_id: CellId::from_uuid(parse_prefixed_uuid(substrate.get("cell_id")?.as_str()?)?),
        mission_id: MissionId::from_uuid(parse_prefixed_uuid(&request.mission_id)?),
        task_id: TaskId::from_uuid(parse_prefixed_uuid(&request.task_id)?),
        tenant_id: TenantId::from_uuid(parse_prefixed_uuid(substrate.get("tenant_id")?.as_str()?)?),
        agent_id: substrate
            .get("agent_id")
            .and_then(Value::as_str)
            .and_then(parse_prefixed_uuid)
            .map_or_else(AgentId::new, AgentId::from_uuid),
    })
}

fn parse_prefixed_uuid(value: &str) -> Option<uuid::Uuid> {
    let uuid = value.rsplit_once('_').map_or(value, |(_, uuid)| uuid);
    uuid::Uuid::parse_str(uuid).ok()
}

fn unix_timestamp_string() -> String {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs());
    seconds.to_string()
}

fn display_path(path: &Path) -> String {
    path.display().to_string()
}

fn safe_filename_part(value: &str) -> String {
    let sanitized = value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.') {
                ch
            } else {
                '_'
            }
        })
        .collect::<String>();
    if sanitized.is_empty() || sanitized == "." || sanitized == ".." {
        "task".to_string()
    } else {
        sanitized
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use serde_json::json;

    use crate::run_task::EventFormat;

    use super::TaskEventSink;

    #[test]
    fn oversized_payload_is_written_as_artifact_reference() {
        let root = std::env::temp_dir().join(format!("ninmu-event-sink-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).expect("temp root should exist");
        let event_log = root.join("events.ndjson");
        let malicious_task_id = "../evil/task";
        let request = ninmu_runtime::harness_contract::HarnessTaskRequest {
            protocol: ninmu_runtime::harness_contract::HarnessProtocolVersion::V1Alpha1,
            mission_id: "mission".to_string(),
            task_id: malicious_task_id.to_string(),
            objective: "test".to_string(),
            workdir: root.display().to_string(),
            model: None,
            permission_mode: None,
            allowed_tools: Vec::new(),
            acceptance_tests: Vec::new(),
            timeout_seconds: None,
            sandbox: None,
            skill_profile: None,
            project_profile: None,
            previous_context: None,
        };
        let mut sink = TaskEventSink::file(event_log.clone(), EventFormat::Native, &request)
            .expect("sink should open");

        sink.emit(
            "mission",
            malicious_task_id,
            "tool.completed",
            json!({"output": "x".repeat(70 * 1024)}),
        )
        .expect("event should emit");

        let line = fs::read_to_string(&event_log).expect("event log should exist");
        let event: serde_json::Value =
            serde_json::from_str(line.trim()).expect("event should parse");
        let artifact = event["payload"]["artifact"]["path"]
            .as_str()
            .expect("artifact path should exist");
        let artifact_path = std::path::Path::new(artifact);
        assert!(artifact_path.exists());
        let artifact_dir = event_log.with_extension("artifacts");
        assert_eq!(artifact_path.parent(), Some(artifact_dir.as_path()));
        let file_name = artifact_path
            .file_name()
            .and_then(|name| name.to_str())
            .expect("artifact filename should be valid utf-8");
        assert!(file_name.ends_with("-tool-completed-1.json"));
        assert!(!file_name.contains('/'));
    }
}
