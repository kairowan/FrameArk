#!/usr/bin/env python3
"""Validate FrameArk repository policy without third-party Python packages."""

from __future__ import annotations

import argparse
import re
import subprocess
import sys
from pathlib import Path
from urllib.parse import unquote, urlparse


REQUIRED_FILES = {
    "AGENTS.md",
    "PLAN.md",
    "README.md",
    "CONTRIBUTING.md",
    "GOVERNANCE.md",
    "SECURITY.md",
    "CODE_OF_CONDUCT.md",
    "LICENSE",
    "LICENSE-APACHE",
    "LICENSE-MIT",
    "docs/BRANCH_PROTECTION.md",
    "docs/COMPATIBILITY.md",
    ".codex/skills/frameark-development/SKILL.md",
}
ALLOWED_BRANCH = re.compile(
    r"^(feat|protocol|template|fix|security|refactor|perf|test|docs|ci|chore|release)/"
    r"[a-z0-9][a-z0-9._-]*$"
)
CONVENTIONAL_COMMIT = re.compile(
    r"^(feat|fix|docs|ci|chore|refactor|perf|test|build|revert|security)"
    r"(?:\([a-z0-9][a-z0-9._-]*\))?!?: .+"
)
MARKDOWN_LINK = re.compile(r"!?\[[^\]]*\]\(([^)]+)\)")
FORBIDDEN_SUFFIXES = {
    ".jks",
    ".keystore",
    ".key",
    ".p12",
    ".pfx",
    ".pcap",
    ".pcapng",
    ".pem",
}
MAX_FILE_BYTES = 5 * 1024 * 1024


def git(root: Path, *args: str) -> str:
    result = subprocess.run(
        ["git", "-C", str(root), *args],
        check=False,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    if result.returncode != 0:
        detail = result.stderr.strip() or result.stdout.strip()
        raise RuntimeError(f"git {' '.join(args)} failed: {detail}")
    return result.stdout.strip()


def repository_files(root: Path) -> list[Path]:
    output = git(root, "ls-files", "--cached", "--others", "--exclude-standard")
    return [root / line for line in output.splitlines() if line]


def check_required_files(root: Path, failures: list[str]) -> None:
    for relative in sorted(REQUIRED_FILES):
        if not (root / relative).is_file():
            failures.append(f"missing required repository file: {relative}")


def check_file_safety(root: Path, files: list[Path], failures: list[str]) -> None:
    for path in files:
        if not path.is_file():
            continue
        relative = path.relative_to(root).as_posix()
        suffix = path.suffix.lower()
        if suffix in FORBIDDEN_SUFFIXES:
            failures.append(f"forbidden sensitive/capture file is tracked: {relative}")
        if path.stat().st_size > MAX_FILE_BYTES:
            failures.append(f"file exceeds 5 MiB policy limit: {relative}")


def local_link_target(raw_target: str) -> str | None:
    target = raw_target.strip()
    if target.startswith("<") and ">" in target:
        target = target[1 : target.index(">")]
    elif " " in target:
        target = target.split(" ", 1)[0]
    target = unquote(target).split("#", 1)[0]
    if not target or target.startswith("#"):
        return None
    parsed = urlparse(target)
    if parsed.scheme or target.startswith("//"):
        return None
    return target


def check_markdown_links(root: Path, files: list[Path], failures: list[str]) -> None:
    for path in files:
        if path.suffix.lower() != ".md" or not path.is_file():
            continue
        try:
            content = path.read_text(encoding="utf-8")
        except UnicodeDecodeError:
            failures.append(f"Markdown file is not UTF-8: {path.relative_to(root)}")
            continue
        for match in MARKDOWN_LINK.finditer(content):
            target = local_link_target(match.group(1))
            if target is None:
                continue
            resolved = (path.parent / target).resolve()
            if not resolved.exists():
                relative = path.relative_to(root).as_posix()
                failures.append(f"broken local Markdown link in {relative}: {target}")


def check_workflows(root: Path, failures: list[str]) -> None:
    workflow_dir = root / ".github" / "workflows"
    if not workflow_dir.exists():
        failures.append("missing .github/workflows")
        return
    mutable_ref = re.compile(r"^\s*uses:\s*[^\s]+@(main|master|latest)\s*$", re.MULTILINE)
    for path in sorted(workflow_dir.glob("*.y*ml")):
        content = path.read_text(encoding="utf-8")
        relative = path.relative_to(root).as_posix()
        if not re.search(r"^permissions:\s*(?:\{|$)", content, re.MULTILINE):
            failures.append(f"workflow lacks top-level least-privilege permissions: {relative}")
        if mutable_ref.search(content):
            failures.append(f"workflow uses a mutable action ref: {relative}")


def check_pull_request(
    root: Path,
    branch: str | None,
    title: str | None,
    base_sha: str | None,
    failures: list[str],
) -> None:
    if branch and not ALLOWED_BRANCH.fullmatch(branch):
        failures.append(f"pull-request branch uses a disallowed name: {branch}")
    if title and not CONVENTIONAL_COMMIT.fullmatch(title):
        failures.append(f"pull-request title is not Conventional Commit style: {title}")
    if not base_sha:
        return
    try:
        subjects = git(root, "log", "--format=%s", f"{base_sha}..HEAD").splitlines()
    except RuntimeError as exc:
        failures.append(str(exc))
        return
    for subject in subjects:
        if not CONVENTIONAL_COMMIT.fullmatch(subject):
            failures.append(f"non-conventional commit subject: {subject}")


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo", default=".", help="repository root")
    parser.add_argument("--head-ref", help="pull-request source branch")
    parser.add_argument("--pr-title", help="pull-request title")
    parser.add_argument("--base-sha", help="pull-request base commit")
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    root = Path(args.repo).resolve()
    failures: list[str] = []
    try:
        root = Path(git(root, "rev-parse", "--show-toplevel")).resolve()
        files = repository_files(root)
    except RuntimeError as exc:
        print(f"repository policy: {exc}", file=sys.stderr)
        return 2

    check_required_files(root, failures)
    check_file_safety(root, files, failures)
    check_markdown_links(root, files, failures)
    check_workflows(root, failures)
    check_pull_request(root, args.head_ref, args.pr_title, args.base_sha, failures)

    if failures:
        for failure in failures:
            print(f"repository policy: {failure}", file=sys.stderr)
        return 1

    print(f"Repository policy passed for {len(files)} files.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
