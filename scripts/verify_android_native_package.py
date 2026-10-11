#!/usr/bin/env python3
"""Verify that an Android APK contains the Rust JNI bridge for every build."""

from __future__ import annotations

import argparse
import sys
import zipfile
from pathlib import Path


EXPECTED_ENTRY = "lib/arm64-v8a/libframeark_ffi.so"


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--apk", required=True, type=Path)
    args = parser.parse_args()
    if not args.apk.is_file():
        print(f"Android APK does not exist: {args.apk}", file=sys.stderr)
        return 1
    try:
        with zipfile.ZipFile(args.apk) as archive:
            try:
                entry = archive.getinfo(EXPECTED_ENTRY)
            except KeyError:
                print(f"Android APK is missing {EXPECTED_ENTRY}", file=sys.stderr)
                return 1
            if entry.file_size <= 0:
                print(f"Android APK contains an empty {EXPECTED_ENTRY}", file=sys.stderr)
                return 1
    except (OSError, zipfile.BadZipFile) as error:
        print(f"Android APK cannot be inspected: {error}", file=sys.stderr)
        return 1
    print(f"Android native package passed: {args.apk} contains {EXPECTED_ENTRY}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
