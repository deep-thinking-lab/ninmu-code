use serde::{Deserialize, Serialize};

use crate::identity::*;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FactoryCellManifest {
    pub api_version: String,
    pub kind: ManifestKind,
    pub metadata: CellMetadata,
    pub spec: CellSpec,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ManifestKind {
    FactoryCell,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CellMetadata {
    pub mission_id: MissionId,
    pub task_id: TaskId,
    pub project_id: ProjectId,
    pub tenant_id: TenantId,
    pub cell_id: CellId,
    /// Populated at launch time by Ultra, not at manifest emission time.
    /// Ninmu emits the manifest without this field; Voxeltron sets it after
    /// Ultra provisions the ephemeral agent identity.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<AgentId>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CellSpec {
    pub objective: String,
    pub repository: RepositorySpec,
    pub runtime: RuntimeSpec,
    pub agent: AgentSpec,
    pub memory: MemorySpec,
    pub tools: ToolSpec,
    pub verification: VerificationSpec,
    pub output: OutputSpec,
}

// --- Repository ---

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "provider")]
pub enum RepositorySpec {
    #[serde(rename = "kizuna")]
    Kizuna {
        instance: String,
        org: String,
        repo: String,
        base_ref: String,
        working_ref: String,
        agent_identity: AgentIdentitySpec,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentIdentitySpec {
    pub trust_level: TrustLevel,
    pub scopes: Vec<AgentScope>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum TrustLevel {
    Untrusted = 0,
    Basic = 1,
    Standard = 2,
    Elevated = 3,
    Trusted = 4,
}

impl Serialize for TrustLevel {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_u8(*self as u8)
    }
}

impl<'de> Deserialize<'de> for TrustLevel {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = u8::deserialize(deserializer)?;
        match value {
            0 => Ok(Self::Untrusted),
            1 => Ok(Self::Basic),
            2 => Ok(Self::Standard),
            3 => Ok(Self::Elevated),
            4 => Ok(Self::Trusted),
            other => Err(serde::de::Error::custom(format!(
                "invalid trust level: {other}, expected 0-4"
            ))),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AgentScope {
    Read,
    Write,
    Merge,
    Deploy,
}

// --- Runtime ---

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeSpec {
    pub isolation: IsolationMode,
    pub image: String,
    pub cpu: u32,
    pub memory_mb: u32,
    pub timeout_ms: u64,
    pub network_policy: NetworkPolicy,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum IsolationMode {
    #[serde(rename = "microvm")]
    MicroVm,
    #[serde(rename = "docker")]
    Docker,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum NetworkPolicy {
    #[serde(rename = "restricted")]
    Restricted,
    #[serde(rename = "allowlist")]
    Allowlist { hosts: Vec<String> },
    #[serde(rename = "unrestricted")]
    Unrestricted,
}

// --- Agent ---

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentSpec {
    pub harness: HarnessKind,
    pub model_policy: ModelPolicy,
    pub max_steps: u32,
    pub max_prompt_tokens_per_call: u32,
    pub max_total_tokens: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum HarnessKind {
    #[serde(rename = "ninmu-code", alias = "cosmictron")]
    NinmuCode,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ModelPolicy {
    #[serde(rename = "cost_aware")]
    CostAware,
    #[serde(rename = "quality_first")]
    QualityFirst,
    #[serde(rename = "fixed")]
    Fixed { model: String },
}

// --- Memory ---

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemorySpec {
    pub peer_id: String,
    pub protocol: MemoryProtocol,
    pub required_capabilities: MemoryCapabilities,
    pub grant: MemoryGrant,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MemoryProtocol {
    #[serde(rename = "oamp")]
    Oamp,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryCapabilities {
    pub governance: bool,
    pub provenance: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryGrant {
    pub sensitivity_ceiling: SensitivityClass,
    pub labels: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum SensitivityClass {
    #[serde(rename = "public")]
    Public,
    #[serde(rename = "internal")]
    Internal,
    #[serde(rename = "confidential")]
    Confidential,
    #[serde(rename = "restricted")]
    Restricted,
}

// --- Tools ---

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolSpec {
    pub allowed: Vec<ToolClass>,
    pub approval_required: Vec<ToolClass>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ToolClass(pub String);

impl ToolClass {
    pub const SHELL_READONLY: &str = "shell.readonly";
    pub const SHELL_TEST: &str = "shell.test";
    pub const SHELL_WRITE: &str = "shell.write";
    pub const GIT_BRANCH: &str = "git.branch";
    pub const GIT_DIFF: &str = "git.diff";
    pub const GIT_COMMIT: &str = "git.commit";
    pub const GIT_PUSH: &str = "git.push";
    pub const SCM_OPEN_PR: &str = "scm.open_pr";
    pub const DEPLOY_PRODUCTION: &str = "deploy.production";
    pub const SECRET_READ: &str = "secret.read";
    pub const NETWORK_UNRESTRICTED: &str = "network.unrestricted";
}

impl From<&str> for ToolClass {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

// --- Verification ---

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationSpec {
    pub required_commands: Vec<String>,
    pub artifact_checks: ArtifactChecks,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArtifactChecks {
    pub max_patch_files: u32,
    pub require_tests_for_code_changes: bool,
}

// --- Output ---

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutputSpec {
    pub mode: OutputMode,
    pub include_report: bool,
    pub include_replay_ref: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum OutputMode {
    #[serde(rename = "draft_pr")]
    DraftPr,
    #[serde(rename = "patch_bundle")]
    PatchBundle,
    #[serde(rename = "report_only")]
    ReportOnly,
}
