# FrameArk agent instructions

Use the repository skill at `.codex/skills/frameark-development/SKILL.md` for every FrameArk implementation, protocol, template, CI/CD, release, or repository-governance task.

Non-negotiable project rules:

- Never edit, commit, or push directly on `main` or `master`.
- Use a focused branch with an approved prefix and run the skill's branch guard before editing and committing.
- Preserve unrelated user changes and stage only intended files.
- Every completed template, feature, protocol increment, or independent fix receives a verified atomic Conventional Commit.
- Keep protocol and session state in Rust; keep platform code focused on OS APIs, codecs, rendering, audio, lifecycle, and UI.
- Update tests, diagnostics, public documentation, compatibility status, and security notes with the code they describe.
- Do not claim certified Cast behavior, universal Miracast support, or DRM/HDCP bypass.
- Reach `main` only through a reviewed pull request with required CI and security checks.

Read `PLAN.md` for the full product roadmap and architecture before making scope decisions.
