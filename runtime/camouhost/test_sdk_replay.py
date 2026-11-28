"""No-browser safety tests for the SDK's derived, per-host launch replay."""

import json
import os
import sys
import tempfile
import types
import unittest
from pathlib import Path
from unittest.mock import Mock, patch

import real


class SdkReplayTests(unittest.TestCase):
    def setUp(self):
        self.folder = tempfile.TemporaryDirectory()
        self.addCleanup(self.folder.cleanup)
        self.root = Path(self.folder.name)
        self.config = {
            "navigator.userAgent": "SDK generated Windows Firefox",
            "webGl:vendor": "Recorded GPU",
            "webGl:parameters": {},
            "webGl:shaderPrecisionFormats": {},
        }
        self.fpgen = {"screen": {"width": 1366, "height": 768}}
        self.config_blob = json.dumps(self.config, separators=(",", ":"))
        self.options = {
            "args": ["-new-tab", real.MANAGED_BOOTSTRAP_TAB_URL],
            "ignore_default_args": ["about:blank"],
            "headless": False,
            "firefox_user_prefs": {"webgl.force-enabled": True},
            "env": {
                "CAMOU_CONFIG_1": self.config_blob,
                "CAMOU_PREFS_1": '{"webgl.force-enabled":true}',
                "UNRELATED_PROCESS_SECRET": "do-not-persist",
            },
            "executable_path": "C:/runtime/browser/camoufox.exe",
            "user_data_dir": "C:/test/profile/user_data",
            "proxy": {"password": "do-not-persist"},
        }
        self.runtime_sha = "1" * 64
        self.binding = patch.object(real, "sdk_replay_binding", return_value="a" * 64)
        self.binary = patch.object(real, "packaged_windows_browser", return_value=Path("C:/runtime/browser/camoufox.exe"))
        self.lock = patch.object(
            real, "load_runtime_lock",
            return_value=({"fingerprint_policy_version": "1"}, self.runtime_sha),
        )
        self.proxy = patch.object(real, "resolve_proxy", return_value=None)
        for p in (self.binding, self.binary, self.lock, self.proxy):
            p.start()
            self.addCleanup(p.stop)

    def saved_cache(self):
        replay = real.sdk_replay_snapshot(
            self.options, self.fpgen,
            real.sha256_bytes(real.canonical_json(self.config)), self.runtime_sha,
        )
        (self.root / real.SDK_REPLAY_NAME).write_text(replay, encoding="utf-8")
        return replay

    def test_sdk_output_replayed_with_exact_fp_and_without_persisting_secrets(self):
        replay = self.saved_cache()
        self.assertNotIn("do-not-persist", replay)
        self.assertNotIn("UNRELATED_PROCESS_SECRET", replay)
        self.assertNotIn("C:/runtime", replay)
        self.assertNotIn("C:/test/profile", replay)
        self.assertEqual(json.loads(replay)["fingerprint"], self.fpgen)
        with patch.dict(os.environ, {"OTHER_PROCESS_VAR": "inherited"}):
            options = real.native_sdk_replay_options({}, self.root, self.config)
        self.assertEqual(options["firefox_user_prefs"], self.options["firefox_user_prefs"])
        self.assertEqual(options["env"]["CAMOU_CONFIG_1"], self.config_blob)
        self.assertEqual(options["env"]["OTHER_PROCESS_VAR"], "inherited")
        self.assertEqual(options["user_data_dir"], str(self.root / real.USER_DATA_NAME))

    def test_mismatched_config_and_untrusted_env_fail_closed(self):
        self.saved_cache()
        with self.assertRaises(real.RuntimeContractError):
            real.native_sdk_replay_options({}, self.root, {"webGl:vendor": "not the identity"})
        path = self.root / real.SDK_REPLAY_NAME
        replay = json.loads(path.read_text(encoding="utf-8"))
        replay["options"]["sdk_env"]["SECRET_TOKEN"] = "would-be-exfiltration"
        path.write_bytes(real.canonical_json(replay))
        with self.assertRaises(real.RuntimeContractError):
            real.native_sdk_replay_options({}, self.root, self.config)

    def test_changed_host_uses_official_sdk_configuration_path(self):
        self.saved_cache()
        with patch.object(real, "sdk_replay_binding", return_value="b" * 64), patch.object(
            real, "camoufox_launch_options", return_value={"sdk": "official"}
        ) as launch:
            self.assertEqual(real.native_sdk_replay_options({}, self.root, self.config), {"sdk": "official"})
            launch.assert_called_once_with({}, self.root, self.config)

    def test_first_open_reuses_the_exact_options_that_the_sdk_generated(self):
        fp_module = types.ModuleType("camoufox.fingerprints")
        fp_module.generate_fingerprint = Mock(return_value=self.fpgen)
        utils_module = types.ModuleType("camoufox.utils")
        utils_module.get_screen_cons = Mock(return_value=None)
        utils_module.launch_options = Mock(return_value=self.options.copy())
        camoufox_module = types.ModuleType("camoufox")
        with patch.dict(sys.modules, {
            "camoufox": camoufox_module,
            "camoufox.fingerprints": fp_module,
            "camoufox.utils": utils_module,
        }), patch.object(real, "verify_python_components"), patch.object(
            real, "camoufox_browser_selector", return_value={"executable_path": "camoufox.exe"}
        ), patch.object(real, "locked_firefox_major", return_value=159):
            report, options = real.materialize_candidate_identity(self.root)
        self.assertEqual(utils_module.launch_options.call_count, 1)
        self.assertEqual(options["args"], ["-new-tab", real.MANAGED_BOOTSTRAP_TAB_URL])
        self.assertEqual(options["ignore_default_args"], ["about:blank"])
        self.assertEqual(json.loads(report["sdk_replay_json"])["fingerprint"], self.fpgen)
        self.assertEqual(json.loads(report["camoufox_config_json"]), self.config)


if __name__ == "__main__":
    unittest.main()
