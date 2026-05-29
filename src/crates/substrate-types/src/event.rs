use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::identity::*;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubstrateEvent {
    pub event_id: Uuid,
    pub cell_id: CellId,
    pub mission_id: MissionId,
    pub task_id: TaskId,
    pub tenant_id: TenantId,
    pub agent_id: AgentId,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub kind: EventKind,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "category", content = "payload")]
pub enum EventKind {
    Lifecycle(LifecycleEvent),
    Agent(AgentEvent),
    Verification(VerificationEvent),
    Governance(GovernanceEvent),
    Memory(MemoryEvent),
    Scm(ScmEvent),
    Cost(CostEvent),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LifecycleEvent {
    Scheduled,
    Starting,
    Ready,
    Running,
    Paused { reason: String },
    Completed,
    Failed { reason: String },
    Cancelled { reason: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AgentEvent {
    PromptSent {
        tokens: u64,
    },
    DecisionMade {
        summary: String,
    },
    ToolCallStarted {
        tool: String,
        idempotency_key: String,
    },
    ToolCallCompleted {
        tool: String,
        success: bool,
        duration_ms: u64,
    },
    ReflectionRecorded {
        content: String,
    },
    RetryTriggered {
        reason: String,
        attempt: u32,
    },
    LoopCompleted {
        steps: u32,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum VerificationEvent {
    CommandStarted {
        command: String,
    },
    CommandPassed {
        command: String,
        duration_ms: u64,
    },
    CommandFailed {
        command: String,
        exit_code: i32,
        stderr_tail: String,
    },
    ArtifactProduced {
        path: String,
        size_bytes: u64,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum GovernanceEvent {
    GrantIssued { capability: String, scope: String },
    ApprovalRequested { action: String, envelope_id: String },
    Approved { action: String, approver: String },
    Denied { action: String, reason: String },
    Expired { action: String },
    Resumed { action: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MemoryEvent {
    RecallRequested {
        query: String,
        label_scope: Vec<String>,
    },
    RecallFiltered {
        total: u32,
        returned: u32,
        filtered_by_sensitivity: u32,
    },
    MemoryWritten {
        entry_count: u32,
        provenance_agent_id: AgentId,
    },
    MemoryRejected {
        reason: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ScmEvent {
    BranchCreated { name: String },
    CommitProduced { sha: String, message: String },
    PrOpened { number: u64, url: String },
    CiObserved { status: String, url: Option<String> },
    ReviewAddressed { comment_id: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CostEvent {
    ModelTokensUsed {
        input: u64,
        output: u64,
        cached: u64,
        model: String,
    },
    RuntimeSecondsElapsed {
        seconds: u64,
    },
    RetryIncurred {
        tool: String,
        attempt: u32,
    },
}
