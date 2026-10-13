#!/usr/bin/env python3
"""Require successful current GitHub Actions CI/security gates on a release commit."""
import argparse
import json
import sys


def gates_pass(payload: dict) -> bool:
    if not isinstance(payload, dict):
        return False
    checks = payload.get("check_runs", [])
    if not isinstance(checks, list) or type(payload.get("total_count")) is not int or payload["total_count"] != len(checks):
        return False  # A truncated response cannot prove the latest gate.
    if any(not isinstance(c, dict) or not isinstance(c.get("app"), dict) or type(c.get("id")) is not int for c in checks):
        return False
    for name in ("ci-gate", "security-gate"):
        candidates = [c for c in checks if c.get("name") == name and c.get("app", {}).get("slug") == "github-actions"]
        if not candidates:
            return False
        current = max(candidates, key=lambda c: c["id"])
        if current.get("status") != "completed" or current.get("conclusion") != "success":
            return False
    return True


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check-runs", required=True)
    args = parser.parse_args()
    try:
        with open(args.check_runs, encoding="utf-8") as stream:
            passed = gates_pass(json.load(stream))
    except (OSError, ValueError, TypeError, KeyError):
        passed = False
    if not passed:
        print("Release requires complete, successful main CI and security checks; retry after they finish.", file=sys.stderr)
        return 1
    print("Release commit CI/security gates passed.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
