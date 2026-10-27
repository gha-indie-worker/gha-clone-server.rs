# Bounded CI continuity platform boundary

This document explains [`platform-boundary.json`](platform-boundary.json), tracked by Linear
issue `DEN-1606`.

`gha-clone-server` is an independent continuity control plane for a deliberately bounded
workflow subset. It does **not** claim parity with GitHub Actions and does not reproduce
GitHub's proprietary control plane.

## Canonical ownership

```text
GitHub-hosted runners / official ARC
    own official GitHub Actions semantics

Agent Pontifex
    owns coding-agent policy, model routing, approvals, and task evidence
              |
              v
GHA clone server
    owns bounded parsing, classification, immutable plans, and run coordination
              |
              v
GHA executor router
    owns provider selection and provider-pinned status
              |
              v
DD build server
    owns fixed-profile queueing, isolation, logs, and artifacts

Fiducia
    owns distributed fencing when claims or irreversible dispatch can race

k8s-cluster
    owns runner/deployment tenancy, shared secrets delivery, and observability backends
```

The organization name `gha-indie-worker` does not create a runtime dependency on an external
`gha-indie-worker` project. External implementations are design provenance only. The Rust
service in this repository remains the product authority for the bounded independent lane.

## Two lanes

The normal lane uses GitHub-hosted runners or official Actions Runner Controller. Existing
workflow YAML receives GitHub's actual expression evaluation, marketplace action behavior,
checks, artifacts, and runner protocol.

The independent lane accepts only a static subset it can prove safe and deterministic. It
parses and classifies the workflow, builds an immutable plan, and maps fully supported jobs
to operator-reviewed fixed profiles. Unsupported behavior is returned explicitly and is
never approximated.

## Request boundary

A planning or execution request contains:

- an exact allowlisted `owner/repository`;
- a full immutable 40-character lowercase hexadecimal commit SHA;
- one direct `.github/workflows/*.yml` or `.yaml` path;
- a deterministic idempotency key;
- W3C trace context.

The GitHub workflow response is bounded before parsing. Redirect following is disabled. The
fetched repository and path must exactly match the request and allowlist.

Branches and tags are useful discovery inputs but are never execution authority. The server
also rejects traversal, indirect workflow locations, caller-selected working directories,
runner images, Kubernetes manifests, and secret/OIDC expressions in the independent lane.

## YAML boundary

The parser rejects YAML tags, merge keys, duplicate keys, multiple documents, confusable
job/step keys, dynamic matrices, dynamic conditions, containers, reusable workflows,
unsupported marketplace actions, unknown `needs` nodes, and cycles.

Support classification is evidence, not a guess. A workflow is executable independently only
when every required job and dependency has a reviewed fixed-profile mapping.

## Execution boundary

The target dispatch path is:

```text
gha-clone-server -> gha-executor-router -> dd-build-server
```

The router selects an allowed provider and returns provider-pinned status. The build server
executes the fixed profile and owns logs and artifacts.

A transitional direct call to one reviewed build-server endpoint is acceptable only while the
clone server submits a fixed profile and immutable revision. It must not evolve into provider
selection, arbitrary command selection, or a second build executor.

A profile request contains repository, immutable commit SHA, fixed profile, deterministic
request id, and trace context. Returned run IDs are validated before constructing status,
log, or artifact URLs.

## Admission and horizontal scaling

Every workflow, job, step, active-run, poll, timeout, retention, and upstream-body limit is
strictly positive. Process-local active capacity is reserved before creating an API or
webhook execution task and released on every rejected or terminal path.

Webhook execution stays at one replica while delivery claims are process-local. Multiple
replicas may enable mutations only after a shared durable delivery claim or Fiducia-fenced
claim prevents duplicate authorization at the dispatch boundary.

Scaling HTTP availability without scaling claim authority is unsafe.

## Authentication

The API uses a server-owned secret compared in constant time over a digest. Private workflow
reads use a short-lived installation-scoped GitHub App token. The build transport uses a
separate scoped service credential.

Personal access tokens, GitHub App private keys, and service credentials never enter source,
task payloads, URLs, logs, or artifacts. GitHub, planning, and execution boundaries do not
reuse one ambient credential.

## Failure semantics

- Unsupported workflows return explicit plan evidence and do not execute.
- Ambiguous build submissions reconcile by deterministic request ID before retry.
- A GitHub fetch or planning failure leaves the webhook delivery retryable if no dispatch
  claim was committed.
- Capacity exhaustion rejects or defers before spawning an execution task.
- A stale fencing token rejects protected dispatch.
- A terminal polling timeout records evidence and prevents dependent job submission.

A duplicate event may produce the same observable terminal result, but never a second
external effect.

## Observability and audit

W3C trace context propagates through planning, routing, and build submission. Logs and traces
record bounded route, lane, profile, timing, and outcome evidence. Repository, commit,
workflow path, request, run, and delivery identifiers are not Prometheus or Loki stream
labels.

API secrets, GitHub tokens, build credentials, authorization headers, secret expression
values, and repository source archives do not enter ordinary telemetry.

Audit evidence records the immutable input, support classification, fixed profile, executor
route, build request ID, terminal result, and rejection reason. Operational logs are not the
sole audit authority.

## Deployment

API and webhook execution default to disabled. Activation proceeds in this order:

1. Provision ExternalSecret values.
2. Verify exact repository and workflow allowlists.
3. Verify fixed profiles and executor routes.
4. Deploy one dormant replica by immutable signed image digest.
5. Run plan-only fixtures.
6. Enable immutable API execution.
7. Enable failure-webhook execution last.

The workflow mirror remains a continuity lane, not an excuse to run unreviewed repository
commands when GitHub capacity is unavailable.
