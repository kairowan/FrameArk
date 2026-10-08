# Contributing to FrameArk

Thank you for helping build FrameArk. The project is pre-alpha, so architecture, testability, security boundaries, and honest compatibility claims take priority over feature count.

## Before starting

1. Read [PLAN.md](PLAN.md), [GOVERNANCE.md](GOVERNANCE.md), and the root [AGENTS.md](AGENTS.md).
2. Search existing issues and pull requests before opening overlapping work.
3. For substantial protocol, public API, persistence, security, or architecture changes, open a design issue or ADR before implementation.
4. Never work directly on `main` or `master`.

## Branch workflow

Start from the latest remote main branch:

```powershell
git fetch origin
git switch -c feat/short-description origin/main
python .codex/skills/frameark-development/scripts/guard_branch.py --repo .
```

Use one of these prefixes: `feat/`, `protocol/`, `template/`, `fix/`, `security/`, `refactor/`, `perf/`, `test/`, `docs/`, `ci/`, `chore/`, or `release/`.

Do not stash, reset, discard, amend, rebase, or force-push someone else's work. Use a separate Git worktree when parallel tasks would otherwise mix changes.

## Completion and commits

Every completed template, feature, protocol increment, or independently useful fix must have:

- implementation and cleanup paths;
- relevant tests and fixtures;
- diagnostics that do not leak secrets or media;
- documentation and compatibility updates;
- one focused Conventional Commit.

Examples:

```text
feat(protocol-fanp): negotiate session recovery
feat(android-receiver): render orientation changes
fix(media-clock): clamp negative drift samples
test(dlna): add malformed SOAP fixtures
docs(compatibility): record tested AirPlay devices
```

Run the branch guard and inspect the full staged diff before every commit. Stage only intended files.

## Code and architecture expectations

- Rust owns protocols, sessions, discovery, trust, timing, cross-platform policy, and diagnostics.
- Platform layers own operating-system APIs, capture, hardware codecs, surfaces, audio routing, lifecycle, and UI.
- Network parsers must bound messages, nesting, collections, queues, retries, timeouts, and memory.
- Async and FFI code must define cancellation, ownership, thread affinity, release order, and error propagation.
- Stable protocol claims require repeatable compatibility evidence and regression fixtures.
- FairPlay, Widevine, HDCP bypass, certified Cast behavior, and universal Miracast support are outside project claims.

## Protocol contributions

Protocol work must identify its specification or interoperability evidence, supported subset, maturity level, security model, state-machine mapping, and legal/licensing implications. Include valid and malformed fixtures, reconnect and version-skew tests, and redacted diagnostic events.

Do not submit packet captures containing personal media, credentials, persistent identifiers, or third-party secrets. Do not copy code from a project with an incompatible license.

## Pull requests

Use the pull-request template and include:

- user or protocol outcome;
- architecture and security impact;
- tests, fixtures, and reference devices;
- documentation and compatibility changes;
- known limitations, rollback, and follow-up work.

All review conversations and required checks must be resolved before merge. Changes enter `main` through a reviewed pull request; direct pushes are not accepted.

## Language and documentation

Code, identifiers, wire specifications, commit messages, and primary API documentation use English. User-facing documentation may be bilingual. Keep FrameArk/帧舟 naming and maturity labels consistent.

## License

Unless explicitly stated otherwise, contributions are accepted under the repository's `Apache-2.0 OR MIT` terms without additional conditions. By submitting a contribution, you represent that you have the right to license it accordingly.
