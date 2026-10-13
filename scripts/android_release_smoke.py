#!/usr/bin/env python3
"""Exercise real dual-APK signing with an ephemeral NON-production fixture key."""
import argparse
import base64
import json
import os
import secrets
import tempfile
from pathlib import Path
from unittest.mock import patch

import android_release as release


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo", type=Path, default=Path("."))
    parser.add_argument("--sdk-tools", type=Path, required=True)
    args = parser.parse_args()
    parent = args.repo.resolve() / "build"
    parent.mkdir(exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="frameark-signing-test-", dir=parent) as temporary:
        private = Path(temporary).resolve()
        release.require(private.parent == parent, "invalid fixture directory")
        key = private / "fixture.p12"
        cert = private / "fixture.cer"
        test_env = release.public_environment()
        test_env["FRAMEARK_FIXTURE_PASSWORD"] = secrets.token_hex(24)
        release.run(["keytool", "-genkeypair", "-noprompt", "-keystore", key, "-storetype", "PKCS12", "-alias", "fixture",
                     "-storepass:env", "FRAMEARK_FIXTURE_PASSWORD", "-keypass:env", "FRAMEARK_FIXTURE_PASSWORD",
                     "-keyalg", "RSA", "-keysize", "2048", "-validity", "2", "-dname", "CN=FrameArk CI Fixture"], "fixture key generation failed", test_env)
        release.run(["keytool", "-exportcert", "-keystore", key, "-alias", "fixture", "-storepass:env", "FRAMEARK_FIXTURE_PASSWORD", "-file", cert], "fixture certificate export failed", test_env)
        commit = release.run(["git", "-C", args.repo, "rev-parse", "HEAD"], "fixture commit unavailable").strip()
        inputs = private / "inputs"
        release.prepare(args.repo, inputs, args.sdk_tools, "v0.0.0-ci", commit, 2)
        signing = {"ANDROID_KEYSTORE_BASE64": base64.b64encode(key.read_bytes()).decode(),
                   "ANDROID_KEY_ALIAS": "fixture", "ANDROID_KEYSTORE_PASSWORD": test_env["FRAMEARK_FIXTURE_PASSWORD"],
                   "ANDROID_KEY_PASSWORD": test_env["FRAMEARK_FIXTURE_PASSWORD"], "ANDROID_SIGNING_CERT_SHA256": release.digest(cert)}
        with patch.dict(os.environ, signing):
            output = private / "signed"
            release.sign(inputs, output, args.sdk_tools, "v0.0.0-ci", commit)
            receipt = json.loads((output / "android-signing.json").read_text(encoding="utf-8"))
            release.require(len(receipt["apks"]) == 2 and receipt["certificateSha256"] == release.digest(cert), "fixture receipt invalid")
            for label, change in (("wrong-certificate", {"ANDROID_SIGNING_CERT_SHA256": "0" * 64}),
                                  ("wrong-password", {"ANDROID_KEYSTORE_PASSWORD": "wrong-fixture-password"}),
                                  ("missing-secret", {"ANDROID_KEYSTORE_BASE64": ""})):
                failed = private / label
                with patch.dict(os.environ, change):
                    try:
                        release.sign(inputs, failed, args.sdk_tools, "v0.0.0-ci", commit)
                    except release.ReleaseError:
                        pass
                    else:
                        raise release.ReleaseError("invalid signing configuration accepted")
                release.require(not failed.exists(), "partial signed outputs leaked")
                release.require(not list(private.glob("frameark-sign-*")), "temporary signing key survived cleanup")
        print("Release smoke passed: both real APKs, fixture certificate, wrong fingerprint/password/missing secret rejection, no partial outputs or retained key.")


if __name__ == "__main__":
    main()
