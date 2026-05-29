use substrate_types::*;

#[test]
fn parse_valid_minimal_manifest() {
    let yaml = include_str!("manifest_fixtures/valid_minimal.yaml");
    let manifest: FactoryCellManifest = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(manifest.spec.objective, "Fix the failing auth timeout test");
    assert!(matches!(
        manifest.spec.repository,
        RepositorySpec::Kizuna { .. }
    ));
    assert_eq!(manifest.spec.agent.max_steps, 16);
}

#[test]
fn manifest_has_correct_api_version() {
    let yaml = include_str!("manifest_fixtures/valid_minimal.yaml");
    let manifest: FactoryCellManifest = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(manifest.api_version, "substrate.deepthinking.dev/v1alpha1");
}

#[test]
fn manifest_roundtrip_yaml() {
    let yaml = include_str!("manifest_fixtures/valid_minimal.yaml");
    let manifest: FactoryCellManifest = serde_yaml::from_str(yaml).unwrap();
    let re_yaml = serde_yaml::to_string(&manifest).unwrap();
    let back: FactoryCellManifest = serde_yaml::from_str(&re_yaml).unwrap();
    assert_eq!(manifest.spec.objective, back.spec.objective);
}

#[test]
fn manifest_roundtrip_json() {
    let yaml = include_str!("manifest_fixtures/valid_minimal.yaml");
    let manifest: FactoryCellManifest = serde_yaml::from_str(yaml).unwrap();
    let json = serde_json::to_string_pretty(&manifest).unwrap();
    let back: FactoryCellManifest = serde_json::from_str(&json).unwrap();
    assert_eq!(manifest.spec.objective, back.spec.objective);
    assert_eq!(manifest.spec.agent.max_steps, back.spec.agent.max_steps);
}

#[test]
fn kizuna_repository_spec_has_agent_identity() {
    let yaml = include_str!("manifest_fixtures/valid_minimal.yaml");
    let manifest: FactoryCellManifest = serde_yaml::from_str(yaml).unwrap();
    match &manifest.spec.repository {
        RepositorySpec::Kizuna { agent_identity, .. } => {
            assert_eq!(agent_identity.trust_level, TrustLevel::Standard);
            assert!(agent_identity.scopes.contains(&AgentScope::Read));
            assert!(agent_identity.scopes.contains(&AgentScope::Write));
        }
    }
}

#[test]
fn kizuna_repository_spec_has_instance_and_org() {
    let yaml = include_str!("manifest_fixtures/valid_minimal.yaml");
    let manifest: FactoryCellManifest = serde_yaml::from_str(yaml).unwrap();
    match &manifest.spec.repository {
        RepositorySpec::Kizuna {
            instance,
            org,
            repo,
            base_ref,
            working_ref,
            ..
        } => {
            assert_eq!(instance, "forge.substrate.dev");
            assert_eq!(org, "acme");
            assert_eq!(repo, "backend");
            assert_eq!(base_ref, "main");
            assert_eq!(working_ref, "factory/auth-timeout");
        }
    }
}

#[test]
fn validation_passes_for_valid_manifest() {
    let yaml = include_str!("manifest_fixtures/valid_minimal.yaml");
    let manifest: FactoryCellManifest = serde_yaml::from_str(yaml).unwrap();
    ManifestValidation::validate(&manifest).unwrap();
}

#[test]
fn validation_rejects_empty_objective() {
    let yaml = include_str!("manifest_fixtures/valid_minimal.yaml");
    let mut manifest: FactoryCellManifest = serde_yaml::from_str(yaml).unwrap();
    manifest.spec.objective = String::new();
    let errs = ManifestValidation::validate(&manifest).unwrap_err();
    assert!(errs
        .iter()
        .any(|e| matches!(e, ValidationError::EmptyObjective)));
}

#[test]
fn validation_rejects_zero_max_steps() {
    let yaml = include_str!("manifest_fixtures/valid_minimal.yaml");
    let mut manifest: FactoryCellManifest = serde_yaml::from_str(yaml).unwrap();
    manifest.spec.agent.max_steps = 0;
    let errs = ManifestValidation::validate(&manifest).unwrap_err();
    assert!(errs
        .iter()
        .any(|e| matches!(e, ValidationError::ZeroMaxSteps)));
}

#[test]
fn validation_rejects_tool_in_both_lists() {
    let yaml = include_str!("manifest_fixtures/valid_minimal.yaml");
    let mut manifest: FactoryCellManifest = serde_yaml::from_str(yaml).unwrap();
    manifest
        .spec
        .tools
        .allowed
        .push(ToolClass("deploy.production".into()));
    let errs = ManifestValidation::validate(&manifest).unwrap_err();
    assert!(errs
        .iter()
        .any(|e| matches!(e, ValidationError::ToolInBothLists(_))));
}

#[test]
fn sensitivity_class_ordering() {
    assert!(SensitivityClass::Public < SensitivityClass::Internal);
    assert!(SensitivityClass::Internal < SensitivityClass::Confidential);
    assert!(SensitivityClass::Confidential < SensitivityClass::Restricted);
}

#[test]
fn memory_grant_has_labels() {
    let yaml = include_str!("manifest_fixtures/valid_minimal.yaml");
    let manifest: FactoryCellManifest = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(
        manifest.spec.memory.grant.sensitivity_ceiling,
        SensitivityClass::Internal
    );
    assert!(manifest
        .spec
        .memory
        .grant
        .labels
        .contains(&"project/*".to_string()));
}

#[test]
fn output_mode_is_draft_pr() {
    let yaml = include_str!("manifest_fixtures/valid_minimal.yaml");
    let manifest: FactoryCellManifest = serde_yaml::from_str(yaml).unwrap();
    assert!(matches!(manifest.spec.output.mode, OutputMode::DraftPr));
    assert!(manifest.spec.output.include_report);
    assert!(manifest.spec.output.include_replay_ref);
}

#[test]
fn parse_valid_full_manifest() {
    let yaml = include_str!("manifest_fixtures/valid_full.yaml");
    let manifest: FactoryCellManifest = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(
        manifest.spec.objective,
        "Implement the new payment webhook handler with retry logic"
    );
    assert_eq!(manifest.spec.agent.max_steps, 32);
    assert_eq!(manifest.spec.runtime.cpu, 4);
    assert_eq!(manifest.spec.runtime.memory_mb, 8192);
    assert!(matches!(
        manifest.spec.runtime.isolation,
        IsolationMode::MicroVm
    ));
    ManifestValidation::validate(&manifest).unwrap();
}

#[test]
fn full_manifest_has_agent_id() {
    let yaml = include_str!("manifest_fixtures/valid_full.yaml");
    let manifest: FactoryCellManifest = serde_yaml::from_str(yaml).unwrap();
    assert!(manifest.metadata.agent_id.is_some());
}

#[test]
fn full_manifest_has_allowlist_network_policy() {
    let yaml = include_str!("manifest_fixtures/valid_full.yaml");
    let manifest: FactoryCellManifest = serde_yaml::from_str(yaml).unwrap();
    match &manifest.spec.runtime.network_policy {
        NetworkPolicy::Allowlist { hosts } => {
            assert_eq!(hosts.len(), 3);
            assert!(hosts.contains(&"crates.io".to_string()));
        }
        _ => panic!("expected Allowlist network policy"),
    }
}

#[test]
fn full_manifest_has_fixed_model_policy() {
    let yaml = include_str!("manifest_fixtures/valid_full.yaml");
    let manifest: FactoryCellManifest = serde_yaml::from_str(yaml).unwrap();
    match &manifest.spec.agent.model_policy {
        ModelPolicy::Fixed { model } => {
            assert_eq!(model, "claude-sonnet-4-6");
        }
        _ => panic!("expected Fixed model policy"),
    }
}

#[test]
fn full_manifest_has_elevated_trust_level() {
    let yaml = include_str!("manifest_fixtures/valid_full.yaml");
    let manifest: FactoryCellManifest = serde_yaml::from_str(yaml).unwrap();
    match &manifest.spec.repository {
        RepositorySpec::Kizuna { agent_identity, .. } => {
            assert_eq!(agent_identity.trust_level, TrustLevel::Elevated);
            assert!(agent_identity.scopes.contains(&AgentScope::Merge));
        }
    }
}

#[test]
fn full_manifest_has_multiple_verification_commands() {
    let yaml = include_str!("manifest_fixtures/valid_full.yaml");
    let manifest: FactoryCellManifest = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(manifest.spec.verification.required_commands.len(), 3);
}

#[test]
fn validation_rejects_empty_objective_from_fixture() {
    let yaml = include_str!("manifest_fixtures/invalid_missing_objective.yaml");
    let manifest: FactoryCellManifest = serde_yaml::from_str(yaml).unwrap();
    let errs = ManifestValidation::validate(&manifest).unwrap_err();
    assert!(errs
        .iter()
        .any(|e| matches!(e, ValidationError::EmptyObjective)));
}

#[test]
fn bad_trust_level_fixture_fails_to_parse() {
    let yaml = include_str!("manifest_fixtures/invalid_bad_trust_level.yaml");
    let result: Result<FactoryCellManifest, _> = serde_yaml::from_str(yaml);
    assert!(
        result.is_err(),
        "trust_level 99 should fail deserialization"
    );
}

#[test]
fn minimal_manifest_has_no_agent_id() {
    let yaml = include_str!("manifest_fixtures/valid_minimal.yaml");
    let manifest: FactoryCellManifest = serde_yaml::from_str(yaml).unwrap();
    assert!(manifest.metadata.agent_id.is_none());
}

#[test]
fn full_manifest_roundtrip_json() {
    let yaml = include_str!("manifest_fixtures/valid_full.yaml");
    let manifest: FactoryCellManifest = serde_yaml::from_str(yaml).unwrap();
    let json = serde_json::to_string_pretty(&manifest).unwrap();
    let back: FactoryCellManifest = serde_json::from_str(&json).unwrap();
    assert_eq!(manifest.spec.objective, back.spec.objective);
    assert_eq!(manifest.spec.agent.max_steps, back.spec.agent.max_steps);
    assert_eq!(manifest.spec.runtime.cpu, back.spec.runtime.cpu);
}

#[test]
fn harness_kind_accepts_cosmictron_alias() {
    let yaml = include_str!("manifest_fixtures/valid_minimal.yaml");
    let patched = yaml.replace("ninmu-code", "cosmictron");
    let manifest: FactoryCellManifest = serde_yaml::from_str(&patched).unwrap();
    assert!(matches!(
        manifest.spec.agent.harness,
        HarnessKind::NinmuCode
    ));
}
