#!/usr/bin/env python3
"""Generate a deterministic SPDX 2.3 SBOM for the FrameArk Rust workspace."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import subprocess
from datetime import datetime, timezone
from pathlib import Path
from urllib.parse import quote


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo", type=Path, default=Path("."))
    parser.add_argument("--tag", default="unreleased")
    parser.add_argument("--cargo-metadata", type=Path)
    parser.add_argument("--output", type=Path, required=True)
    return parser.parse_args()


def read_metadata(repo: Path, path: Path | None) -> dict:
    if path is not None:
        return json.loads(path.read_text(encoding="utf-8"))
    result = subprocess.run(
        ["cargo", "metadata", "--format-version", "1", "--locked"],
        cwd=repo,
        check=True,
        capture_output=True,
        text=True,
    )
    return json.loads(result.stdout)


def package_id(name: str, version: str, source: str | None = None) -> str:
    digest = hashlib.sha256(
        f"{name}@{version}@{source or 'workspace'}".encode("utf-8")
    ).hexdigest()[:20]
    return f"SPDXRef-Package-{digest}"


def package_url(name: str, version: str, source: str | None) -> str:
    if source and "crates.io-index" in source:
        return f"pkg:cargo/{quote(name)}@{quote(version)}"
    return "NOASSERTION"


def build_document(metadata: dict, tag: str) -> dict:
    packages: dict[str, dict] = {}
    root_id = package_id("FrameArk", tag)
    packages[root_id] = {
        "SPDXID": root_id,
        "name": "FrameArk",
        "versionInfo": tag,
        "downloadLocation": "NOASSERTION",
        "licenseConcluded": "NOASSERTION",
        "licenseDeclared": "Apache-2.0 OR MIT",
        "filesAnalyzed": False,
        "copyrightText": "NOASSERTION",
    }
    id_by_package_id: dict[str, str] = {}
    for package in metadata.get("packages", []):
        spdx_id = package_id(
            package["name"], package["version"], package.get("source")
        )
        id_by_package_id[package["id"]] = spdx_id
        packages[spdx_id] = {
            "SPDXID": spdx_id,
            "name": package["name"],
            "versionInfo": package["version"],
            "downloadLocation": package_url(
                package["name"], package["version"], package.get("source")
            ),
            "licenseConcluded": "NOASSERTION",
            "licenseDeclared": package.get("license") or "NOASSERTION",
            "filesAnalyzed": False,
            "copyrightText": "NOASSERTION",
        }

    relationships = [
        {
            "spdxElementId": "SPDXRef-DOCUMENT",
            "relationshipType": "DESCRIBES",
            "relatedSpdxElement": root_id,
        }
    ]
    for node in metadata.get("resolve", {}).get("nodes", []):
        source_id = id_by_package_id.get(node["id"])
        if source_id is None:
            continue
        for dependency in node.get("dependencies", []):
            target_id = id_by_package_id.get(dependency)
            if target_id is not None and target_id != source_id:
                relationships.append(
                    {
                        "spdxElementId": source_id,
                        "relationshipType": "DEPENDS_ON",
                        "relatedSpdxElement": target_id,
                    }
                )
    relationships = sorted(
        {
            (item["spdxElementId"], item["relationshipType"], item["relatedSpdxElement"]): item
            for item in relationships
        }.values(),
        key=lambda item: (
            item["spdxElementId"],
            item["relationshipType"],
            item["relatedSpdxElement"],
        ),
    )
    epoch = int(os.environ.get("SOURCE_DATE_EPOCH", "0"))
    created = datetime.fromtimestamp(epoch, timezone.utc).isoformat().replace("+00:00", "Z")
    return {
        "spdxVersion": "SPDX-2.3",
        "dataLicense": "CC0-1.0",
        "SPDXID": "SPDXRef-DOCUMENT",
        "name": f"FrameArk {tag}",
        "documentNamespace": f"https://frameark.dev/spdx/{quote(tag, safe='.-_')}",
        "creationInfo": {
            "created": created,
            "creators": ["Tool: FrameArk generate_sbom.py"],
        },
        "packages": [packages[key] for key in sorted(packages)],
        "relationships": relationships,
    }


def main() -> int:
    args = parse_args()
    metadata = read_metadata(args.repo.resolve(), args.cargo_metadata)
    document = build_document(metadata, args.tag)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(
        json.dumps(document, ensure_ascii=False, indent=2, sort_keys=False) + "\n",
        encoding="utf-8",
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
