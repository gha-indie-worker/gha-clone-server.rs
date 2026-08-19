# `gha-clone-server-rs`

`gha-clone-server-rs` is the bounded independent-continuity half of the
portfolio's GitHub Actions strategy. It does **not** claim to reproduce GitHub's
proprietary control plane.

- **Native parity:** GitHub-hosted runners and Actions Runner Controller execute
  workflows through GitHub's expression evaluator, job orchestration,
  marketplace actions, checks, artifacts, and runner protocol.
- **Independent continuity:** this service accepts a deliberately restricted
  workflow subset and compiles it to operator-reviewed fixed profiles executed
  behind `dd-build-server` and `gha-indie-worker`.

The independent lane never forwards caller-selected shell, action code, images,
providers, Kubernetes manifests, deployment targets, or credentials. Its
execution envelope contains only a canonical repository, immutable full commit
SHA, fixed reviewed profile, and deterministic request identity.

## API

All `/v1/*` endpoints require `x-gha-clone-auth` or `x-server-auth`, compared in
constant time over SHA-256 digests.

- `GET /v1/capabilities` — supported independent profiles, limits, and explicit
  exclusions.
- `POST /v1/plans` — parse workflow YAML and report per-job support.
- `POST /v1/runs` — enqueue a fully supported immutable plan.
- `GET /v1/runs/<uuid>` — inspect build submissions and terminal state.
- `POST /webhooks/github` — verify the raw-body HMAC and delivery UUID; for
  `workflow_run`, require the configured terminal conclusion, exact repository,
  exact workflow path, immutable SHA, recursion exclusion, bounded workflow
  fetch, supported plan, execution readiness, and a previously unseen delivery
  claim.
- `GET /healthz`, `GET /readyz`.

Example plan:

```json
{
  "repository": "gha-indie-worker/gha-clone-server.rs",
  "revision": "0123456789abcdef0123456789abcdef01234567",
  "workflowPath": ".github/workflows/gha-clone-server-meta.yml",
  "workflowYaml": "jobs:\n  test:\n    runs-on: ubuntu-latest\n    steps:\n      - run: cargo test --locked --all-targets\n"
}
```

## Fixed-profile mapping

| Workflow evidence | Fixed build-server profile |
| --- | --- |
| Cargo/rustfmt/Clippy/tests | `rust-verify` |
| npm/pnpm/yarn/Node tests | `node-verify` |
| Python compile/pytest | `python-verify` |
| Flutter analyze/tests | `flutter-verify` |
| Flutter Android APK/App Bundle | `flutter-android-debug` |
| Flutter web build | `flutter-web-release` |
| Flutter Linux build | `flutter-linux-release` |
| Flutter Linux `main_desktop.dart` | `flutter-linux-desktop-entrypoint` |
| Playwright | `playwright` |
| Puppeteer | `puppeteer` |

Static `needs` dependencies are validated for unknown nodes and cycles. Runs
execute in deterministic topological order and poll each accepted build to a
terminal result before submitting its dependents.

## Standalone meta self-test

`.github/workflows/gha-clone-server-meta.yml` is part of this repository and is
limited to the independent compiler's supported subset. It describes one Rust
verification job for the root crate and contains no secrets, dynamic matrices,
conditions, service containers, caller-selected working directory, or mutable
revision.

`tests/meta_self_test.rs` starts the real binary and a recording build-server
double, submits that exact local workflow through authenticated `POST /v1/runs`,
polls the run to terminal success, and verifies the outgoing request contains
only:

- `gha-indie-worker/gha-clone-server.rs`;
- `https://github.com/gha-indie-worker/gha-clone-server.rs.git`;
- one full immutable 40-hex commit SHA;
- `jobKind=run-profile`;
- the fixed `rust-verify` profile; and
- the deterministic plan/job request ID.

This proves the extracted repository no longer depends on the historical
`ORESoftware/k8s-cluster` filesystem layout or identity. The mock does not
execute untrusted repository code; it keeps pull-request CI hermetic while
exercising the real HTTP server, authentication, planner, run store,
dispatcher, polling, and terminal-state update.

## Failure-webhook contract

GitHub emits `workflow_run` completion events for every conclusion. Before any
independent dispatch, the service requires:

1. a valid `X-Hub-Signature-256` over the unmodified raw body;
2. a valid UUID `X-GitHub-Delivery`;
3. an exactly allowlisted repository;
4. a full lowercase immutable 40-hex `workflow_run.head_sha`;
5. `action=completed`;
6. a configured terminal conclusion;
7. a workflow name outside the exact recursion-exclusion set;
8. an exact configured `.github/workflows/*.yml|yaml` path;
9. a bounded workflow fetch and fully supported fixed-profile plan; and
10. an unclaimed delivery UUID within the bounded retention window.

The delivery claim is inserted only after workflow retrieval, planning, and
execution-readiness checks succeed. A transient pre-dispatch failure remains
retryable with the same delivery ID. Concurrent copies are serialized through
one process-local claim and can create at most one run set.

Delivery retention is bounded by TTL and entry count. It is process-local, so
webhook execution must remain single-replica until a shared durable claim store
or Fiducia-fenced ownership is proven.

## Fail-closed exclusions

The independent lane rejects, among other unsupported semantics:

- branches or tags in place of immutable commit IDs;
- secret/OIDC expressions in environment, action inputs, or commands;
- dynamic matrices and conditional jobs or steps;
- arbitrary marketplace actions;
- job or service containers;
- native macOS/iOS and Windows execution;
- environments, deployments, reusable workflows, and caller-selected commands.

Unsupported jobs remain candidates for the native GitHub/ARC lane; they are not
silently approximated here.

## Configuration

| Variable | Purpose |
| --- | --- |
| `GHA_CLONE_AUTH_SECRET` | operator/API authentication |
| `GHA_CLONE_GITHUB_WEBHOOK_SECRET` | GitHub webhook HMAC authority |
| `GHA_CLONE_GITHUB_TOKEN` | short-lived GitHub App installation token for exact workflow reads |
| `GHA_CLONE_GITHUB_API_BASE_URL` | GitHub API origin; production default `https://api.github.com` |
| `GHA_CLONE_BUILD_SERVER_URL` | internal executor-router/build-server origin |
| `GHA_CLONE_BUILD_SERVER_AUTH` | scoped downstream authentication |
| `GHA_CLONE_ALLOWED_REPOSITORIES` | exact comma-separated `owner/repo` allowlist |
| `GHA_CLONE_WORKFLOW_RULES_JSON` | exact repository-to-workflow-path map |
| `GHA_CLONE_EXECUTION_ENABLED` | authenticated manual execution gate |
| `GHA_CLONE_WEBHOOK_EXECUTION_ENABLED` | signed-webhook execution gate |
| `GHA_CLONE_WEBHOOK_FAILURE_CONCLUSIONS` | eligible terminal conclusions |
| `GHA_CLONE_WEBHOOK_IGNORED_WORKFLOWS` | exact recursion-exclusion names |
| `GHA_CLONE_WEBHOOK_DELIVERY_TTL_SECONDS` | nonzero delivery-claim TTL |
| `GHA_CLONE_MAX_WEBHOOK_DELIVERIES` | nonzero retained-delivery bound |
| `GHA_CLONE_MAX_WORKFLOW_BYTES` | parser input bound |
| `GHA_CLONE_MAX_JOBS` | workflow job bound |
| `GHA_CLONE_MAX_STEPS_PER_JOB` | per-job step bound |
| `GHA_CLONE_BUILD_TIMEOUT_SECONDS` | terminal build wait bound |

Invalid repository syntax, paths outside `.github/workflows`, traversal,
backslashes, duplicate paths, empty rule lists, unsafe API origins, or zero
retention/execution bounds cause startup to fail before the listener is bound.

Use repository-scoped GitHub Apps and External Secrets. Do not put classic PATs,
private keys, or shared secrets in source, Argo parameters, Linear, logs, URLs,
or image layers.

## Register a GitHub failure webhook

Run the helper only after the HTTPS route and matching runtime HMAC secret exist.
Authentication may come from `GH_TOKEN` or an existing `gh auth login`; the HMAC
secret is accepted only through `--secret-file`.

```console
bash scripts/register-github-webhook.sh \
  --repo gha-indie-worker/gha-clone-server.rs \
  --url https://ci.example.com/webhooks/github \
  --secret-file /secure/path/github_webhook_secret
```

For a true organization-level installation, use `--org <organization>`. The
helper subscribes only to `workflow_run` and:

- rejects symlinks and unreadable/non-regular secret files;
- normalizes one optional terminal line ending before validating the actual
  32–4096 byte visible-ASCII value;
- constructs the API payload through mode-`0600` temporary files and
  `jq --rawfile`;
- never accepts the HMAC secret from an environment variable, prints it, or
  places it in a process argument;
- rejects multiple hooks using the same exact callback URL; and
- verifies active state, event list, JSON content type, TLS verification, URL,
  and returned hook ID.

`tests/register_github_webhook.sh` uses a fake `gh` process to prove the secret
is absent from the child argument vector and covers duplicate URLs, newline
normalization, and symlink refusal.

## Source and deployment ownership

This repository is the standalone source publication extracted from
`ORESoftware/k8s-cluster`. Its own CI now verifies the local crate and local meta
fixture directly.

Production GitOps, ExternalSecrets, NetworkPolicies, exact repository/workflow
admission, fixed build-server bindings, gateway routing, and image digests remain
owned by `ORESoftware/k8s-cluster`. Source synchronization into a runtime image
must be explicit, immutable, byte/provenance verified, and separately reviewed;
this standalone repository does not mutate or auto-sync the cluster.

As of August 19, 2026, the reviewed `k8s-cluster` GitOps branch describes a
single-replica pilot with execution gates enabled for one exact
`ORESoftware/k8s-cluster` meta workflow. That statement describes repository
state, not a live-cluster probe from this repository's CI. Live claims still
require Argo reconciliation, rollout status, signed delivery evidence,
exact-SHA terminal execution evidence, and rollback proof from an authorized
cluster operator environment.
