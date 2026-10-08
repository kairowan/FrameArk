# FrameArk governance

FrameArk is maintained as an open-source project with transparent technical decisions, reviewable changes, and explicit compatibility boundaries.

## Roles

- **Maintainers** set roadmap priorities, review changes, manage releases, handle security reports, and protect project integrity.
- **Contributors** propose issues, designs, code, tests, documentation, compatibility results, and reviews.
- **Committers**, when appointed, may review and merge in defined areas but remain subject to branch protection and review requirements.

The initial repository owner and maintainer is `@kairowan`. Additional maintainers should be added through a documented pull request describing their responsibility area.

## Decision making

Routine changes are decided through pull-request review. Changes to wire protocols, public API/ABI, persistence, trust, security posture, crate boundaries, compatibility promises, or licensing require an Architecture Decision Record and maintainer approval.

When consensus is not immediate, maintainers summarize the alternatives, evidence, compatibility impact, and decision. Decisions may be revisited when new interoperability or performance evidence appears.

## Repository policy

- `main` is protected and receives changes only through pull requests.
- At least one approval and all required checks are needed before merge.
- Authors do not bypass failed checks or unresolved review conversations.
- Force pushes and branch deletion are blocked on protected branches.
- Releases are created from reviewed commits and include signed or verifiable artifacts when release automation is available.

Detailed contributor workflow is in [CONTRIBUTING.md](CONTRIBUTING.md). Required GitHub settings are documented in [docs/BRANCH_PROTECTION.md](docs/BRANCH_PROTECTION.md).

## Releases and compatibility

Stable claims require a named compatibility matrix, regression coverage, security review, documentation, migration guidance, and known limitations. Experimental or conditional modules are isolated and never silently promoted to Stable.

Release notes identify supported protocols, platforms, configuration migrations, security changes, and artifact checksums. Versioning rules are defined before the first public binary release.

## Conduct and enforcement

Participation is governed by [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md). Security reports follow [SECURITY.md](SECURITY.md). Maintainers may restrict access when conduct, security, licensing, or repository-integrity requirements are repeatedly violated.

