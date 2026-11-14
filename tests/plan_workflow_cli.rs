//! Black-box tests for the source-free offline workflow planner binary.
use std::fs;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT_FILE: AtomicUsize = AtomicUsize::new(0);
const SHA: &str = "0123456789abcdef0123456789abcdef01234567";

fn invoke(yaml: &str, require_independent: bool) -> Output {
    let index = NEXT_FILE.fetch_add(1, Ordering::Relaxed);
    let file = std::env::temp_dir().join(format!(
        "gha-clone-offline-plan-{}-{index}.yml",
        std::process::id()
    ));
    fs::write(&file, yaml).expect("write isolated YAML fixture");
    let mut command = Command::new(env!("CARGO_BIN_EXE_plan_workflow"));
    if require_independent {
        command.arg("--require-independent");
    }
    let output = command
        .args(["owner/repo", SHA, ".github/workflows/ci.yml"])
        .arg(&file)
        .output()
        .expect("offline planner process must start");
    fs::remove_file(file).expect("cleanup temporary fixture");
    output
}

#[test]
fn fixed_profile_workflow_produces_json_without_network() {
    let output = invoke(
        "jobs:\n  test:\n    runs-on: ubuntu-latest\n    steps:\n      - run: cargo test\n",
        true,
    );
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("plan JSON");
    assert_eq!(value["repository"], "owner/repo");
    assert_eq!(value["immutableRevision"], true);
    assert_eq!(value["independentExecutable"], true);
    assert_eq!(value["jobs"][0]["independentProfile"], "rust-verify");
}

#[test]
fn unsupported_commands_are_explained_and_fail_closed_only_when_required() {
    let yaml = "jobs:\n  sample:\n    runs-on: ubuntu-latest\n    steps:\n      - run: echo hello\n";
    let observe = invoke(yaml, false);
    assert!(observe.status.success());
    let require = invoke(yaml, true);
    assert_eq!(require.status.code(), Some(3));
    let value: serde_json::Value =
        serde_json::from_slice(&require.stdout).expect("plan JSON on denied admission");
    assert_eq!(value["independentExecutable"], false);
    assert!(value["jobs"][0]["independentReasons"]
        .as_array()
        .expect("reason list")
        .iter()
        .any(|reason| reason.as_str().unwrap_or("").contains("no fixed build-server profile")));
}

#[test]
fn invalid_yaml_and_oversized_input_return_validation_failure() {
    let malformed = invoke("jobs: [broken", false);
    assert_eq!(malformed.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&malformed.stderr).contains("not valid YAML"));

    let oversized = invoke(&"x".repeat(256 * 1024 + 1), false);
    assert_eq!(oversized.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&oversized.stderr).contains("byte limit"));
}
