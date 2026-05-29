use crate::manifest::{FactoryCellManifest, ToolClass};

#[derive(Debug, Clone)]
pub enum ValidationError {
    EmptyObjective,
    ZeroMaxSteps,
    ZeroTimeout,
    ToolInBothLists(ToolClass),
    InvalidApiVersion(String),
}

impl std::fmt::Display for ValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyObjective => write!(f, "objective must not be empty"),
            Self::ZeroMaxSteps => write!(f, "max_steps must be greater than zero"),
            Self::ZeroTimeout => write!(f, "timeout_ms must be greater than zero"),
            Self::ToolInBothLists(tool) => {
                write!(
                    f,
                    "tool '{}' appears in both allowed and approval_required",
                    tool.0
                )
            }
            Self::InvalidApiVersion(v) => write!(f, "invalid api_version: {}", v),
        }
    }
}

impl std::error::Error for ValidationError {}

pub struct ManifestValidation;

impl ManifestValidation {
    pub fn validate(manifest: &FactoryCellManifest) -> Result<(), Vec<ValidationError>> {
        let mut errors = Vec::new();

        if manifest.api_version != "substrate.deepthinking.dev/v1alpha1" {
            errors.push(ValidationError::InvalidApiVersion(
                manifest.api_version.clone(),
            ));
        }

        if manifest.spec.objective.is_empty() {
            errors.push(ValidationError::EmptyObjective);
        }

        if manifest.spec.agent.max_steps == 0 {
            errors.push(ValidationError::ZeroMaxSteps);
        }

        if manifest.spec.runtime.timeout_ms == 0 {
            errors.push(ValidationError::ZeroTimeout);
        }

        for tool in &manifest.spec.tools.approval_required {
            if manifest.spec.tools.allowed.contains(tool) {
                errors.push(ValidationError::ToolInBothLists(tool.clone()));
            }
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }
}
