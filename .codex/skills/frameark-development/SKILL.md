---
name: frameark-development
description: Build, extend, review, or maintain the FrameArk screen-mirroring project, including its Rust core, platform apps, templates, protocols, CI/CD, releases, and repository governance. Use for work in kairowan/FrameArk or when the user asks for FrameArk features, FANP, AirPlay/RAOP, DLNA, Cast compatibility, Miracast integration, or project setup; do not use for unrelated casting projects.
---

# FrameArk Development

Deliver FrameArk changes as tested, documented, reviewable units while protecting the main branch and the project's protocol boundaries.

## Establish context

1. Confirm the repository from `origin` or the root `PLAN.md`. The canonical remote is `https://github.com/kairowan/FrameArk.git`.
2. Read the repository `AGENTS.md` and the sections of `PLAN.md` relevant to the requested change.
3. Inspect the current branch, worktree, remotes, and recent commits. Preserve unrelated user changes.
4. Before editing, run `python .codex/skills/frameark-development/scripts/guard_branch.py --repo .` from the repository root. Use the installed skill copy only when the repository copy is unavailable.

Never edit or commit on `main` or `master`. If the current branch is protected, fetch `origin` and create a focused branch from `origin/main`. If unrelated local changes make that unsafe, stop and report the conflict rather than stashing, resetting, or moving them without authorization. For an empty repository, create a non-protected bootstrap branch.

## Route the work

- For architecture, module ownership, platform boundaries, or protocol maturity, read [references/architecture.md](references/architecture.md).
- For a new feature, template, protocol, bug fix, or security change, read [references/development-checklists.md](references/development-checklists.md) and use only the applicable checklist.
- For branches, commits, pull requests, CI, releases, or GitHub settings, read [references/git-governance.md](references/git-governance.md).

## Implement within project boundaries

- Keep protocol, session, discovery, timing, trust, and cross-platform policy in Rust.
- Keep Android, desktop, and web layers responsible for OS APIs, capture, codec surfaces, rendering, audio routing, lifecycle, and UI.
- Reuse the unified device, capability, session, track, event, and error models; do not create a parallel state machine for one platform.
- Treat FANP as the primary controlled protocol. Mark AirPlay/RAOP and DLNA claims by tested compatibility, Cast V2 as compatibility mode, Miracast as conditional, and DRM/HDCP bypass as unsupported.
- Add bounded parsing, cancellation, timeouts, resource cleanup, redacted diagnostics, and explicit unsupported-state errors at network boundaries.
- Do not copy incompatible third-party code into the dual-licensed core. Record dependencies and license implications.

## Verify and commit every completed unit

A new template, feature, protocol increment, or independently useful fix is not complete until its relevant tests, documentation, and diagnostics pass and it has its own Git commit.

Before each commit:

1. Run the branch guard again.
2. Inspect `git diff` and `git status`; stage only intended paths.
3. Run the narrowest relevant tests plus repository policy checks. Expand testing in proportion to protocol, FFI, security, or media risk.
4. Update public behavior, compatibility, configuration, security, and migration documentation in the same logical unit.
5. Commit with a Conventional Commit message and a meaningful FrameArk scope.

Do not combine unrelated protocols or features in one commit. Do not rewrite or force-push shared history. Never claim completion while intended changes remain uncommitted.

Local commits are part of the FrameArk development workflow. Push branches, open or modify pull requests, change repository settings, publish releases, or merge only when the current task authorizes the corresponding external action.

## Finish with evidence

Report:

- the working branch;
- commit hashes and subjects created for the task;
- tests and checks run, including anything unavailable or skipped;
- documentation and compatibility changes;
- push and pull-request status;
- remaining risks or follow-up work.

