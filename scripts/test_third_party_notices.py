import gzip
import copy
import io
import json
import tarfile
import tempfile
import unittest
import zipfile
from pathlib import Path

import third_party_notices as notices
import verify_third_party_notices as verification
from android_release import ReleaseError

MIT = b"Copyright Fixture Authors\nPermission is hereby granted\nTHE SOFTWARE IS PROVIDED AS IS\n"


def write_json(path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value), encoding="utf-8")


def tar_bytes(members):
    output = io.BytesIO()
    with tarfile.open(fileobj=output, mode="w:gz") as archive:
        for name, value in members:
            entry = tarfile.TarInfo(name)
            entry.size = len(value)
            archive.addfile(entry, io.BytesIO(value))
    return output.getvalue()


class NoticeTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="frameark-notice-test-")
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        (self.root / "rust-toolchain.toml").write_text('[toolchain]\nchannel = "1.97.0"\n', encoding="utf-8")
        self.pkg = self.root / "cargo/registry/src/fixture-index/fixture-1.0.0"
        self.pkg.mkdir(parents=True)
        manifest = b'[package]\nname = "fixture"\nversion = "1.0.0"\nlicense = "MIT"\n'
        (self.pkg / "Cargo.toml").write_bytes(manifest)
        self.archive = self.root / "cargo/registry/cache/fixture-index/fixture-1.0.0.crate"
        self.archive.parent.mkdir(parents=True)
        self.archive.write_bytes(tar_bytes([
            ("fixture-1.0.0/Cargo.toml", manifest), ("fixture-1.0.0/LICENSE-MIT", MIT),
            ("fixture-1.0.0/third party/nested/NOTICE", b"Nested contributor attribution\n"),
            ("fixture-1.0.0/.cargo_vcs_info.json", json.dumps({"git": {"sha1": "a" * 40}}).encode()),
        ]))
        (self.root / "Cargo.lock").write_text('[[package]]\nname = "fixture"\nversion = "1.0.0"\nsource = "' + notices.REGISTRY + '"\nchecksum = "' + notices.digest(self.archive) + '"\n', encoding="utf-8")
        self.metadata = {"packages": [{"name": "fixture", "version": "1.0.0", "source": notices.REGISTRY,
                                       "license": "MIT", "manifest_path": str(self.pkg / "Cargo.toml")}]}
        self.jar = self.root / "fixture.jar"
        self.jar.write_bytes(b"synthetic runtime fixture")
        coordinate = "org.example:fixture:1.0.0"
        self.coordinate = coordinate
        supplement = self.root / "docs/third-party/supplements/fixture/LICENSE"
        supplement.parent.mkdir(parents=True)
        supplement.write_bytes(MIT)
        (supplement.parent / "NOTICE").write_bytes(b"Required fixture attribution\n")
        self.policy = {"schema": 1, "rustLicenseChoices": {"MIT": "MIT"},
                       "rustSupplements": {"fixture@1.0.0": {"vcs": "a" * 40, "files": ["fixture/LICENSE"]}},
                       "androidApplications": {app: [coordinate] for app in ("receiver", "sender")},
                       "android": {coordinate: {"sha256": notices.digest(self.jar), "declaredLicense": "MIT", "noticeScope": "fixture", "files": ["fixture/LICENSE", "fixture/NOTICE"]}},
                       "files": {"fixture/LICENSE": {"sha256": notices.sha(MIT), "source": "https://example.invalid/fixture/LICENSE"},
                                 "fixture/NOTICE": {"sha256": notices.sha(b"Required fixture attribution\n"), "source": "https://example.invalid/fixture/NOTICE"}}}
        write_json(self.root / "docs/third-party/policy.json", self.policy)
        for app in ("receiver", "sender"):
            write_json(self.root / f"apps/android/{app}/build/reports/frameark/release-runtime.json",
                       {"schema": 1, "app": app, "configuration": "releaseRuntimeClasspath",
                        "artifacts": [{"coordinate": coordinate, "file": str(self.jar), "sha256": notices.digest(self.jar)}]})
        self.rust = self.root / "rust"
        docs = self.rust / "share/doc/rust"
        (docs / "licenses").mkdir(parents=True)
        (docs / "COPYRIGHT-library.html").write_text("<h1>Fixture stdlib notice</h1><p>Fixture authors &amp; contributors</p>", encoding="utf-8")
        for name in ("MIT.txt", "Apache-2.0.txt"):
            (docs / "licenses" / name).write_bytes(b"Synthetic runtime license fixture\n")
        self.ndk = self.root / "ndk"
        self.ndk.mkdir()
        (self.ndk / "source.properties").write_text("Pkg.Revision = 28.0.123-beta1\n", encoding="utf-8")
        for name in ("NOTICE", "NOTICE.toolchain"):
            (self.ndk / name).write_bytes(b"Synthetic NDK runtime notice\n")

    def build(self, name="out"):
        output = self.root / name
        notices.generate(self.root, output, self.metadata, self.rust, self.ndk, "1.97.0")
        return {name: (output / name).read_bytes() for name in notices.ASSET_NAMES}

    def test_complete_deterministic_bundle_and_no_local_paths(self):
        first = self.build()
        second = self.build("second")
        self.assertEqual(first, second)
        hashes = verification.verify(first, self.root)
        self.assertEqual(set(notices.ASSET_NAMES), set(hashes))
        index = json.loads(first["third-party-index.json"])
        self.assertEqual(4, len(index["components"]))
        self.assertNotIn(str(self.root), first["third-party-index.json"].decode())
        with zipfile.ZipFile(io.BytesIO(first["third-party-licenses.zip"])) as archive:
            self.assertIn("cargo/fixture-1.0.0/third party/nested/NOTICE", archive.namelist())
            self.assertTrue(all(e.date_time == (1980, 1, 1, 0, 0, 0) for e in archive.infolist()))
        self.assertIn(b"Fixture authors & contributors", first["THIRD_PARTY_NOTICES.txt"])

    def test_missing_lock_dependency_and_changed_archive_fail_before_output(self):
        self.metadata["packages"] = []
        with self.assertRaises(ReleaseError):
            self.build()
        self.assertFalse((self.root / "out").exists())
        data = self.archive.read_bytes()
        with self.assertRaisesRegex(ReleaseError, "checksum"):
            notices.crate_files(self.archive, "fixture-1.0.0", "0" * 64, None)
        self.assertEqual(data, self.archive.read_bytes())

    def test_tar_traversal_duplicate_link_and_expansion_limits(self):
        variants = [tar_bytes([("fixture-1.0.0/../LICENSE", MIT)]),
                    tar_bytes([("fixture-1.0.0/LICENSE", MIT), ("fixture-1.0.0/LICENSE", MIT)])]
        linked = tarfile.TarInfo("fixture-1.0.0/LICENSE")
        linked.type = tarfile.SYMTYPE; linked.linkname = "../../outside"
        variants.append(gzip.compress(linked.tobuf() + b"\0" * 1024))
        huge = tarfile.TarInfo("fixture-1.0.0/LICENSE"); huge.size = 129 * 1024 * 1024
        variants.append(gzip.compress(huge.tobuf() + b"\0" * 1024))
        for data in variants:
            self.archive.write_bytes(data)
            with self.subTest(size=len(data)), self.assertRaises(ReleaseError):
                notices.crate_files(self.archive, "fixture-1.0.0", notices.sha(data), None)

    def test_unknown_license_incomplete_text_and_changed_manifest_fail(self):
        self.metadata["packages"][0]["license"] = "GPL-3.0-only"
        with self.assertRaisesRegex(ReleaseError, "unreviewed Rust license"):
            self.build()
        with self.assertRaisesRegex(ReleaseError, "missing or incomplete"):
            notices.verify_license_text("MIT", [b"Authors only"])
        self.metadata["packages"][0]["license"] = "MIT"
        (self.pkg / "Cargo.toml").write_bytes(b"changed")
        with self.assertRaisesRegex(ReleaseError, "manifest differs"):
            self.build()

    def test_runtime_artifact_supplement_and_graph_changes_fail(self):
        original = self.jar.read_bytes()
        self.jar.write_bytes(b"tampered")
        with self.assertRaisesRegex(ReleaseError, "artifact checksum"):
            self.build()
        self.jar.write_bytes(original)
        file = self.root / "docs/third-party/supplements/fixture/LICENSE"
        file.write_bytes(b"replaced license")
        with self.assertRaisesRegex(ReleaseError, "supplement checksum"):
            self.build()
        file.write_bytes(MIT)
        self.policy["androidApplications"]["sender"] = []
        write_json(self.root / "docs/third-party/policy.json", self.policy)
        with self.assertRaisesRegex(ReleaseError, "runtime graph changed"):
            self.build()

    def test_missing_runtime_notice_and_existing_output_fail_closed(self):
        self.build()
        with self.assertRaisesRegex(ReleaseError, "already exists"):
            self.build()
        (self.ndk / "NOTICE.toolchain").unlink()
        with self.assertRaises(ReleaseError):
            self.build("new")
        self.assertFalse((self.root / "new").exists())

    def test_text_tampering_stale_checkout_and_missing_component_rejected(self):
        assets = self.build()
        with self.assertRaisesRegex(ValueError, "readable text hash"):
            verification.verify({**assets, "THIRD_PARTY_NOTICES.txt": b"tampered"}, self.root)
        index = json.loads(assets["third-party-index.json"])
        index["components"] = [c for c in index["components"] if not c["id"].startswith("cargo:")]
        with self.assertRaisesRegex(ValueError, "coverage mismatch"):
            verification.verify({**assets, "third-party-index.json": json.dumps(index).encode()}, self.root)
        with (self.root / "Cargo.lock").open("a", encoding="utf-8") as file:
            file.write("\n# lock change\n")
        with self.assertRaisesRegex(ValueError, "stale"):
            verification.verify(assets, self.root)

    def test_zip_original_text_tampering_and_missing_entries_rejected(self):
        assets = self.build()
        original = zipfile.ZipFile(io.BytesIO(assets["third-party-licenses.zip"]))
        self.addCleanup(original.close)
        for operation in ("tamper", "omit", "symlink"):
            output = io.BytesIO()
            with zipfile.ZipFile(output, "w") as changed:
                for entry in original.infolist():
                    data = original.read(entry)
                    entry = copy.copy(entry)
                    if entry.filename.endswith("LICENSE-MIT"):
                        if operation == "omit":
                            continue
                        if operation == "tamper":
                            data = b"wrong original legal text"
                        if operation == "symlink":
                            entry.external_attr = 0o120777 << 16
                    changed.writestr(entry, data)
            with self.subTest(operation=operation), self.assertRaises(ValueError):
                verification.verify({**assets, "third-party-licenses.zip": output.getvalue()}, self.root)

    def rewritten_bundle(self, assets, component_id, suffix, operation):
        """Keep all attacker-controlled hashes/text/ZIP consistent with the edit."""
        index = json.loads(assets["third-party-index.json"])
        bundle = notices.Bundle()
        bundle.components = index["components"]
        with zipfile.ZipFile(io.BytesIO(assets["third-party-licenses.zip"])) as archive:
            bundle.files = {name: archive.read(name) for name in archive.namelist() if name != "third-party-index.json"}
        component = next(c for c in bundle.components if c["id"] == component_id)
        record = next(r for r in component["files"] if r["path"].endswith(suffix))
        if operation == "omit":
            component["files"].remove(record)
            del bundle.files[record["path"]]
        elif operation == "replace":
            bundle.files[record["path"]] = b"Self-consistent replacement, not the reviewed license.\n"
            record["sha256"] = notices.sha(bundle.files[record["path"]])
        elif operation == "source":
            record["source"] = "https://example.invalid/unreviewed-source"
        elif operation == "reassign":
            old_path = record["path"]
            record["path"] = "unowned/" + Path(old_path).name
            bundle.files[record["path"]] = bundle.files.pop(old_path)
        elif operation == "license":
            component["declaredLicense"] = "Apache-2.0"
        else:
            raise AssertionError("unknown fixture mutation")
        return notices.serialize(bundle, index["cargoLockSha256"], index["androidRuntime"], index["policySha256"])

    def test_self_consistent_maven_notice_edits_rejected_by_policy(self):
        assets = self.build()
        for operation in ("omit", "replace", "source"):
            changed = self.rewritten_bundle(assets, "maven:" + self.coordinate, "/NOTICE", operation)
            with self.subTest(operation=operation), self.assertRaises(ValueError):
                verification.verify(changed, self.root)

    def test_self_consistent_rust_supplement_edits_rejected_by_policy(self):
        assets = self.build()
        for operation in ("omit", "replace", "source"):
            changed = self.rewritten_bundle(assets, "cargo:fixture:1.0.0", "/supplement-LICENSE", operation)
            with self.subTest(operation=operation), self.assertRaises(ValueError):
                verification.verify(changed, self.root)

    def test_self_consistent_runtime_file_omissions_rejected(self):
        assets = self.build()
        for component, suffix in (("runtime:rust-stdlib:1.97.0", "/COPYRIGHT-library.html"),
                                  ("runtime:android-ndk:28.0.123-beta1", "/NOTICE.toolchain")):
            changed = self.rewritten_bundle(assets, component, suffix, "omit")
            with self.subTest(component=component), self.assertRaises(ValueError):
                verification.verify(changed, self.root)

    def test_component_ownership_and_maven_license_metadata_are_bound(self):
        assets = self.build()
        changed = self.rewritten_bundle(assets, "cargo:fixture:1.0.0", "/LICENSE-MIT", "reassign")
        with self.assertRaisesRegex(ValueError, "different component"):
            verification.verify(changed, self.root)
        changed = self.rewritten_bundle(assets, "maven:" + self.coordinate, "/NOTICE", "license")
        with self.assertRaisesRegex(ValueError, "license metadata"):
            verification.verify(changed, self.root)


if __name__ == "__main__":
    unittest.main()
