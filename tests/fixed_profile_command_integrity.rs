use gha_clone_server::{build_plan, PlanRequest, PlannerLimits};

const SHA: &str = "0123456789abcdef0123456789abcdef01234567";

fn reason_text(yaml: &str) -> (bool, String) {
    let request = PlanRequest {
        repository: "example/verification".into(),
        revision: SHA.into(),
        workflow_path: ".github/workflows/ci.yml".into(),
        workflow_yaml: yaml.into(),
    };
    let plan = build_plan(&request, &PlannerLimits::default()).expect("valid workflow syntax");
    (
        plan.independent_executable,
        plan.jobs[0].independent_reasons.join("\n"),
    )
}

#[test]
fn setup_only_cannot_be_misreported_as_verification() {
    for setup in [
        "dtolnay/rust-toolchain@abcdef",
        "actions/setup-node@abcdef",
        "actions/setup-python@abcdef",
        "subosito/flutter-action@abcdef",
    ] {
        let yaml = format!(
            "jobs:\n  verify:\n    runs-on: ubuntu-latest\n    steps:\n      - uses: {setup}\n"
        );
        let (supported, reasons) = reason_text(&yaml);
        assert!(!supported, "setup {setup} accepted without a run step");
        assert!(
            reasons.contains("requires a reviewed verification run command"),
            "{setup}: {reasons}"
        );
    }
}

#[test]
fn mixed_language_verification_cannot_collapse_to_one_profile() {
    for command in [
        "cargo test && npm test",
        "npm test && cargo clippy",
        "python -m pytest && cargo test",
        "flutter analyze && cargo test",
    ] {
        let yaml = format!(
            "jobs:\n  verify:\n    runs-on: ubuntu-latest\n    steps:\n      - run: {command}\n"
        );
        let (supported, reasons) = reason_text(&yaml);
        assert!(!supported, "{command} was admitted");
        assert!(
            reasons.contains("does not match fixed profile"),
            "{command}: {reasons}"
        );
    }
}

#[test]
fn installation_only_cannot_be_disguised_as_a_node_test() {
    for command in ["npm ci", "npm ci --ignore-scripts"] {
        let yaml = format!(
            "jobs:\n  verify:\n    runs-on: ubuntu-latest\n    steps:\n      - run: {command}\n"
        );
        let (supported, reasons) = reason_text(&yaml);
        assert!(!supported, "{command} was admitted");
        assert!(
            reasons.contains("requires a reviewed verification run command"),
            "{command}: {reasons}"
        );
    }
}

#[test]
fn unrelated_toolchain_setup_is_not_silently_ignored() {
    for setup in [
        "actions/setup-python@abcdef",
        "actions/setup-node@abcdef",
        "subosito/flutter-action@abcdef",
        "actions/setup-java@abcdef",
    ] {
        let yaml = format!(
            "jobs:\n  verify:\n    runs-on: ubuntu-latest\n    steps:\n      - uses: {setup}\n      - run: cargo test\n"
        );
        let (supported, reasons) = reason_text(&yaml);
        assert!(!supported, "{setup} silently ignored");
        assert!(
            reasons.contains("does not match fixed profile"),
            "{setup}: {reasons}"
        );
    }
}

#[test]
fn same_profile_multi_step_verification_remains_supported() {
    for (setup, run) in [
        (
            "dtolnay/rust-toolchain@abcdef",
            "cargo fmt --check && cargo test",
        ),
        ("actions/setup-node@abcdef", "npm ci && npm test"),
        (
            "actions/setup-python@abcdef",
            "python -m compileall . && python -m pytest",
        ),
    ] {
        let yaml = format!(
            "jobs:\n  verify:\n    runs-on: ubuntu-latest\n    steps:\n      - uses: actions/checkout@abcdef\n      - uses: {setup}\n      - run: {run}\n"
        );
        let (supported, reasons) = reason_text(&yaml);
        assert!(supported, "{setup} / {run}: {reasons}");
    }
}
