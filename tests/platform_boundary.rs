use serde_json::Value;

const BOUNDARY_JSON: &str = include_str!("../docs/platform-boundary.json");
const BOUNDARY_DOC: &str = include_str!("../docs/platform-boundary.md");

fn boundary() -> Value {
    serde_json::from_str(BOUNDARY_JSON).expect("platform boundary must be valid JSON")
}

fn array_contains(document: &Value, pointer: &str, expected: &str) -> bool {
    document
        .pointer(pointer)
        .and_then(Value::as_array)
        .is_some_and(|values| values.iter().any(|value| value.as_str() == Some(expected)))
}

fn string_at<'a>(document: &'a Value, pointer: &str) -> &'a str {
    document
        .pointer(pointer)
        .and_then(Value::as_str)
        .unwrap_or_else(|| panic!("missing string at {pointer}"))
}

#[test]
fn makes_no_full_github_actions_parity_claim() {
    let document = boundary();

    assert_eq!(document["contract_version"], 1);
    assert_eq!(string_at(&document, "/linear_issue"), "DEN-1606");
    assert_eq!(string_at(&document, "/service"), "gha-clone-server");
    assert_eq!(string_at(&document, "/parity_claim"), "none");
    assert_eq!(
        string_at(
            &document,
            "/canonical_ownership/official_github_actions_semantics"
        ),
        "GitHub-hosted runners and official Actions Runner Controller"
    );
}

#[test]
fn keeps_planning_routing_execution_and_agent_policy_separate() {
    let document = boundary();

    assert_eq!(
        string_at(
            &document,
            "/canonical_ownership/bounded_planning_and_run_coordination"
        ),
        "gha-clone-server"
    );
    assert_eq!(
        string_at(
            &document,
            "/canonical_ownership/cloud_selection_and_provider_status"
        ),
        "gha-executor-router"
    );
    assert_eq!(
        string_at(
            &document,
            "/canonical_ownership/fixed_profile_execution_logs_and_artifacts"
        ),
        "dd-build-server"
    );
    assert_eq!(
        string_at(
            &document,
            "/canonical_ownership/agent_policy_and_model_routing"
        ),
        "agent-pontifex"
    );
    assert!(array_contains(
        &document,
        "/does_not_own",
        "cloud provider selection"
    ));
    assert!(array_contains(
        &document,
        "/does_not_own",
        "general coding-agent model routing"
    ));
}

#[test]
fn requires_immutable_direct_workflow_inputs() {
    let document = boundary();

    for field in [
        "repository",
        "immutable_commit_sha",
        "workflow_path",
        "idempotency_key",
        "traceparent",
    ] {
        assert!(array_contains(
            &document,
            "/request_contract/required_fields",
            field
        ));
    }

    assert!(string_at(&document, "/request_contract/immutable_commit_sha").contains("40"));
    assert!(string_at(&document, "/request_contract/workflow_path")
        .contains("direct file under .github/workflows"));
    assert!(array_contains(
        &document,
        "/request_contract/forbidden",
        "branch or tag as execution authority"
    ));
    assert!(array_contains(
        &document,
        "/request_contract/forbidden",
        "redirect-followed workflow fetch"
    ));
}

#[test]
fn rejects_ambiguous_or_dynamic_yaml_instead_of_approximating_it() {
    let document = boundary();

    for construct in [
        "YAML tags",
        "merge keys",
        "duplicate mapping keys",
        "multiple YAML documents",
        "dynamic matrix",
        "dynamic if expression",
        "reusable workflow",
        "unsupported marketplace action",
        "needs cycle",
    ] {
        assert!(array_contains(
            &document,
            "/workflow_parser/rejects",
            construct
        ));
    }

    assert_eq!(
        string_at(&document, "/workflow_parser/unsupported_behavior"),
        "return explicit unsupported evidence and do not approximate"
    );
}

#[test]
fn dispatches_only_fixed_profiles_and_validates_returned_ids() {
    let document = boundary();

    let target_path = document["execution_contract"]["target_path"]
        .as_array()
        .expect("target path must be an array")
        .iter()
        .filter_map(Value::as_str)
        .collect::<Vec<_>>();
    assert_eq!(
        target_path,
        vec!["gha-clone-server", "gha-executor-router", "dd-build-server"]
    );

    for field in [
        "repository",
        "immutable_commit_sha",
        "profile",
        "request_id",
        "traceparent",
    ] {
        assert!(array_contains(
            &document,
            "/execution_contract/profile_request_fields",
            field
        ));
    }

    assert!(
        string_at(&document, "/execution_contract/returned_id_validation")
            .contains("before constructing")
    );
}

#[test]
fn reserves_capacity_before_tasks_and_fences_horizontal_webhook_execution() {
    let document = boundary();

    assert!(string_at(&document, "/admission_and_scaling/capacity_rule")
        .contains("before API or webhook task creation"));
    assert!(
        string_at(&document, "/admission_and_scaling/single_replica_rule").contains("one replica")
    );
    assert!(
        string_at(&document, "/admission_and_scaling/horizontal_scaling_gate")
            .contains("Fiducia-fenced")
    );

    for limit in [
        "workflow bytes",
        "active runs",
        "run timeout",
        "delivery retention TTL",
        "upstream response bytes",
    ] {
        assert!(array_contains(
            &document,
            "/admission_and_scaling/limits_must_be_strictly_positive",
            limit
        ));
    }
}

#[test]
fn separates_credentials_and_excludes_them_from_telemetry() {
    let document = boundary();

    assert_eq!(
        string_at(&document, "/authentication/github"),
        "short-lived installation-scoped GitHub App token"
    );
    assert_eq!(
        string_at(&document, "/authentication/build_transport"),
        "separate scoped service credential"
    );

    for forbidden in [
        "personal access token in source or task payload",
        "credential in a URL",
        "shared credential reused across GitHub, planning, and execution boundaries",
    ] {
        assert!(array_contains(
            &document,
            "/authentication/forbidden",
            forbidden
        ));
    }

    for field in [
        "API auth secret",
        "GitHub token",
        "build transport credential",
        "raw Authorization header",
    ] {
        assert!(array_contains(
            &document,
            "/observability/forbidden_fields",
            field
        ));
    }
}

#[test]
fn defaults_execution_off_and_documents_safe_activation_order() {
    let document = boundary();

    assert_eq!(document["deployment"]["default_execution_enabled"], false);
    assert_eq!(
        document["deployment"]["default_webhook_execution_enabled"],
        false
    );
    assert!(string_at(&document, "/deployment/artifact_policy").contains("immutable digest"));
    assert!(array_contains(
        &document,
        "/deployment/activation_order",
        "run plan-only fixtures"
    ));
    assert!(array_contains(
        &document,
        "/deployment/activation_order",
        "enable failure-webhook execution last"
    ));
}

#[test]
fn human_readable_contract_states_the_same_fail_closed_boundary() {
    for statement in [
        "does **not** claim parity with GitHub Actions",
        "Support classification is evidence, not a guess",
        "gha-executor-router -> dd-build-server",
        "Fiducia-fenced",
        "Personal access tokens",
        "API and webhook execution default to disabled",
    ] {
        assert!(
            BOUNDARY_DOC.contains(statement),
            "human-readable contract must mention {statement}"
        );
    }
}
