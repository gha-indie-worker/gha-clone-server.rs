# Source provenance and synchronization contract

This repository was split from `ORESoftware/k8s-cluster` at immutable commit `5cfac43c6900898f36f588d044ca34083da1c726`.

- Source path: `remote/deployments/gha-clone-server-rs`
- Standalone target: `gha-indie-worker/gha-clone-server.rs`
- Imported role: planner, workflow-subset compiler, run coordinator, and pre-submit executor router
- Import date: 2026-08-04
- Initial history policy: the import commit preserved every source file byte and executable mode; this provenance file was the only added file.

This code is an independent continuity lane, not a claim to reproduce GitHub's proprietary Actions control plane. Native workflow semantics remain GitHub-hosted Actions and Actions Runner Controller (ARC).

## Continuing synchronization

The standalone repository now has independently reviewed `dev` and protected `main` histories. A later synchronization from `ORESoftware/k8s-cluster` must therefore be a semantic pull request rather than a tree replacement.

Every synchronization must:

1. identify one full immutable 40-hex source commit and the exact source path;
2. compare source and target file bytes, hidden files, executable modes, and `Cargo.lock`;
3. preserve standalone-only CI, documentation, architecture contracts, and review history unless a reviewed replacement carries the same intent;
4. reconcile overlapping source changes conceptually instead of selecting an entire conflict side;
5. run formatting, warnings-denied Clippy, all locked tests, release builds, both hardened container targets, live health/readiness smoke, and full-history secret scanning;
6. merge through `dev` first and promote `dev` to `main` only through the protected independent-review gate;
7. never force-push, store a classic PAT, or publish a credential-bearing URL, log, artifact, or Actions output.

Publication and synchronization must fail closed when:

- the source revision is not one full immutable 40-hex commit;
- any source file, hidden file, executable mode, or `Cargo.lock` entry is omitted;
- the target introduces an unreviewed manual-only file instead of reconciling it with the source contract;
- the standalone meta test reaches outside this repository; or
- the native Rust, real-process, hardened-image, or full-history secret-scan matrix is not green.

The extraction-owned `.github/workflows/ci.yml` and `.github/workflows/gha-clone-server-meta.yml` are root-level workflows in this standalone repository. Their equivalent source copies may remain inert when nested inside the monorepo, but a publication must preserve their behavior and immutable action pins.

The machine-readable architecture contract under `docs/` remains authoritative for ownership, capability, and call-direction boundaries after extraction.
