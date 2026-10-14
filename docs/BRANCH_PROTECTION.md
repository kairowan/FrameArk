# GitHub repository protection

This document records the required GitHub settings for `kairowan/FrameArk`. Workflow files cannot enforce repository settings by themselves; a repository administrator must apply and periodically audit these controls.

## Protection recovery — 2026-10-14

The owner restored public visibility. The environment again returns required
reviewer `kairowan`, `prevent_self_review=false`, `can_admins_bypass=false` and
the tag-only `v*` deployment policy. No production environment secrets are set.
Main protection did **not** return automatically (the API reported "Branch not
protected"); it was reapplied and read back with strict status checks for
`pull-request-policy`, `ci-gate`, `security-gate`, `dependency-review`, one approval,
CODEOWNERS review, stale-review dismissal, conversation resolution and linear
history, with force pushes/deletion disabled. Administrator enforcement remains
disabled solely for the existing owner-authorized single-maintainer exception.

This verifies configuration recovery, not successful execution of every check or
a production release. Require fresh successful checks on the current PR head and
main before merging/releasing. No paid plan, organization transfer, production
key, tag or release was created during recovery.

### Historical interruption — 2026-10-13

While the repository was private, the main protection endpoint returned HTTP 403
(plan/visibility restriction), dependency review was unsupported and CodeQL upload
reported code scanning disabled. `android-release` returned only `branch_policy`,
not required reviewers. The environment secret count was zero at that audit.

The settings below are required policy and must be checked against effective
configuration. On any recurrence, stop merges and production signing setup until
the owner chooses a supported plan and controls are restored and verified. Do not
make a private repository public without explicit permission or weaken checks
to pass CI. The bootstrap exception waives impossible independent self-approval;
it does not waive CI/security or missing environment protection. A plan upgrade
must be checked against every required feature, not just branch protection.

Audit the actual API/UI configuration after recovery, then rerun checks on the
current PR head. Do not infer recovery from the environment name or workflow YAML.

## Main branch protection

Protect `refs/heads/main` with a branch protection rule or equivalent active ruleset:

- require a pull request before merging;
- require at least one approval;
- dismiss stale approvals when new commits are pushed;
- require review from CODEOWNERS when a matching owner exists;
- require all conversations to be resolved;
- require the branch to be up to date before merging;
- require the unique `pull-request-policy`, `ci-gate`, `security-gate`, and `dependency-review` checks after they have reported at least once;
- block force pushes and deletion;
- do not allow routine bypasses.

During the single-maintainer bootstrap, administrator enforcement may remain disabled so the owner is not permanently blocked from a pull request that cannot receive an independent approval. This is an emergency/bootstrap escape hatch, not permission to develop or push directly on `main`. Enable administrator enforcement as soon as a second trusted reviewer is available.

Do not require optional language jobs individually. The aggregate `ci-gate` and `security-gate` checks remain stable while Rust, Android, and other language subtrees are introduced.

## Merge and repository settings

- Set `main` as the default branch after the bootstrap pull request is merged.
- Prefer rebase merge to preserve atomic feature/protocol commits; allow squash only for a pull request that is itself one logical unit.
- Delete head branches after merge.
- Disable direct pushes through the ruleset rather than relying on convention.
- Enable automatic branch updates where appropriate.

## Security settings

Enable, where available for the repository visibility and plan:

- dependency graph;
- Dependabot alerts and security updates;
- secret scanning and push protection;
- private vulnerability reporting;
- CodeQL default or advanced setup;
- GitHub Actions workflow permissions set to read repository contents by default.

Grant write permissions only to jobs that publish a reviewed artifact, advisory, or release. Pin third-party actions to reviewed major versions or immutable commit SHAs.

## Audit schedule

Android release secrets must live in `android-release`: required reviewer `kairowan`,
administrator bypass disabled, and a tag-only `v*` deployment policy. The sole
maintainer may approve their own deployment; require independent review when a
second trusted maintainer joins. The workflow additionally checks exact annotated
tag/checkout identity, reviewed main ancestry and successful main CI/security.
Do not move or reuse published tags. See [signing setup](ANDROID_RELEASE_SIGNING.md).

Review this configuration before every stable release and at least quarterly. Confirm that required check names still match workflow job names and that no broad bypass actor or write-token permission has been added.
