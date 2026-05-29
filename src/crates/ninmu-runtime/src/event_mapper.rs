use substrate_types::{
    AgentEvent, AgentId, CellId, CostEvent, EventKind, LifecycleEvent, MissionId, SubstrateEvent,
    TaskId, TenantId, VerificationEvent,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventContext {
    pub cell_id: CellId,
    pub mission_id: MissionId,
    pub task_id: TaskId,
    pub tenant_id: TenantId,
    pub agent_id: AgentId,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HarnessEvent {
    TurnStart {
        prompt_tokens: u64,
    },
    ToolCallStart {
        tool_name: String,
        call_id: String,
    },
    ToolCallEnd {
        tool_name: String,
        success: bool,
        duration_ms: u64,
    },
    AcceptanceTestResult {
        command: String,
        exit_code: i32,
        duration_ms: u64,
        stderr_tail: String,
    },
    TokenUsage {
        input: u64,
        output: u64,
        cached: u64,
        model: String,
    },
    TaskComplete {
        status: String,
    },
}

pub struct EventMapper;

impl EventMapper {
    pub fn map(
        event: &HarnessEvent,
        ctx: &EventContext,
    ) -> Result<SubstrateEvent, EventMapperError> {
        let kind = match event {
            HarnessEvent::TurnStart { prompt_tokens } => EventKind::Agent(AgentEvent::PromptSent {
                tokens: *prompt_tokens,
            }),
            HarnessEvent::ToolCallStart { tool_name, call_id } => {
                EventKind::Agent(AgentEvent::ToolCallStarted {
                    tool: tool_name.clone(),
                    idempotency_key: call_id.clone(),
                })
            }
            HarnessEvent::ToolCallEnd {
                tool_name,
                success,
                duration_ms,
            } => EventKind::Agent(AgentEvent::ToolCallCompleted {
                tool: tool_name.clone(),
                success: *success,
                duration_ms: *duration_ms,
            }),
            HarnessEvent::AcceptanceTestResult {
                command,
                exit_code,
                duration_ms,
                stderr_tail,
            } if *exit_code == 0 => EventKind::Verification(VerificationEvent::CommandPassed {
                command: command.clone(),
                duration_ms: *duration_ms,
            }),
            HarnessEvent::AcceptanceTestResult {
                command,
                exit_code,
                stderr_tail,
                ..
            } => EventKind::Verification(VerificationEvent::CommandFailed {
                command: command.clone(),
                exit_code: *exit_code,
                stderr_tail: stderr_tail.clone(),
            }),
            HarnessEvent::TokenUsage {
                input,
                output,
                cached,
                model,
            } => EventKind::Cost(CostEvent::ModelTokensUsed {
                input: *input,
                output: *output,
                cached: *cached,
                model: model.clone(),
            }),
            HarnessEvent::TaskComplete { status } => EventKind::Lifecycle(match status.as_str() {
                "completed" => LifecycleEvent::Completed,
                "failed" => LifecycleEvent::Failed {
                    reason: "task failed".to_string(),
                },
                "cancelled" => LifecycleEvent::Cancelled {
                    reason: "task cancelled".to_string(),
                },
                other => return Err(EventMapperError::UnsupportedStatus(other.to_string())),
            }),
        };
        Ok(SubstrateEvent {
            event_id: uuid::Uuid::now_v7(),
            cell_id: ctx.cell_id,
            mission_id: ctx.mission_id,
            task_id: ctx.task_id,
            tenant_id: ctx.tenant_id,
            agent_id: ctx.agent_id,
            timestamp: chrono::Utc::now(),
            kind,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EventMapperError {
    UnsupportedStatus(String),
}

impl std::fmt::Display for EventMapperError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnsupportedStatus(status) => {
                write!(formatter, "unsupported task status: {status}")
            }
        }
    }
}

impl std::error::Error for EventMapperError {}
