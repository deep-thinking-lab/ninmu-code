use std::collections::BTreeSet;
use std::fmt;

use ninmu_runtime::harness_contract::{HarnessProtocolVersion, HarnessTaskRequest};
use serde_json::json;
use substrate_types::{FactoryCellManifest, ModelPolicy, ToolClass};

pub(crate) struct ManifestAdapter;

impl ManifestAdapter {
    pub(crate) fn to_harness_request(
        manifest: &FactoryCellManifest,
        workdir: &str,
    ) -> Result<HarnessTaskRequest, ManifestAdapterError> {
        if manifest.spec.objective.trim().is_empty() {
            return Err(ManifestAdapterError::Validation(
                "objective must not be empty".to_string(),
            ));
        }
        let mut allowed_tools = BTreeSet::new();
        for class in &manifest.spec.tools.allowed {
            if let Some(tool) = map_tool_class(class) {
                allowed_tools.insert(tool.to_string());
            }
        }
        let timeout_ms = manifest.spec.runtime.timeout_ms;
        Ok(HarnessTaskRequest {
            protocol: HarnessProtocolVersion::V1Alpha1,
            mission_id: manifest.metadata.mission_id.to_string(),
            task_id: manifest.metadata.task_id.to_string(),
            objective: manifest.spec.objective.clone(),
            workdir: workdir.to_string(),
            model: Some(model_for_policy(&manifest.spec.agent.model_policy)),
            permission_mode: None,
            allowed_tools: allowed_tools.into_iter().collect(),
            acceptance_tests: manifest.spec.verification.required_commands.clone(),
            timeout_seconds: Some(timeout_ms.saturating_add(999) / 1000),
            sandbox: None,
            skill_profile: None,
            project_profile: Some(json!({
                "substrate": {
                    "cell_id": manifest.metadata.cell_id.to_string(),
                    "tenant_id": manifest.metadata.tenant_id.to_string(),
                    "agent_id": manifest.metadata.agent_id.map(|id| id.to_string()),
                    "memory": manifest.spec.memory,
                }
            })),
            previous_context: None,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ManifestAdapterError {
    Validation(String),
}

impl fmt::Display for ManifestAdapterError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Validation(message) => formatter.write_str(message),
        }
    }
}

impl std::error::Error for ManifestAdapterError {}

fn model_for_policy(policy: &ModelPolicy) -> String {
    match policy {
        ModelPolicy::CostAware | ModelPolicy::QualityFirst => crate::DEFAULT_MODEL.to_string(),
        ModelPolicy::Fixed { model } => model.clone(),
    }
}

fn map_tool_class(class: &ToolClass) -> Option<&'static str> {
    match class.0.as_str() {
        ToolClass::SHELL_READONLY | ToolClass::SHELL_TEST | ToolClass::SHELL_WRITE => Some("bash"),
        ToolClass::GIT_BRANCH
        | ToolClass::GIT_DIFF
        | ToolClass::GIT_COMMIT
        | ToolClass::GIT_PUSH => Some("git"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::ManifestAdapter;
    use substrate_types::FactoryCellManifest;

    fn minimal_manifest() -> FactoryCellManifest {
        serde_yaml::from_str(include_str!("../tests/fixtures/cell_manifest_minimal.yaml"))
            .expect("minimal manifest should parse")
    }

    #[test]
    fn adapter_maps_objective_from_manifest() {
        let request = ManifestAdapter::to_harness_request(&minimal_manifest(), "/tmp/workdir")
            .expect("manifest should adapt");

        assert_eq!(request.objective, "Fix the failing auth timeout test");
    }

    #[test]
    fn adapter_maps_acceptance_tests_from_verification_spec() {
        let request = ManifestAdapter::to_harness_request(&minimal_manifest(), "/tmp/workdir")
            .expect("manifest should adapt");

        assert!(request
            .acceptance_tests
            .contains(&"cargo test --workspace".to_string()));
    }

    #[test]
    fn adapter_maps_allowed_tools() {
        let request = ManifestAdapter::to_harness_request(&minimal_manifest(), "/tmp/workdir")
            .expect("manifest should adapt");

        assert!(request.allowed_tools.contains(&"bash".to_string()));
        assert!(request.allowed_tools.contains(&"git".to_string()));
    }

    #[test]
    fn adapter_maps_cost_aware_model_policy() {
        let request = ManifestAdapter::to_harness_request(&minimal_manifest(), "/tmp/workdir")
            .expect("manifest should adapt");

        assert!(request
            .model
            .as_deref()
            .is_some_and(|model| !model.is_empty()));
    }

    #[test]
    fn adapter_maps_fixed_model_policy() {
        let manifest: FactoryCellManifest = serde_yaml::from_str(include_str!(
            "../tests/fixtures/cell_manifest_fixed_model.yaml"
        ))
        .expect("fixed model manifest should parse");
        let request = ManifestAdapter::to_harness_request(&manifest, "/tmp/workdir")
            .expect("manifest should adapt");

        assert_eq!(request.model.as_deref(), Some("ollama/llama-3.1-70b"));
    }

    #[test]
    fn adapter_converts_timeout_ms_to_seconds() {
        let request = ManifestAdapter::to_harness_request(&minimal_manifest(), "/tmp/workdir")
            .expect("manifest should adapt");

        assert_eq!(request.timeout_seconds, Some(900));
    }

    #[test]
    fn adapter_sets_workdir() {
        let request = ManifestAdapter::to_harness_request(&minimal_manifest(), "/workspace/repo")
            .expect("manifest should adapt");

        assert_eq!(request.workdir, "/workspace/repo");
    }

    #[test]
    fn adapter_maps_metadata_ids() {
        let manifest = minimal_manifest();
        let request = ManifestAdapter::to_harness_request(&manifest, "/tmp/workdir")
            .expect("manifest should adapt");

        assert_eq!(request.mission_id, manifest.metadata.mission_id.to_string());
        assert_eq!(request.task_id, manifest.metadata.task_id.to_string());
    }

    #[test]
    fn adapter_rejects_empty_objective() {
        let manifest: FactoryCellManifest = serde_yaml::from_str(include_str!(
            "../tests/fixtures/cell_manifest_empty_objective.yaml"
        ))
        .expect("empty objective manifest should parse");

        let error = ManifestAdapter::to_harness_request(&manifest, "/tmp/workdir")
            .expect_err("empty objective should be rejected");

        assert!(error.to_string().contains("objective"));
    }
}
