# GitHub repository protection

This document records the required GitHub settings for `kairowan/FrameArk`. Workflow files cannot enforce repository settings by themselves; a repository administrator must apply and periodically audit these controls.

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

Review this configuration before every stable release and at least quarterly. Confirm that required check names still match workflow job names and that no broad bypass actor or write-token permission has been added.

