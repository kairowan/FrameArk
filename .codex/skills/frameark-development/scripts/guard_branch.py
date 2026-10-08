#!/usr/bin/env python3
"""Fail closed when FrameArk development is attempted on an unsafe Git branch."""

from __future__ import annotations

import argparse
import re
import subprocess
import sys
from pathlib import Path


ALLOWED_BRANCH = re.compile(
    r"^(feat|protocol|template|fix|security|refactor|perf|test|docs|ci|chore|release)/"
    r"[a-z0-9][a-z0-9._-]*$"
)
ALLOWED_REMOTES = {
    "https://github.com/kairowan/FrameArk.git",
    "git@github.com:kairowan/FrameArk.git",
    "ssh://git@github.com/kairowan/FrameArk.git",
}
PROTECTED_BRANCHES = {"main", "master"}


def git(repo: Path, *args: str, required: bool = True) -> str:
    result = subprocess.run(
        ["git", "-C", str(repo), *args],
        check=False,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    if required and result.returncode != 0:
        detail = result.stderr.strip() or result.stdout.strip()
        raise RuntimeError(f"git {' '.join(args)} failed: {detail}")
    return result.stdout.strip()


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo", default=".", help="FrameArk repository root")
    parser.add_argument(
        "--require-clean",
        action="store_true",
        help="also reject staged, unstaged, or untracked changes",
    )
    args = parser.parse_args()
    repo = Path(args.repo).resolve()

    try:
        root = Path(git(repo, "rev-parse", "--show-toplevel")).resolve()
        branch = git(root, "symbolic-ref", "--quiet", "--short", "HEAD")
        remote = git(root, "remote", "get-url", "origin")
    except RuntimeError as exc:
        print(f"FrameArk branch guard: {exc}", file=sys.stderr)
        return 2

    failures: list[str] = []
    if branch in PROTECTED_BRANCHES:
        failures.append(f"'{branch}' is protected; create a focused working branch")
    elif not ALLOWED_BRANCH.fullmatch(branch):
        failures.append(f"branch '{branch}' does not use an allowed FrameArk prefix")

    if remote not in ALLOWED_REMOTES:
        failures.append(f"origin is '{remote}', expected kairowan/FrameArk")

    if args.require_clean and git(root, "status", "--porcelain"):
        failures.append("the worktree is not clean")

    if failures:
        for failure in failures:
            print(f"FrameArk branch guard: {failure}", file=sys.stderr)
        return 1

    print(f"FrameArk branch guard passed: branch={branch} origin={remote}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

