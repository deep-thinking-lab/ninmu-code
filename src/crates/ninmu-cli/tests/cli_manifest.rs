mod common;

use std::fs;
use std::process::Command;

use common::{assert_success, unique_temp_dir};
use serde_json::Value;

fn repo_root() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("..")
}

#[test]
fn cli_accepts_manifest_flag() {
    let root = unique_temp_dir("cli-manifest");
    let home = root.join("home");
    let config_home = root.join("config");
    let workspace = root.join("workspace");
    fs::create_dir_all(&home).expect("home should exist");
    fs::create_dir_all(&config_home).expect("config home should exist");
    fs::create_dir_all(&workspace).expect("workspace should exist");

    let output = Command::new(env!("CARGO_BIN_EXE_ninmu"))
        .current_dir(repo_root().join("src"))
        .env_clear()
        .env("NINMU_CODE_TASK_MOCK_RUNTIME", "1")
        .env("NINMU_CONFIG_HOME", &config_home)
        .env("HOME", &home)
        .env("NO_COLOR", "1")
        .env("PATH", "/usr/bin:/bin")
        .args([
            "run-task",
            "--manifest",
            concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/fixtures/cell_manifest_minimal.yaml"
            ),
            "--workdir",
            workspace.to_str().expect("workspace path utf8"),
            "--dry-run",
            "--output-format",
            "json",
        ])
        .output()
        .expect("ninmu should run");

    assert_success(&output);
    let parsed: Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(parsed["mission_id"].as_str().unwrap()[..4], *"mis_");
    assert_eq!(parsed["objective"], "Fix the failing auth timeout test");
    assert_eq!(
        parsed["workdir"],
        workspace.to_str().expect("workspace path utf8")
    );
}

#[test]
fn cli_rejects_manifest_with_input() {
    let output = Command::new(env!("CARGO_BIN_EXE_ninmu"))
        .current_dir(repo_root().join("src"))
        .args([
            "run-task",
            "--manifest",
            "cell.yaml",
            "--input",
            "task.json",
            "--output-format",
            "json",
        ])
        .output()
        .expect("ninmu should run");

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("cannot use --manifest and --input together"));
}
