#!/usr/bin/env python3
"""Verify complete embedded notice sets against the checked-out dependency policy."""
import hashlib
import io
import json
import re
import tomllib
import zipfile
from pathlib import PurePosixPath, Path

ASSET_NAMES = ("THIRD_PARTY_NOTICES.txt", "third-party-index.json", "third-party-licenses.zip")
MAX_TOTAL = 32 * 1024 * 1024


def require(condition: bool, message: str) -> None:
    if not condition:
        raise ValueError(message)


def sha(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def verify(assets: dict[str, bytes], repo: Path) -> dict[str, str]:
    require(set(assets) == set(ASSET_NAMES), "three notice artifacts are required")
    require(all(0 < len(v) <= MAX_TOTAL for v in assets.values()), "notice asset size invalid")
    index_data = assets["third-party-index.json"]
    require(len(index_data) <= 2 * 1024 * 1024, "notice index oversized")
    index = json.loads(index_data)
    require(isinstance(index, dict) and type(index.get("schema")) is int and index["schema"] == 1 and index.get("generator") == "FrameArk third_party_notices.py", "notice index schema mismatch")
    lock_bytes = (repo / "Cargo.lock").read_bytes()
    policy_bytes = (repo / "docs/third-party/policy.json").read_bytes()
    require(index.get("cargoLockSha256") == sha(lock_bytes) and index.get("policySha256") == sha(policy_bytes), "notice index is stale for this checkout")
    require(index.get("noticeTextSha256") == sha(assets["THIRD_PARTY_NOTICES.txt"]), "notice readable text hash mismatch")
    policy = json.loads(policy_bytes)
    require(index.get("androidRuntime") == policy["androidApplications"], "notice Android app inventory mismatch")
    locked = {f"cargo:{p['name']}:{p['version']}": p for p in tomllib.loads(lock_bytes.decode())["package"] if "source" in p}
    expected_maven = {"maven:" + c for values in policy["androidApplications"].values() for c in values}
    components = index.get("components")
    require(isinstance(components, list) and 0 < len(components) <= 1024, "invalid notice components")
    require(all(isinstance(c, dict) and isinstance(c.get("id"), str) for c in components), "malformed notice component")
    ids = [c["id"] for c in components]
    require(len(set(ids)) == len(ids), "duplicate notice component")
    runtimes = [c for c in ids if c.startswith("runtime:")]
    require(len(runtimes) == 2 and sum(c.startswith("runtime:rust-stdlib:") for c in runtimes) == 1 and sum(c.startswith("runtime:android-ndk:") for c in runtimes) == 1, "runtime notices missing")
    rust_version = tomllib.loads((repo / "rust-toolchain.toml").read_text(encoding="utf-8"))["toolchain"]["channel"]
    require("runtime:rust-stdlib:" + rust_version in runtimes, "notice Rust toolchain version mismatch")
    require(set(ids) == set(locked) | expected_maven | set(runtimes), "notice dependency coverage mismatch")
    expected_files = {}
    for component in components:
        component_id = component["id"]
        pinned_files = {}
        required_paths = None
        if component_id in locked:
            package = locked[component_id]
            prefix = f"cargo/{package['name']}-{package['version']}/"
            require(component.get("packageChecksum") == package["checksum"], "notice Cargo checksum mismatch")
            require(component.get("declaredLicense") in policy["rustLicenseChoices"] and component.get("selectedLicense") == policy["rustLicenseChoices"][component["declaredLicense"]], "unreviewed notice license choice")
            extra = policy["rustSupplements"].get(f"{package['name']}@{package['version']}", {})
            for relative in extra.get("files", []):
                pinned_files[prefix + "supplement-" + PurePosixPath(relative).name] = policy["files"][relative]
        elif component_id.startswith("maven:"):
            coordinate = component_id[6:]
            expected = policy["android"][coordinate]
            prefix = "maven/" + coordinate.replace(":", "/") + "/"
            require(component.get("artifactSha256") == expected["sha256"], "notice Android artifact hash mismatch")
            require(component.get("declaredLicense") == expected["declaredLicense"] and component.get("noticeScope") == expected["noticeScope"], "notice Android license metadata mismatch")
            for relative in expected["files"]:
                pinned_files[prefix + PurePosixPath(relative).name] = policy["files"][relative]
            required_paths = set(pinned_files)
        elif component_id.startswith("runtime:rust-stdlib:"):
            prefix = "runtime/rust/"
            required_paths = {prefix + name for name in ("COPYRIGHT-library.html", "Apache-2.0.txt", "MIT.txt")}
        else:
            prefix = "runtime/android-ndk/"
            required_paths = {prefix + name for name in ("NOTICE", "NOTICE.toolchain")}
        files = component.get("files")
        require(isinstance(files, list) and 0 < len(files) <= 512, "component has no original legal files")
        component_files = {}
        for record in files:
            require(isinstance(record, dict), "invalid notice file record")
            name, checksum = record.get("path"), record.get("sha256")
            require(isinstance(name, str) and 0 < len(name) <= 512 and "\\" not in name and ":" not in name, "invalid notice archive path")
            parts = name.split("/")
            require(not PurePosixPath(name).is_absolute() and all(p not in ("", ".", "..") for p in parts) and not any(ord(c) < 32 or ord(c) == 127 for c in name), "notice archive traversal")
            require(name.startswith(prefix), "notice file belongs to a different component")
            require(name not in expected_files and isinstance(checksum, str) and re.fullmatch(r"[0-9a-f]{64}", checksum) is not None, "duplicate or invalid notice hash")
            expected_files[name] = checksum
            component_files[name] = record
        if required_paths is not None:
            require(set(component_files) == required_paths, "required notice file inventory mismatch")
        for name, expected in pinned_files.items():
            record = component_files.get(name)
            require(record is not None and record["sha256"] == expected["sha256"] and record.get("source") == expected["source"], "notice supplement differs from reviewed policy")
    require(0 < len(expected_files) <= 2048, "notice file inventory exceeds bound")
    with zipfile.ZipFile(io.BytesIO(assets["third-party-licenses.zip"])) as archive:
        entries = archive.infolist()
        require(len(entries) == len(expected_files) + 1 and len({e.filename for e in entries}) == len(entries), "duplicate or missing notice ZIP entry")
        require({e.filename for e in entries} == set(expected_files) | {"third-party-index.json"}, "unexpected notice ZIP member")
        require(sum(e.file_size for e in entries) <= MAX_TOTAL, "expanded notice ZIP exceeds bound")
        for entry in entries:
            require(0 < entry.file_size <= 2 * 1024 * 1024 and ((entry.external_attr >> 16) & 0o170000) in (0, 0o100000), "invalid notice ZIP member type or size")
            data = archive.read(entry)
            require(len(data) == entry.file_size, "notice ZIP member size mismatch")
            if entry.filename == "third-party-index.json":
                require(data == index_data, "notice ZIP index differs from APK index")
            else:
                require(sha(data) == expected_files[entry.filename], "original legal text hash mismatch")
    return {name: sha(assets[name]) for name in ASSET_NAMES}
