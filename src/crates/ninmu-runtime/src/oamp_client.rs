use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use substrate_types::{AgentId, MemoryGrant, MissionId, SensitivityClass, TaskId};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecallRequest {
    pub query: String,
    pub sensitivity_ceiling: String,
    pub label_scope: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WriteRequest {
    pub content: String,
    pub sensitivity: String,
    pub provenance: SerializedProvenance,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Provenance {
    pub agent_id: AgentId,
    pub mission_id: MissionId,
    pub task_id: TaskId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SerializedProvenance {
    pub agent_id: String,
    pub mission_id: String,
    pub task_id: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MemoryEntry {
    pub content: String,
    #[serde(default)]
    pub provenance: Value,
}

#[derive(Debug, Clone)]
pub struct OampClient {
    endpoint: String,
    grant: MemoryGrant,
    http: reqwest::Client,
    mock_entries: Option<Vec<MemoryEntry>>,
}

impl OampClient {
    #[must_use]
    pub fn new(endpoint: &str, grant: MemoryGrant) -> Self {
        Self {
            endpoint: endpoint.to_string(),
            grant,
            http: reqwest::Client::new(),
            mock_entries: None,
        }
    }

    #[must_use]
    pub fn mock(grant: MemoryGrant, entries: Vec<MemoryEntry>) -> Self {
        Self {
            endpoint: "mock://oamp".to_string(),
            grant,
            http: reqwest::Client::new(),
            mock_entries: Some(entries),
        }
    }

    #[must_use]
    pub fn grant(&self) -> &MemoryGrant {
        &self.grant
    }

    pub async fn recall(&self, query: &str) -> Result<Vec<MemoryEntry>, OampError> {
        if let Some(entries) = &self.mock_entries {
            return Ok(entries.clone());
        }
        let request = Self::build_recall_request(query, &self.grant);
        let response = self
            .http
            .post(format!("{}/recall", self.endpoint.trim_end_matches('/')))
            .json(&request)
            .send()
            .await
            .map_err(|error| OampError::Connection(error.to_string()))?;
        response
            .json::<Vec<MemoryEntry>>()
            .await
            .map_err(|error| OampError::Protocol(error.to_string()))
    }

    pub async fn write(
        &self,
        content: &str,
        provenance: &Provenance,
        sensitivity: SensitivityClass,
    ) -> Result<(), OampError> {
        Self::validate_write_sensitivity(sensitivity, &self.grant)?;
        if self.mock_entries.is_some() {
            return Ok(());
        }
        let request = Self::build_write_request(content, provenance, sensitivity);
        self.http
            .post(format!("{}/write", self.endpoint.trim_end_matches('/')))
            .json(&request)
            .send()
            .await
            .map_err(|error| OampError::Connection(error.to_string()))?;
        Ok(())
    }

    #[must_use]
    pub fn build_recall_request(query: &str, grant: &MemoryGrant) -> RecallRequest {
        RecallRequest {
            query: query.to_string(),
            sensitivity_ceiling: sensitivity_label(grant.sensitivity_ceiling).to_string(),
            label_scope: grant.labels.clone(),
        }
    }

    #[must_use]
    pub fn build_write_request(
        content: &str,
        provenance: &Provenance,
        sensitivity: SensitivityClass,
    ) -> WriteRequest {
        WriteRequest {
            content: content.to_string(),
            sensitivity: sensitivity_label(sensitivity).to_string(),
            provenance: SerializedProvenance {
                agent_id: provenance.agent_id.to_string(),
                mission_id: provenance.mission_id.to_string(),
                task_id: provenance.task_id.to_string(),
            },
        }
    }

    pub fn validate_write_sensitivity(
        sensitivity: SensitivityClass,
        grant: &MemoryGrant,
    ) -> Result<(), OampError> {
        if sensitivity > grant.sensitivity_ceiling {
            return Err(OampError::SensitivityExceeded {
                requested: sensitivity_label(sensitivity).to_string(),
                ceiling: sensitivity_label(grant.sensitivity_ceiling).to_string(),
            });
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OampError {
    Connection(String),
    Protocol(String),
    SensitivityExceeded { requested: String, ceiling: String },
    InvalidSensitivity(String),
}

impl std::fmt::Display for OampError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Connection(message) => write!(formatter, "connection error: {message}"),
            Self::Protocol(message) => write!(formatter, "protocol error: {message}"),
            Self::SensitivityExceeded { requested, ceiling } => write!(
                formatter,
                "sensitivity {requested} exceeds grant ceiling {ceiling}"
            ),
            Self::InvalidSensitivity(value) => write!(formatter, "invalid sensitivity: {value}"),
        }
    }
}

impl std::error::Error for OampError {}

#[must_use]
pub fn sensitivity_label(sensitivity: SensitivityClass) -> &'static str {
    match sensitivity {
        SensitivityClass::Public => "public",
        SensitivityClass::Internal => "internal",
        SensitivityClass::Confidential => "confidential",
        SensitivityClass::Restricted => "restricted",
    }
}

pub fn parse_sensitivity(value: &str) -> Result<SensitivityClass, OampError> {
    match value {
        "public" => Ok(SensitivityClass::Public),
        "internal" => Ok(SensitivityClass::Internal),
        "confidential" => Ok(SensitivityClass::Confidential),
        "restricted" => Ok(SensitivityClass::Restricted),
        other => Err(OampError::InvalidSensitivity(other.to_string())),
    }
}

#[must_use]
pub fn memory_entry_json(entries: &[MemoryEntry]) -> Value {
    json!({
        "entries": entries,
    })
}
