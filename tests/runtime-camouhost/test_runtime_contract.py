from __future__ import annotations

import hashlib
import importlib.util
import json
import sys
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
MODULE_PATH = ROOT / "runtime" / "camouhost" / "real.py"
spec = importlib.util.spec_from_file_location("ar10_camouhost_real", MODULE_PATH)
assert spec is not None and spec.loader is not None
real = importlib.util.module_from_spec(spec)
sys.modules[spec.name] = real
spec.loader.exec_module(real)


class RuntimeContractTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.root = Path(self.temp.name)
        self.generation = self.root / "generation"
        self.generation.mkdir()
        (self.generation / "user_data").mkdir()
        self.runtime_manifest = self.root / "runtime.json"
        self.manifest = json.loads(
            (ROOT / "runtime" / "camouhost" / "runtime-candidate.json").read_text()
        )
        self.manifest["launch_eligible"] = True
        self.manifest["browser"]["artifact_sha256"] = "2" * 64
        self.runtime_manifest.write_text(json.dumps(self.manifest), encoding="utf-8")
        self.config = {
            "navigator.userAgent": "ua",
            "navigator.platform": "Win32",
            "navigator.language": "ru-RU",
            "navigator.languages": ["ru-RU", "ru"],
            "navigator.hardwareConcurrency": 8,
            "navigator.deviceMemory": 8,
            "screen.width": 1920,
            "screen.height": 1080,
        }
        canonical = real.canonical_json_bytes(self.config)
        (self.generation / "camoufox-fingerprint.json").write_bytes(canonical)
        identity = {
            "schema": "profile-platform-browser-identity-v1",
            "fingerprint_policy_version": 1,
            "fingerprint_config_sha256": hashlib.sha256(canonical).hexdigest(),
            "fingerprint_config_size": len(canonical),
            "fingerprint_config_keys": len(self.config),
        }
        (self.generation / "browser-identity.json").write_text(
            json.dumps(identity), encoding="utf-8"
        )
        self.env = {
            real.RUNTIME_MANIFEST_ENV: str(self.runtime_manifest),
            real.GENERATION_ROOT_ENV: str(self.generation),
        }
        self.versions = lambda name: self.manifest["packages"][name]["version"]
        self.browser_calls = 0

    def tearDown(self):
        self.temp.cleanup()

    def verify_browser(self, manifest):
        self.browser_calls += 1
        self.assertEqual(manifest["browser"]["version"], "152.0.4-beta.28")

    def prepare(self):
        return real.prepare_runtime(
            self.env,
            version_getter=self.versions,
            python_version_getter=lambda: "3.12.10",
            verify_browser=self.verify_browser,
        )

    def test_canonical_digest_is_order_independent(self):
        left = {"b": 2, "a": 1}
        right = {"a": 1, "b": 2}
        self.assertEqual(real.canonical_sha256(left), real.canonical_sha256(right))

    def test_candidate_manifest_fails_closed_until_artifact_digest_is_resolved(self):
        candidate_path = ROOT / "runtime" / "camouhost" / "runtime-candidate.json"
        with self.assertRaisesRegex(real.ContractError, "not launch eligible"):
            real._runtime_manifest(candidate_path)

    def test_valid_materialization_reaches_browser_inventory_gate(self):
        prepared = self.prepare()
        self.assertEqual(
            prepared.fingerprint_sha256, real.canonical_sha256(self.config)
        )
        self.assertEqual(self.browser_calls, 1)

    def test_missing_identity_fails_before_browser_inventory(self):
        (self.generation / "browser-identity.json").unlink()
        with self.assertRaises(OSError):
            self.prepare()
        self.assertEqual(self.browser_calls, 0)

    def test_partial_identity_shape_fails_before_browser_inventory(self):
        identity_path = self.generation / "browser-identity.json"
        identity = json.loads(identity_path.read_text())
        identity["fingerprint_config_keys"] += 1
        identity_path.write_text(json.dumps(identity), encoding="utf-8")
        with self.assertRaisesRegex(real.ContractError, "shape mismatch"):
            self.prepare()
        self.assertEqual(self.browser_calls, 0)

    def test_config_digest_mismatch_fails_before_browser_inventory(self):
        config_path = self.generation / "camoufox-fingerprint.json"
        config_path.write_text(
            json.dumps({**self.config, "screen.width": 1366}), encoding="utf-8"
        )
        with self.assertRaisesRegex(real.ContractError, "digest mismatch|size mismatch"):
            self.prepare()
        self.assertEqual(self.browser_calls, 0)

    def test_package_version_mismatch_fails_before_browser_inventory(self):
        def wrong(name):
            return "1.61.0" if name == "playwright" else self.versions(name)

        with self.assertRaisesRegex(real.ContractError, "version mismatch"):
            real.prepare_runtime(
                self.env,
                version_getter=wrong,
                python_version_getter=lambda: "3.12.10",
                verify_browser=self.verify_browser,
            )
        self.assertEqual(self.browser_calls, 0)

    def test_python_version_mismatch_fails_before_identity_and_browser(self):
        with self.assertRaisesRegex(real.ContractError, "python runtime version mismatch"):
            real.prepare_runtime(
                self.env,
                version_getter=self.versions,
                python_version_getter=lambda: "3.12.13",
                verify_browser=self.verify_browser,
            )
        self.assertEqual(self.browser_calls, 0)

    def test_runtime_manifest_rejects_floating_browser_digest(self):
        manifest = json.loads(self.runtime_manifest.read_text())
        manifest["browser"]["artifact_sha256"] = "latest"
        self.runtime_manifest.write_text(json.dumps(manifest), encoding="utf-8")
        with self.assertRaisesRegex(real.ContractError, "sha256"):
            self.prepare()
        self.assertEqual(self.browser_calls, 0)

    def test_user_data_directory_is_generation_scoped(self):
        prepared = self.prepare()
        self.assertEqual(
            prepared.user_data_dir, self.generation.resolve() / "user_data"
        )
        self.assertTrue(
            str(prepared.user_data_dir).startswith(str(self.generation.resolve()))
        )


if __name__ == "__main__":
    unittest.main()
