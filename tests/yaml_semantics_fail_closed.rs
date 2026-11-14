use gha_clone_server::{build_plan, PlanRequest, PlannerLimits, WorkflowPlan};

const SHA: &str = "0123456789abcdef0123456789abcdef01234567";

fn request(yaml: &str) -> PlanRequest {
    PlanRequest {
        repository: "example/ci-project".into(),
        revision: SHA.into(),
        workflow_path: ".github/workflows/ci.yml".into(),
        workflow_yaml: yaml.into(),
    }
}

fn plan(yaml: &str) -> WorkflowPlan {
    build_plan(&request(yaml), &PlannerLimits::default()).expect("valid YAML plan")
}

#[test]
fn unknown_job_and_step_fields_cannot_be_silently_executed() {
    let plan = plan(r#"
jobs:
  verify:
    runs-on: ubuntu-latest
    concurrency: one-at-a-time
    steps:
      - run: cargo test
        bogus-workspace: src
"#);
    let reasons = plan.jobs[0].independent_reasons.join("\n");
    assert!(reasons.contains("job-level concurrency"), "{reasons}");
    assert!(reasons.contains("field bogus-workspace"), "{reasons}");
    assert!(!plan.jobs[0].independent_supported);
    assert!(!plan.independent_executable);
    assert!(plan.jobs[0].arc_compatible);
}

#[test]
fn ordinary_environment_overrides_cannot_be_dropped() {
    let plan = plan(r#"
jobs:
  rust:
    runs-on: ubuntu-latest
    env:
      RUSTFLAGS: -D warnings
    steps:
      - run: cargo test
        env:
          CARGO_TARGET_DIR: custom-target
"#);
    let reasons = plan.jobs[0].independent_reasons.join("\n");
    assert!(reasons.contains("job-level env cannot be reproduced"), "{reasons}");
    assert!(reasons.contains("steps[0]: env cannot be reproduced"), "{reasons}");
    assert!(!plan.independent_executable);
}

#[test]
fn non_string_mapping_keys_are_not_ignored() {
    let plan = plan(r#"
42: unsupported-top-level
jobs:
  rust:
    runs-on: ubuntu-latest
    13: ignored-job-behavior
    steps:
      - run: cargo test
        99: ignored-step-behavior
"#);
    let reasons = plan.jobs[0].independent_reasons.join("\n");
    assert!(reasons.contains("workflow-level non-string mapping keys"), "{reasons}");
    assert!(reasons.contains("job-level non-string mapping keys"), "{reasons}");
    assert!(reasons.contains("steps[0]: non-string mapping keys"), "{reasons}");
    assert!(!plan.independent_executable);
}

#[test]
fn dynamic_runner_selection_must_not_map_to_static_host() {
    let plan = plan(r#"
jobs:
  test:
    runs-on: "${{ matrix.os }}"
    steps:
      - run: cargo test
"#);
    assert!(plan.jobs[0]
        .independent_reasons
        .iter()
        .any(|reason| reason.contains("dynamic runs-on expressions")));
    assert!(!plan.independent_executable);
}

#[test]
fn malformed_action_and_command_shapes_fail_validation() {
    for (step, expected) in [
        ("- name: no-command", "exactly one of run or uses"),
        ("- run: cargo test\n        uses: actions/checkout@abc", "exactly one of run or uses"),
        ("- run: 42", "run must be a string"),
        ("- uses: [actions/checkout@abc]", "uses must be a string"),
    ] {
        let yaml = format!("jobs:\n  test:\n    runs-on: ubuntu-latest\n    steps:\n      {step}\n");
        let errors = build_plan(&request(&yaml), &PlannerLimits::default())
            .expect_err("malformed step should be rejected")
            .join("\n");
        assert!(errors.contains(expected), "{expected:?} not found in {errors:?}");
    }
}

#[test]
fn literal_no_override_workflow_remains_executable() {
    let plan = plan(r#"
on: [push]
jobs:
  verify:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@abc
      - run: cargo test
"#);
    assert!(plan.independent_executable, "{:?}", plan.jobs[0].independent_reasons);
    assert_eq!(plan.jobs[0].independent_profile.as_deref(), Some("rust-verify"));
}
