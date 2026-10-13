#!/usr/bin/env python3
"""Fail-closed dual-APK release preparation/signing; no third-party Python dependencies."""

from __future__ import annotations

import argparse
import base64
import hashlib
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
import time
import zipfile
from pathlib import Path
from verify_third_party_notices import ASSET_NAMES, verify as verify_notices

APPS = ("receiver", "sender")
SECRET_NAMES = ("ANDROID_KEYSTORE_BASE64", "ANDROID_KEYSTORE_PASSWORD", "ANDROID_KEY_ALIAS", "ANDROID_KEY_PASSWORD")
HEX = re.compile(r"[0-9a-f]{64}")
SEMVER = re.compile(r"v(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)(?:-([0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*))?(?:\+([0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*))?")


class ReleaseError(Exception):
    """Only static diagnostics are safe to log across the signing boundary."""


def require(condition: bool, stage: str) -> None:
    if not condition:
        raise ReleaseError(stage)


def version(tag: str) -> str:
    match = SEMVER.fullmatch(tag)
    require(match is not None and len(tag) <= 100, "invalid semantic release tag")
    assert match is not None
    prerelease = match.group(4)
    require(not prerelease or all(not p.isdigit() or p == "0" or not p.startswith("0") for p in prerelease.split(".")), "invalid prerelease identifier")
    return tag[1:]


def run(args: list[str | Path], stage: str, env: dict[str, str] | None = None) -> str:
    try:
        result = subprocess.run([str(a) for a in args], stdin=subprocess.DEVNULL, capture_output=True, text=True, encoding="utf-8", errors="replace", timeout=120, env=public_environment() if env is None else env, check=False)
    except (OSError, subprocess.TimeoutExpired) as exc:
        raise ReleaseError(stage) from exc
    require(result.returncode == 0, stage)
    return result.stdout


def public_environment() -> dict[str, str]:
    inherited = ("PATH", "JAVA_HOME", "SystemRoot", "WINDIR", "TEMP", "TMP", "TMPDIR", "LANG", "LC_ALL")
    return {name: os.environ[name] for name in inherited if name in os.environ}


def tool(sdk_tools: Path, name: str) -> Path:
    path = sdk_tools / (name + (".exe" if os.name == "nt" else ""))
    require(path.is_file(), "Android build tool missing")
    return path


def signer(sdk_tools: Path) -> list[str | Path]:
    jar = sdk_tools / "lib" / "apksigner.jar"
    require(jar.is_file(), "apksigner missing")
    return ["java", "-jar", jar]


def digest(path: Path) -> str:
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def verify_apk(path: Path, sdk_tools: Path, app: str, name: str, code: int) -> dict[str, str]:
    require(path.is_file() and not path.is_symlink(), "release APK missing")
    badging = run([tool(sdk_tools, "aapt"), "dump", "badging", path], "APK manifest validation failed")
    package = re.search(r"^package: name='([^']+)' versionCode='([^']+)' versionName='([^']+)'", badging, re.MULTILINE)
    require(package is not None and package.groups() == (f"dev.frameark.{app}", str(code), name), "APK identity/version mismatch")
    require("application-debuggable" not in badging, "debug APK cannot be released")
    with zipfile.ZipFile(path) as archive:
        names = archive.namelist()
        require(len(names) == len(set(names)), "duplicate APK ZIP entry")
        libs = [entry for entry in names if entry.startswith("lib/")]
        require(libs == ["lib/arm64-v8a/libframeark_ffi.so"], "APK must contain only the production ARM64 bridge")
        library = archive.read(libs[0])
        require(library[:6] == b"\x7fELF\x02\x01" and library[18:20] == b"\xb7\x00", "invalid ARM64 ELF")
        require(b"PlaybackNetworkProbe" not in library, "native probe present")
        for entry in names:
            if re.fullmatch(r"classes[0-9]*\.dex", entry):
                data = archive.read(entry)
                require(b"CaptureFixtureActivity" not in data and b"PlaybackNetworkProbe" not in data, "test-only class present")
        assets = {}
        for asset in ASSET_NAMES:
            entry = "assets/" + asset
            require(entry in names and 0 < archive.getinfo(entry).file_size <= 32 * 1024 * 1024, "APK third-party notice asset missing or oversized")
            assets[asset] = archive.read(entry)
        try:
            return verify_notices(assets, Path(__file__).resolve().parent.parent)
        except (ValueError, KeyError, TypeError, OSError, RecursionError, zipfile.BadZipFile):
            raise ReleaseError("APK third-party notice verification failed") from None


def fingerprint(output: str) -> str:
    values = re.findall(r"^Signer #[0-9]+ certificate SHA-256 digest: ([0-9a-fA-F]{64})\s*$", output, re.MULTILINE)
    require(len(values) == 1 and "Android Debug" not in output, "release must have one non-debug signer")
    return values[0].lower()


def fresh_directory(path: Path) -> None:
    require(not path.exists(), "output directory already exists")
    path.mkdir(parents=True)


def publish_staged(staged: Path, output: Path) -> None:
    # Windows antivirus may briefly hold a freshly signed APK. Keep publication
    # atomic and bounded, rather than copying a partial set into the final path.
    for attempt in range(10):
        try:
            staged.rename(output)
            return
        except PermissionError:
            if os.name != "nt" or attempt == 9:
                raise ReleaseError("signed output publication failed") from None
            time.sleep(0.1)


def prepare(repo: Path, output: Path, sdk_tools: Path, tag: str, commit: str, code: int) -> None:
    name = version(tag)
    require(re.fullmatch(r"[0-9a-f]{40}", commit) is not None and 1 <= code <= 2_100_000_000, "invalid release identity")
    sources = {app: repo / f"apps/android/{app}/build/outputs/apk/release/{app}-release-unsigned.apk" for app in APPS}
    # Validate both before writing any distribution; never guess a signed filename.
    notices = None
    for app, source in sources.items():
        current = verify_apk(source, sdk_tools, app, name, code)
        require(notices is None or notices == current, "APK notice sets differ")
        notices = current
    fresh_directory(output)
    for app, source in sources.items():
        shutil.copyfile(source, output / f"{app}-unsigned.apk")
    with zipfile.ZipFile(sources["receiver"]) as archive:
        for asset in ASSET_NAMES:
            (output / asset).write_bytes(archive.read("assets/" + asset))
    metadata = {"schema": 2, "tag": tag, "commit": commit, "versionCode": code,
                "unsignedSha256": {app: digest(path) for app, path in sources.items()}, "noticesSha256": notices}
    (output / "android-inputs.json").write_text(json.dumps(metadata, indent=2) + "\n", encoding="utf-8")


def validate_inputs(inputs: Path, tag: str, commit: str) -> dict:
    path = inputs / "android-inputs.json"
    require(path.is_file() and not path.is_symlink() and path.stat().st_size <= 8192, "release manifest missing or oversized")
    metadata = json.loads(path.read_text(encoding="utf-8"))
    require(isinstance(metadata, dict) and set(metadata) == {"schema", "tag", "commit", "versionCode", "unsignedSha256", "noticesSha256"} and type(metadata["schema"]) is int and metadata["schema"] == 2, "release manifest schema mismatch")
    require(metadata["tag"] == tag and metadata["commit"] == commit and re.fullmatch(r"[0-9a-f]{40}", commit) is not None, "release manifest identity mismatch")
    require(type(metadata["versionCode"]) is int and 1 <= metadata["versionCode"] <= 2_100_000_000, "invalid Android version code")
    require(isinstance(metadata["unsignedSha256"], dict) and set(metadata["unsignedSha256"]) == set(APPS), "release requires both APKs")
    for app in APPS:
        path = inputs / f"{app}-unsigned.apk"
        expected = metadata["unsignedSha256"][app]
        require(isinstance(expected, str) and HEX.fullmatch(expected) is not None, "invalid APK hash")
        require(path.is_file() and not path.is_symlink() and digest(path) == expected, "unsigned APK hash mismatch")
    require(isinstance(metadata["noticesSha256"], dict) and set(metadata["noticesSha256"]) == set(ASSET_NAMES), "release requires all three notice artifacts")
    for asset in ASSET_NAMES:
        path = inputs / asset
        expected = metadata["noticesSha256"][asset]
        require(isinstance(expected, str) and HEX.fullmatch(expected) is not None, "invalid notice artifact hash")
        require(path.is_file() and not path.is_symlink() and digest(path) == expected, "notice artifact hash mismatch")
    return metadata


def signing_environment() -> tuple[bytes, str, dict[str, str], str]:
    for name in SECRET_NAMES:
        require(bool(os.environ.get(name)) and all(c not in os.environ[name] for c in "\n\r\0"), "signing secret missing or malformed")
    require(len(os.environ["ANDROID_KEY_ALIAS"]) <= 256 and not os.environ["ANDROID_KEY_ALIAS"].startswith("-"), "invalid key alias")
    encoded = os.environ["ANDROID_KEYSTORE_BASE64"]
    require(len(encoded) <= 48 * 1024, "keystore secret exceeds limit")
    try:
        key = base64.b64decode(encoded, validate=True)
    except ValueError as exc:
        raise ReleaseError("invalid keystore encoding") from exc
    require(1 <= len(key) <= 36 * 1024, "keystore size invalid")
    expected = os.environ.get("ANDROID_SIGNING_CERT_SHA256", "").lower()
    require(HEX.fullmatch(expected) is not None, "expected signing fingerprint missing or invalid")
    # Only passwords reach apksigner, not the base64 key, GitHub token, OIDC or alias secret.
    env = public_environment()
    env.update({name: os.environ[name] for name in ("ANDROID_KEYSTORE_PASSWORD", "ANDROID_KEY_PASSWORD")})
    return key, os.environ["ANDROID_KEY_ALIAS"], env, expected


def sign(inputs: Path, output: Path, sdk_tools: Path, tag: str, commit: str) -> None:
    name = version(tag)
    metadata = validate_inputs(inputs, tag, commit)
    for app in APPS:
        require(verify_apk(inputs / f"{app}-unsigned.apk", sdk_tools, app, name, metadata["versionCode"]) == metadata["noticesSha256"], "APK notices differ from release manifest")
    key, alias, env, expected = signing_environment()
    require(not output.exists(), "signed output directory already exists")
    output.parent.mkdir(parents=True, exist_ok=True)
    # Only this newly allocated temporary directory is removed; never a caller's directory.
    with tempfile.TemporaryDirectory(prefix="frameark-sign-", dir=output.parent) as temporary:
        private = Path(temporary).resolve()
        require(private.parent == output.parent.resolve(), "invalid signing temporary directory")
        keystore = private / "release.keystore"
        keystore.write_bytes(key)
        keystore.chmod(0o600)
        staged = private / "verified"
        staged.mkdir()
        for app in APPS:
            aligned = private / f"{app}-aligned.apk"
            target = staged / f"FrameArk-{app.title()}-{tag}-arm64.apk"
            run([tool(sdk_tools, "zipalign"), "-P", "16", "4", inputs / f"{app}-unsigned.apk", aligned], "APK alignment failed")
            run([*signer(sdk_tools), "sign", "--ks", keystore, "--ks-key-alias", alias,
                 "--ks-pass", "env:ANDROID_KEYSTORE_PASSWORD", "--key-pass", "env:ANDROID_KEY_PASSWORD",
                 "--v4-signing-enabled", "false", "--out", target, aligned], "APK signing failed", env)
            verification = run([*signer(sdk_tools), "verify", "--verbose", "--print-certs", "-Werr", target], "APK signature verification failed")
            require(fingerprint(verification) == expected, "APK signing certificate mismatch")
            run([tool(sdk_tools, "zipalign"), "-c", "-P", "16", "4", target], "signed APK alignment failed")
            require(verify_apk(target, sdk_tools, app, name, metadata["versionCode"]) == metadata["noticesSha256"], "signed APK notice set changed")
        for asset in ASSET_NAMES:
            shutil.copyfile(inputs / asset, staged / asset)
        receipt = {"schema": 2, "tag": tag, "commit": commit, "versionCode": metadata["versionCode"],
                   "certificateSha256": expected, "apks": {p.name: digest(p) for p in sorted(staged.glob("*.apk"))},
                   "noticesSha256": metadata["noticesSha256"]}
        (staged / "android-signing.json").write_text(json.dumps(receipt, indent=2) + "\n", encoding="utf-8")
        # No partial APK set is published on any failure.
        publish_staged(staged, output)
    print("Both release-variant APKs signed, pinned-certificate verified and aligned.")


def validate_tag(repo: Path, tag: str) -> dict:
    name = version(tag)
    require(run(["git", "-C", repo, "rev-parse", "--is-shallow-repository"], "version history unavailable").strip() == "false", "release history must be complete")
    ref = f"refs/tags/{tag}"
    require(run(["git", "-C", repo, "cat-file", "-t", ref], "release tag missing").strip() == "tag", "release tag must be annotated or signed")
    commit = run(["git", "-C", repo, "rev-parse", f"{ref}^{{commit}}"], "tag commit missing").strip()
    require(run(["git", "-C", repo, "rev-parse", "HEAD"], "checkout missing").strip() == commit, "checkout differs from release tag")
    run(["git", "-C", repo, "merge-base", "--is-ancestor", commit, "origin/main"], "tag is not on reviewed main")
    require(os.environ.get("GITHUB_REF", ref) == ref, "release must run on the selected tag ref")
    code = int(run(["git", "-C", repo, "rev-list", "--count", "--first-parent", commit], "version history unavailable").strip())
    require(1 <= code <= 2_100_000_000, "Android version code out of range")
    return {"tag": tag, "commit": commit, "version_name": name, "version_code": code,
            "prerelease": str(SEMVER.fullmatch(tag).group(4) is not None).lower(),
            "source_date_epoch": run(["git", "-C", repo, "show", "-s", "--format=%ct", commit], "commit timestamp unavailable").strip()}


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("operation", choices=("tag", "prepare", "sign"))
    parser.add_argument("--repo", type=Path, default=Path("."))
    parser.add_argument("--tag", required=True)
    parser.add_argument("--commit")
    parser.add_argument("--version-code", type=int)
    parser.add_argument("--sdk-tools", type=Path)
    parser.add_argument("--inputs", type=Path)
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()
    try:
        if args.operation == "tag":
            for name, value in validate_tag(args.repo, args.tag).items():
                print(f"{name}={value}")
        else:
            require(args.sdk_tools is not None and args.output is not None and args.commit is not None, "missing release paths/identity")
            if args.operation == "prepare":
                require(args.version_code is not None, "version code missing")
                prepare(args.repo, args.output, args.sdk_tools, args.tag, args.commit, args.version_code)
            else:
                require(args.inputs is not None, "release inputs missing")
                sign(args.inputs, args.output, args.sdk_tools, args.tag, args.commit)
    except ReleaseError as exc:
        print(f"Android release: {exc}", file=sys.stderr)
        return 1
    except (OSError, ValueError, KeyError, TypeError, zipfile.BadZipFile):
        print("Android release: invalid input or filesystem failure", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
