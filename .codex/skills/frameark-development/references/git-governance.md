# FrameArk Git and GitHub governance

Read this reference for branch creation, commits, pull requests, CI/CD, releases, or repository settings.

## Protected branches

Never edit, commit, or push directly to `main` or `master`. Changes reach `main` only through a reviewed pull request with required checks.

Start ordinary branches from the latest `origin/main` after inspecting local changes. Use a separate worktree when parallel work would otherwise mix unrelated changes.

Allowed branch prefixes:

| Prefix | Use |
|---|---|
| `feat/` | Product feature or module |
| `protocol/` | Protocol implementation or interoperability work |
| `template/` | Reusable template or scaffold |
| `fix/` | Bug fix |
| `security/` | Non-embargoed security hardening |
| `refactor/` | Behavior-preserving restructuring |
| `perf/` | Measured performance work |
| `test/` | Test infrastructure or fixtures |
| `docs/` | Documentation only |
| `ci/` | Automation or repository policy |
| `chore/` | Maintenance and bootstrap work |
| `release/` | Release preparation |

Use lowercase, short hyphenated names, for example `protocol/fanp-session-resume`.

## Atomic commit policy

Create one commit whenever a template, feature, protocol increment, or independently useful fix reaches its completion gate. Include its tests and directly required documentation in that commit. Split unrelated work before committing.

Use Conventional Commits:

```text
feat(protocol-fanp): negotiate session recovery
feat(android-receiver): render dynamic orientation changes
fix(media-clock): clamp negative drift samples
test(dlna): add malformed SOAP replay fixtures
docs(compatibility): record AirPlay beta matrix
ci(github): enforce pull-request policy checks
```

Before committing, run the branch guard, inspect the complete diff, verify tests, and stage only intended paths. Do not use `git add -A` when unrelated files exist. Never reset, discard, amend, rebase, or force-push user work without explicit authority.

## Pull requests

Each pull request must describe:

- the user or protocol outcome;
- architecture and security impact;
- test evidence and reference hardware;
- documentation and compatibility changes;
- known limitations, rollback, and follow-up work.

Keep pull requests focused. Protocol changes include fixtures/specification updates; UI screenshots are needed only when UI changed. Resolve every review conversation and rerun checks after the branch is updated.

## Required repository controls

Protect `main` with a GitHub ruleset:

- require pull requests and at least one approval;
- dismiss stale approvals and require CODEOWNERS review where applicable;
- require conversation resolution;
- block force pushes and branch deletion;
- require the repository policy and CI gate checks;
- require the branch to be up to date before merge;
- enable dependency graph, Dependabot alerts/updates, secret scanning, push protection, and code scanning when available.

Do not make optional or environment-dependent matrix jobs individually required. Require stable aggregate gate jobs so a missing Android or Rust subtree does not permanently block bootstrap pull requests.

## CI expectations

- Every pull request: branch/title/commit policy, repository document check, Rust fmt/clippy/test when present, Android lint/test when present, and dependency review.
- Main and pull requests: CodeQL for languages present in the repository and workflow analysis.
- Scheduled: extended tests, vulnerability audit, fuzz/fixture replay, and stale dependency detection as those suites become available.
- Release: tag validation, full tests, reproducible artifacts, checksums, SBOM, signatures, release notes, and compatibility matrix.

Actions use least-privilege permissions and pinned major versions or immutable SHAs. Any action that gains write permission requires explicit review.

## External actions

Commits are local project artifacts. Pushing, opening a pull request, editing a ruleset, merging, publishing packages/releases, or posting review comments changes GitHub state; perform those actions only when the current task authorizes them. Report the exact branch and commit hashes even when publication is pending.

