import base64
import json
import os
import tempfile
import unittest
import zipfile
from pathlib import Path
from unittest.mock import patch

import android_release as release
from release_checks import gates_pass


class ReleasePolicyTests(unittest.TestCase):
    def test_output_publication_does_not_copy_or_replace_on_failure(self):
        with patch.object(Path, "rename", side_effect=PermissionError), patch.object(release.time, "sleep"):
            with self.assertRaisesRegex(release.ReleaseError, "signed output publication failed"):
                release.publish_staged(Path("fixture-staged"), Path("fixture-final"))

    def test_semver_rejects_refs_and_invalid_identifiers(self):
        for value in ("v0.1.0", "v12.3.4-alpha.1+build-7"):
            self.assertEqual(value[1:], release.version(value))
        for value in ("main", "../v1.0.0", "v01.0.0", "v1.0.0.foo", "v1.0.0-01", "v1.0.0-a..b", "v1.0.0\n", "v1.0.0;echo token", "v1.0.0/escape"):
            with self.subTest(value=value), self.assertRaises(release.ReleaseError):
                release.version(value)

    def test_only_latest_real_github_gates_count(self):
        def check(name, number, result="success", app="github-actions"):
            return {"name": name, "id": number, "status": "completed", "conclusion": result, "app": {"slug": app}}
        checks = [check("ci-gate", 1), check("security-gate", 2)]
        self.assertTrue(gates_pass({"total_count": 2, "check_runs": checks}))
        self.assertFalse(gates_pass({"total_count": 3, "check_runs": checks}))
        for extra in (check("ci-gate", 3, "failure"), check("ci-gate", 3, "cancelled")):
            self.assertFalse(gates_pass({"total_count": 3, "check_runs": checks + [extra]}))
        self.assertFalse(gates_pass({"total_count": 2, "check_runs": [check("ci-gate", 1, app="impostor"), checks[1]]}))
        for malformed in (None, [], {"total_count": 0, "check_runs": None}, {"total_count": 1, "check_runs": [None]}, {"total_count": 1, "check_runs": [{"id": 1, "app": None}]}):
            self.assertFalse(gates_pass(malformed))

    def test_fingerprint_requires_single_non_debug_signer(self):
        line = "Signer #1 certificate SHA-256 digest: " + "A" * 64
        self.assertEqual("a" * 64, release.fingerprint(line))
        for value in ("", line + "\n" + line.replace("#1", "#2"), "Signer #1 certificate DN: CN=Android Debug\n" + line):
            with self.assertRaises(release.ReleaseError):
                release.fingerprint(value)

    def test_secrets_fail_closed_and_child_environment_is_minimal(self):
        fixture = {name: "fixture-only" for name in release.SECRET_NAMES}
        fixture.update(ANDROID_KEYSTORE_BASE64=base64.b64encode(b"fixture").decode(), ANDROID_SIGNING_CERT_SHA256="a" * 64, GH_TOKEN="do-not-inherit", ACTIONS_ID_TOKEN_REQUEST_TOKEN="do-not-inherit")
        with patch.dict(os.environ, fixture, clear=True):
            key, alias, env, fingerprint = release.signing_environment()
            self.assertEqual(b"fixture", key)
            self.assertEqual("fixture-only", alias)
            self.assertEqual("a" * 64, fingerprint)
            self.assertEqual({"ANDROID_KEYSTORE_PASSWORD", "ANDROID_KEY_PASSWORD"}, set(env))
            self.assertEqual({}, release.public_environment())
            for name in (*release.SECRET_NAMES, "ANDROID_SIGNING_CERT_SHA256"):
                with patch.dict(os.environ, {name: ""}), self.assertRaises(release.ReleaseError):
                    release.signing_environment()
            for value in ("invalid%%base64", "YWJj\n"):
                with patch.dict(os.environ, {"ANDROID_KEYSTORE_BASE64": value}), self.assertRaises(release.ReleaseError):
                    release.signing_environment()
            # Windows itself rejects environment values over 32767 chars; inject a mapping
            # to exercise our GitHub-secret bound without depending on the host limit.
            with patch.object(release.os, "environ", {**fixture, "ANDROID_KEYSTORE_BASE64": "a" * 50000}), self.assertRaises(release.ReleaseError):
                release.signing_environment()

    def test_subprocess_failure_does_not_log_secret_output(self):
        import subprocess
        with patch.object(release.subprocess, "run", return_value=subprocess.CompletedProcess([], 1, "PRIVATE", "PRIVATE")):
            with self.assertRaisesRegex(release.ReleaseError, "^static signing stage$"):
                release.run(["tool"], "static signing stage")

    def test_inputs_require_both_hashes_identity_and_integrity(self):
        with tempfile.TemporaryDirectory(prefix="frameark-input-test-") as temp:
            root = Path(temp)
            for app in release.APPS:
                (root / f"{app}-unsigned.apk").write_bytes(app.encode())
            for asset in release.ASSET_NAMES:
                (root / asset).write_bytes(b"notice fixture")
            data = {"schema": 2, "tag": "v0.1.0-alpha.1", "commit": "a" * 40, "versionCode": 7,
                    "noticesSha256": {asset: release.digest(root / asset) for asset in release.ASSET_NAMES},
                    "unsignedSha256": {app: release.digest(root / f"{app}-unsigned.apk") for app in release.APPS}}
            manifest = root / "android-inputs.json"
            manifest.write_text(json.dumps(data), encoding="utf-8")
            self.assertEqual(data, release.validate_inputs(root, data["tag"], data["commit"]))
            with self.assertRaises(release.ReleaseError):
                release.validate_inputs(root, data["tag"], "b" * 40)
            (root / "sender-unsigned.apk").write_bytes(b"tampered")
            with self.assertRaises(release.ReleaseError):
                release.validate_inputs(root, data["tag"], data["commit"])
            for malformed in (None, [], {**data, "schema": True}, {**data, "schema": 1}, {**data, "unsignedSha256": ["receiver", "sender"]}):
                manifest.write_text(json.dumps(malformed), encoding="utf-8")
                with self.assertRaises(release.ReleaseError):
                    release.validate_inputs(root, data["tag"], data["commit"])

    def test_notice_sidecars_are_required_and_hash_bound(self):
        with tempfile.TemporaryDirectory(prefix="frameark-sidecar-test-") as temp:
            root = Path(temp)
            for name in (*release.ASSET_NAMES, "receiver-unsigned.apk", "sender-unsigned.apk"):
                (root / name).write_bytes(b"fixture")
            data = {"schema": 2, "tag": "v0.1.0-alpha.1", "commit": "a" * 40, "versionCode": 7,
                    "unsignedSha256": {app: release.digest(root / f"{app}-unsigned.apk") for app in release.APPS},
                    "noticesSha256": {name: release.digest(root / name) for name in release.ASSET_NAMES}}
            (root / "android-inputs.json").write_text(json.dumps(data), encoding="utf-8")
            self.assertEqual(data, release.validate_inputs(root, data["tag"], data["commit"]))
            (root / release.ASSET_NAMES[0]).write_bytes(b"altered")
            with self.assertRaisesRegex(release.ReleaseError, "notice artifact hash"):
                release.validate_inputs(root, data["tag"], data["commit"])

    def test_apk_rejects_wrong_identity_debug_abi_and_probe(self):
        with tempfile.TemporaryDirectory(prefix="frameark-apk-test-") as temp:
            root = Path(temp)
            apk = root / "test.apk"
            elf = bytearray(64); elf[:6] = b"\x7fELF\x02\x01"; elf[18:20] = b"\xb7\x00"
            badge = "package: name='dev.frameark.sender' versionCode='2' versionName='0.1.0'"
            def write_jar(abi="arm64-v8a", dex=b"normal", library=elf):
                with zipfile.ZipFile(apk, "w") as archive:
                    archive.writestr(f"lib/{abi}/libframeark_ffi.so", library)
                    archive.writestr("classes.dex", dex)
                    for asset in release.ASSET_NAMES:
                        archive.writestr("assets/" + asset, b"notice fixture")
            with patch.object(release, "tool", return_value=root / "aapt"), patch.object(release, "run", return_value=badge), patch.object(release, "verify_notices", return_value={}):
                write_jar()
                release.verify_apk(apk, root, "sender", "0.1.0", 2)
                with self.assertRaises(release.ReleaseError):
                    release.verify_apk(apk, root, "receiver", "0.1.0", 2)
                for kwargs in ({"abi": "x86_64"}, {"dex": b"CaptureFixtureActivity"}, {"library": elf + b"PlaybackNetworkProbe"}):
                    write_jar(**kwargs)
                    with self.assertRaises(release.ReleaseError):
                        release.verify_apk(apk, root, "sender", "0.1.0", 2)
            write_jar()
            with patch.object(release, "tool", return_value=root / "aapt"), patch.object(release, "run", return_value=badge + "\napplication-debuggable"), self.assertRaises(release.ReleaseError):
                release.verify_apk(apk, root, "sender", "0.1.0", 2)

    def test_tag_must_be_annotated_checked_out_and_on_main(self):
        with tempfile.TemporaryDirectory(prefix="frameark-tag-test-") as temp:
            repo = Path(temp)
            def git(*args):
                return release.run(["git", "-C", repo, *args], "git fixture failed")
            git("init", "--initial-branch=fixture")
            git("config", "user.name", "FrameArk Test")
            git("config", "user.email", "fixture@example.invalid")
            (repo / "fixture.txt").write_text("fixture", encoding="utf-8")
            git("add", "fixture.txt"); git("commit", "-m", "test: isolated fixture")
            git("update-ref", "refs/remotes/origin/main", "HEAD")
            git("tag", "-a", "v0.1.0-alpha.1", "-m", "isolated fixture")
            with patch.dict(os.environ, {"GITHUB_REF": "refs/tags/v0.1.0-alpha.1"}):
                result = release.validate_tag(repo, "v0.1.0-alpha.1")
                self.assertEqual(1, result["version_code"])
                self.assertEqual("true", result["prerelease"])
            shallow = repo / "shallow"
            git("clone", "--depth=1", repo.as_uri(), str(shallow))
            with self.assertRaisesRegex(release.ReleaseError, "history must be complete"):
                release.validate_tag(shallow, "v0.1.0-alpha.1")
            with patch.dict(os.environ, {"GITHUB_REF": "refs/heads/main"}), self.assertRaises(release.ReleaseError):
                release.validate_tag(repo, "v0.1.0-alpha.1")
            git("tag", "v0.1.0")
            with self.assertRaises(release.ReleaseError):
                release.validate_tag(repo, "v0.1.0")
            git("commit", "--allow-empty", "-m", "test: unreviewed fixture")
            git("tag", "-a", "v0.2.0", "-m", "not on main")
            with patch.dict(os.environ, {"GITHUB_REF": "refs/tags/v0.2.0"}), self.assertRaises(release.ReleaseError):
                release.validate_tag(repo, "v0.2.0")


if __name__ == "__main__":
    unittest.main()
