#!/usr/bin/env python3
"""Run a bounded cross-platform startup smoke for the reference receiver."""

from __future__ import annotations

import argparse
import os
import subprocess
import sys
import time
from pathlib import Path


def resolve_binary(raw: str) -> Path:
    path = Path(raw)
    if path.is_file():
        return path
    if os.name == "nt" and path.suffix.lower() != ".exe":
        executable = path.with_suffix(".exe")
        if executable.is_file():
            return executable
    raise FileNotFoundError(f"receiver binary was not found: {raw}")


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", required=True, help="path to framearkd")
    parser.add_argument(
        "--wait-seconds",
        type=float,
        default=3.0,
        help="bounded startup observation window",
    )
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    if not 0.5 <= args.wait_seconds <= 30:
        raise SystemExit("--wait-seconds must be between 0.5 and 30")
    binary = resolve_binary(args.binary)
    process = subprocess.Popen(
        [
            str(binary),
            "--bind",
            "127.0.0.1:0",
            "--server-name",
            "localhost",
            "--pairing-code",
            "123456",
        ],
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
    )
    try:
        time.sleep(args.wait_seconds)
        status = process.poll()
        if status is not None:
            stdout, stderr = process.communicate()
            print(
                f"framearkd exited during startup with status={status}\n"
                f"stdout={stdout[-2000:]}\n"
                f"stderr={stderr[-2000:]}",
                file=sys.stderr,
            )
            return 1
        print(f"framearkd remained alive for {args.wait_seconds:.1f}s on {os.name}")
        return 0
    finally:
        if process.poll() is None:
            process.terminate()
            try:
                process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait(timeout=5)
        if process.stdout is not None:
            process.stdout.close()
        if process.stderr is not None:
            process.stderr.close()


if __name__ == "__main__":
    raise SystemExit(main())
