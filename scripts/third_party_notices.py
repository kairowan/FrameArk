#!/usr/bin/env python3
"""Build a deterministic, offline notice bundle from locked, resolved dependencies."""
from __future__ import annotations

import argparse
import hashlib
import io
import json
import os
import re
import sys
import tarfile
import tomllib
import zipfile
from html.parser import HTMLParser
from pathlib import Path

from android_release import ReleaseError, digest, public_environment, require, run
from verify_third_party_notices import ASSET_NAMES, verify

IDENTIFIER = re.compile(r"[A-Za-z0-9_.+-]{1,160}")
LEGAL = re.compile(r"(?:licen[cs]e|notice|copying|copyright|authors|unlicense)(?:[._-].*)?", re.I)
REGISTRY = "registry+https://github.com/rust-lang/crates.io-index"
MAX_FILE = 2 * 1024 * 1024
MAX_TOTAL = 32 * 1024 * 1024


def sha(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def read(path: Path, limit: int = MAX_FILE) -> bytes:
    require(path.is_file() and not path.is_symlink() and 0 < path.stat().st_size <= limit, "notice input missing, linked, empty or oversized")
    data = path.read_bytes()
    require(len(data) <= limit, "notice input grew beyond bound")
    return data


def json_file(path: Path, limit: int = MAX_FILE) -> dict:
    value = json.loads(read(path, limit))
    require(isinstance(value, dict), "notice JSON must be an object")
    return value


def safe_relative(value: str) -> str:
    require(isinstance(value, str) and 0 < len(value) <= 512 and "\\" not in value, "invalid notice path")
    require(not any(ord(c) < 32 or ord(c) == 127 or c == ":" for c in value), "invalid notice path character")
    require(all(value.split("/")) and not value.startswith("/"), "empty notice path component")
    require(all(p not in (".", "..") for p in value.split("/")), "notice path traversal")
    return value


def contained(root: Path, relative: str) -> Path:
    path = root / safe_relative(relative)
    require(path.resolve().is_relative_to(root.resolve()), "notice path escapes source root")
    cursor = path
    while cursor != root:
        require(not cursor.is_symlink(), "linked notice path")
        cursor = cursor.parent
    return path


def crate_files(archive_path: Path, prefix: str, checksum: str, license_file: str | None) -> dict[str, bytes]:
    data = read(archive_path, 32 * 1024 * 1024)
    require(sha(data) == checksum, "Cargo archive checksum differs from lockfile")
    result, seen = {}, set()
    expanded = 0
    with tarfile.open(fileobj=io.BytesIO(data), mode="r:gz") as archive:
        for member in archive:
            require(member.name.startswith(prefix + "/"), "unexpected Cargo archive root")
            relative = safe_relative(member.name[len(prefix) + 1:].rstrip("/"))
            require(relative not in seen and len(seen) < 20000, "duplicate or oversized Cargo archive inventory")
            seen.add(relative)
            expanded += member.size
            require(0 <= member.size <= 32 * 1024 * 1024 and expanded <= 128 * 1024 * 1024, "Cargo expanded archive exceeds bound")
            wanted = LEGAL.fullmatch(Path(relative).name) or relative in ("Cargo.toml", ".cargo_vcs_info.json", license_file)
            if not wanted or member.isdir():
                continue
            require(member.isfile() and 0 < member.size <= MAX_FILE, "linked, empty or oversized Cargo legal file")
            stream = archive.extractfile(member)
            require(stream is not None, "Cargo legal file unavailable")
            with stream:
                result[relative] = stream.read(MAX_FILE + 1)
            require(len(result[relative]) == member.size, "Cargo legal file size mismatch")
    return result


def verify_license_text(choice: str, texts: list[bytes]) -> None:
    # Completeness sentinels catch AUTHORS-only / header-only substitutions. They
    # are not a legal interpretation or a substitute for reviewing new licenses.
    joined = " ".join(b.decode("utf-8", errors="strict") for b in texts).lower()
    joined = " ".join(joined.split())
    sentinels = {
        "MIT": ("permission is hereby granted", "the software is provided"),
        "Apache-2.0": ("apache license", "terms and conditions for use, reproduction, and distribution"),
        "ISC": ("permission to use, copy, modify", "the software is provided"),
        "BSD-3-Clause": ("redistribution and use in source and binary forms", "neither the name"),
        "Unicode-3.0": ("unicode", "permission is hereby granted"),
    }
    for term in choice.split(" AND "):
        require(term in sentinels and all(s in joined for s in sentinels[term]), "selected license text is missing or incomplete")


class Bundle:
    def __init__(self) -> None:
        self.files: dict[str, bytes] = {}
        self.components: list[dict] = []
        self.total = 0

    def add(self, name: str, data: bytes) -> dict:
        safe_relative(name)
        require(name not in self.files and 0 < len(data) <= MAX_FILE, "duplicate, empty or oversized notice")
        self.total += len(data)
        require(self.total <= MAX_TOTAL and len(self.files) < 2048, "notice bundle exceeds bound")
        data.decode("utf-8", errors="strict")
        self.files[name] = data
        return {"path": name, "sha256": sha(data)}


def supplement(repo: Path, policy: dict, name: str) -> tuple[bytes, dict]:
    require(name in policy["files"], "unreviewed notice supplement")
    record = policy["files"][name]
    data = read(contained(repo / "docs/third-party/supplements", name))
    require(sha(data) == record["sha256"], "notice supplement checksum mismatch")
    return data, record


def collect_rust(repo: Path, metadata: dict, policy: dict, bundle: Bundle) -> str:
    lock_bytes = read(repo / "Cargo.lock")
    lock = tomllib.loads(lock_bytes.decode("utf-8"))
    locked = {(p["name"], p["version"], p["source"]): p for p in lock["package"] if "source" in p}
    packages = [p for p in metadata["packages"] if p.get("source")]
    require(0 < len(packages) <= 512, "invalid Rust dependency inventory size")
    require({(p["name"], p["version"], p["source"]) for p in packages} == set(locked), "Cargo metadata does not cover the complete lockfile")
    require(len(packages) == len(locked), "duplicate Cargo metadata package")
    for package in sorted(packages, key=lambda p: (p["name"], p["version"])):
        name, version = package["name"], package["version"]
        require(IDENTIFIER.fullmatch(name) is not None and IDENTIFIER.fullmatch(version) is not None, "invalid Cargo package identity")
        require(package["source"] == REGISTRY, "non-registry Rust dependency needs explicit review")
        declared = package.get("license")
        require(declared in policy["rustLicenseChoices"], "unreviewed Rust license expression")
        choice = policy["rustLicenseChoices"][declared]
        root = Path(package["manifest_path"]).parent
        checksum = locked[(name, version, REGISTRY)]["checksum"]
        archive_path = root.parents[2] / "cache" / root.parent.name / f"{name}-{version}.crate"
        license_file = safe_relative(package["license_file"]) if package.get("license_file") else None
        originals = crate_files(archive_path, f"{name}-{version}", checksum, license_file)
        require(read(Path(package["manifest_path"])) == originals.get("Cargo.toml"), "Cargo manifest differs from locked archive")
        paths = [p for p in originals if p not in ("Cargo.toml", ".cargo_vcs_info.json")]
        records, texts = [], []
        for relative in sorted(set(paths)):
            data = originals[relative]
            texts.append(data)
            records.append(bundle.add(f"cargo/{name}-{version}/{relative}", data))
        extra = policy["rustSupplements"].get(f"{name}@{version}")
        if extra:
            vcs_data = originals[".cargo_vcs_info.json"]
            require(json.loads(vcs_data)["git"]["sha1"] == extra["vcs"], "supplement does not match crate source revision")
            for relative in extra["files"]:
                data, provenance = supplement(repo, policy, relative)
                texts.append(data)
                record = bundle.add(f"cargo/{name}-{version}/supplement-{Path(relative).name}", data)
                record["source"] = provenance["source"]
                records.append(record)
        verify_license_text(choice, texts)
        bundle.components.append({"id": f"cargo:{name}:{version}", "declaredLicense": declared,
                                  "selectedLicense": choice, "packageChecksum": checksum,
                                  "source": f"https://crates.io/crates/{name}/{version}", "files": records})
    return sha(lock_bytes)


def collect_android(repo: Path, policy: dict, bundle: Bundle) -> dict:
    apps: dict[str, list[str]] = {}
    components: dict[str, dict] = {}
    for app in ("receiver", "sender"):
        report = json_file(repo / f"apps/android/{app}/build/reports/frameark/release-runtime.json")
        require(report.get("schema") == 1 and report.get("app") == app and report.get("configuration") == "releaseRuntimeClasspath", "Android runtime report identity mismatch")
        artifacts = report.get("artifacts")
        require(isinstance(artifacts, list) and 0 < len(artifacts) <= 128, "empty or oversized Android runtime report")
        app_ids = []
        for artifact in artifacts:
            coordinate = artifact["coordinate"]
            require(coordinate in policy["android"], "Android dependency version needs notice review")
            expected = policy["android"][coordinate]
            require(artifact["sha256"] == expected["sha256"] == digest(Path(artifact["file"])), "Android runtime artifact checksum mismatch")
            require(coordinate not in app_ids, "duplicate Android runtime component")
            app_ids.append(coordinate)
            if coordinate in components:
                continue
            records = []
            for relative in expected["files"]:
                data, provenance = supplement(repo, policy, relative)
                record = bundle.add("maven/" + coordinate.replace(":", "/") + "/" + Path(relative).name, data)
                record["source"] = provenance["source"]
                records.append(record)
            group, name, version = coordinate.split(":")
            components[coordinate] = {"id": "maven:" + coordinate, "declaredLicense": expected["declaredLicense"],
                                      "noticeScope": expected["noticeScope"], "artifactSha256": expected["sha256"],
                                      "source": f"https://repo.maven.apache.org/maven2/{group.replace('.', '/')}/{name}/{version}/",
                                      "files": records}
        apps[app] = sorted(app_ids)
        require(apps[app] == policy["androidApplications"][app], "Android runtime graph changed; update reviewed notice policy")
    bundle.components.extend(components.values())
    return apps


def collect_runtime(rust_root: Path, ndk_root: Path, rust_version: str, bundle: Bundle) -> None:
    require(IDENTIFIER.fullmatch(rust_version) is not None, "invalid Rust toolchain version")
    rust_docs = rust_root / "share/doc/rust"
    records = [bundle.add("runtime/rust/COPYRIGHT-library.html", read(rust_docs / "COPYRIGHT-library.html"))]
    for name in ("Apache-2.0.txt", "MIT.txt"):
        records.append(bundle.add("runtime/rust/" + name, read(rust_docs / "licenses" / name)))
    bundle.components.append({"id": "runtime:rust-stdlib:" + rust_version,
                              "noticeScope": "Upstream standard-library copyright report includes in-tree and bundled dependency terms; not all compiler/toolchain code is shipped.", "files": records})
    properties = read(ndk_root / "source.properties").decode("utf-8")
    match = re.search(r"^Pkg.Revision\s*=\s*([A-Za-z0-9._+-]{1,160})\s*$", properties, re.MULTILINE)
    require(match is not None, "Android NDK version unavailable")
    records = [bundle.add("runtime/android-ndk/" + name, read(ndk_root / name)) for name in ("NOTICE", "NOTICE.toolchain")]
    bundle.components.append({"id": "runtime:android-ndk:" + match.group(1),
                              "noticeScope": "Conservative upstream NDK/runtime and compiler-runtime notice superset; not a claim that every listed NDK component is in the APK.", "files": records})


class PlainHtml(HTMLParser):
    def __init__(self) -> None:
        super().__init__(convert_charrefs=True)
        self.parts: list[str] = []

    def handle_data(self, data: str) -> None:
        self.parts.append(data)

    def handle_endtag(self, tag: str) -> None:
        if tag in ("p", "div", "pre", "li", "h1", "h2", "h3"):
            self.parts.append("\n")


def serialize(bundle: Bundle, lock_hash: str, apps: dict, policy_hash: str) -> dict[str, bytes]:
    components = sorted(bundle.components, key=lambda c: c["id"])
    require(len({c["id"] for c in components}) == len(components), "duplicate notice component")
    document = {"schema": 1, "generator": "FrameArk third_party_notices.py", "cargoLockSha256": lock_hash,
                "policySha256": policy_hash, "androidRuntime": apps, "components": components,
                "scope": "Complete locked Rust dependency superset, resolved Android release runtime, Rust stdlib and NDK runtime notices. Not an exact binary linkage map or a legal/patent clearance."}
    parts = ["FrameArk third-party notices\n\n", document["scope"], "\nOriginal, byte-preserved texts and provenance are in third-party-licenses.zip and third-party-index.json.\n"]
    for component in components:
        parts.append("\n" + "=" * 72 + "\n" + component["id"] + "\n")
        if "selectedLicense" in component:
            parts.append("License option selected: " + component["selectedLicense"] + "\n")
        for record in component["files"]:
            text = bundle.files[record["path"]].decode("utf-8")
            if record["path"].endswith(".html"):
                parser = PlainHtml(); parser.feed(text); parser.close()
                text = "".join(parser.parts)
            parts.extend(("\n--- " + record["path"] + " ---\n", text, "\n"))
    text = "".join(parts).encode("utf-8")
    require(len(text) <= MAX_TOTAL, "combined notice exceeds bound")
    document["noticeTextSha256"] = sha(text)
    index = (json.dumps(document, ensure_ascii=False, sort_keys=True, indent=2) + "\n").encode()
    output = io.BytesIO()
    with zipfile.ZipFile(output, "w", compression=zipfile.ZIP_DEFLATED, compresslevel=9) as archive:
        for name, data in sorted({**bundle.files, "third-party-index.json": index}.items()):
            info = zipfile.ZipInfo(name, (1980, 1, 1, 0, 0, 0))
            info.create_system = 3
            info.external_attr = 0o100644 << 16
            info.compress_type = zipfile.ZIP_DEFLATED
            archive.writestr(info, data, compresslevel=9)
    return {"THIRD_PARTY_NOTICES.txt": text, "third-party-index.json": index, "third-party-licenses.zip": output.getvalue()}


def generate(repo: Path, output: Path, metadata: dict, rust_root: Path, ndk_root: Path, rust_version: str) -> None:
    require(not output.exists(), "notice output already exists; choose a fresh output directory")
    policy_bytes = read(repo / "docs/third-party/policy.json")
    policy = json.loads(policy_bytes)
    require(policy.get("schema") == 1, "unsupported notice policy")
    bundle = Bundle()
    lock_hash = collect_rust(repo, metadata, policy, bundle)
    apps = collect_android(repo, policy, bundle)
    collect_runtime(rust_root, ndk_root, rust_version, bundle)
    artifacts = serialize(bundle, lock_hash, apps, sha(policy_bytes))
    verify(artifacts, repo)
    output.mkdir(parents=True)
    for name, data in artifacts.items():
        (output / name).write_bytes(data)
    print(f"Notice bundle complete: {len(bundle.components)} components, {len(bundle.files)} original legal files; both Android runtimes checked.")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo", type=Path, default=Path("."))
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--cargo-metadata", type=Path)
    parser.add_argument("--ndk-root", type=Path, required=True)
    args = parser.parse_args()
    try:
        env = public_environment()
        env.update({k: os.environ[k] for k in ("HOME", "USERPROFILE", "CARGO_HOME", "RUSTUP_HOME") if k in os.environ})
        metadata = json_file(args.cargo_metadata, 16 * 1024 * 1024) if args.cargo_metadata else json.loads(run(["cargo", "metadata", "--manifest-path", args.repo / "Cargo.toml", "--format-version", "1", "--locked"], "Cargo metadata unavailable", env))
        rust_root = Path(run(["rustc", "--print", "sysroot"], "Rust sysroot unavailable", env).strip())
        rust_version = run(["rustc", "--version"], "Rust version unavailable", env).split()[1]
        generate(args.repo, args.output, metadata, rust_root, args.ndk_root, rust_version)
    except ReleaseError as exc:
        print(f"Third-party notices: {exc}", file=sys.stderr)
        return 1
    except (OSError, ValueError, TypeError, KeyError, IndexError, AttributeError, RecursionError, tarfile.TarError):
        print("Third-party notices: invalid input or filesystem failure", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
