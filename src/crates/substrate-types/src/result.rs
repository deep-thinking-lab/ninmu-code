use serde::{Deserialize, Serialize};

use crate::identity::*;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FactoryCellResult {
    pub cell_id: CellId,
    pub mission_id: MissionId,
    pub task_id: TaskId,
    pub tenant_id: TenantId,
    pub agent_id: AgentId,
    pub status: CellStatus,
    pub verification: VerificationResult,
    pub cost: CostSummary,
    pub artifacts: CellArtifacts,
    pub event_log_ref: String,
    pub replay_ref: Option<String>,
    pub completed_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum CellStatus {
    Succeeded,
    Failed { reason: String },
    Cancelled { reason: String },
    Timeout,
    ApprovalDenied { action: String, reason: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationResult {
    pub commands: Vec<CommandResult>,
    pub all_passed: bool,
    pub patch_file_count: u32,
    pub has_test_changes: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandResult {
    pub command: String,
    pub exit_code: i32,
    pub passed: bool,
    pub duration_ms: u64,
    pub stdout_tail: Option<String>,
    pub stderr_tail: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CostSummary {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cached_tokens: u64,
    pub model_calls: u32,
    pub estimated_cost_usd: f64,
    pub runtime_seconds: u64,
    pub retries: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CellArtifacts {
    pub patch_bundle: Option<PatchBundle>,
    pub pr_ref: Option<PrRef>,
    pub report: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatchBundle {
    pub base_ref: String,
    pub head_ref: String,
    pub diff_stat: String,
    pub files_changed: u32,
    pub insertions: u32,
    pub deletions: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrRef {
    pub provider: String,
    pub url: String,
    pub number: u64,
    pub state: PrState,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PrState {
    Draft,
    Open,
    Merged,
    Closed,
}
