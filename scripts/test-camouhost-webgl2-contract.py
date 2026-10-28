#!/usr/bin/env python3
"""Stdlib-only fail-closed Camouhost WebGL2 generation/upgrade contract."""
from __future__ import annotations

import importlib.util
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location("s0_webgl_contract", ROOT / "runtime/camouhost/real.py")
assert spec and spec.loader
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)


def make_config(enabled: bool) -> dict:
    config = {
        "webGl:parameters": {"3379": 1},
        "webGl:shaderPrecisionFormats": {},
        "webGl:contextAttributes": {},
        "fonts": ["Arial"],
    }
    if enabled:
        config.update({
            "webGl2:parameters": {"3379": 2},
            "webGl2:shaderPrecisionFormats": {},
            "webGl2:contextAttributes": {},
        })
    return config


def refused(config: dict, source_enabled: bool | None = None) -> None:
    try:
        if source_enabled is None:
            module.browser_visible_probe_request(config)
        else:
            module.verify_generated_webgl2_choice(
                config, {"firefox_user_prefs": {"webgl.enable-webgl2": source_enabled}}
            )
    except module.RuntimeContractError:
        return
    raise AssertionError("inconsistent WebGL2 fingerprint should fail closed")


for enabled in (False, True):
    cfg = make_config(enabled)
    request = module.browser_visible_probe_request(cfg)
    assert (request["webgl2"] is not None) == enabled
    assert module.generation_webgl2_enabled(cfg) == enabled
    module.verify_generated_webgl2_choice(
        cfg, {"firefox_user_prefs": {"webgl.enable-webgl2": enabled}}
    )
    refused(cfg, not enabled)

disabled = make_config(False)
for key in ("webGl2:parameters", "webGl2:shaderPrecisionFormats", "webGl2:contextAttributes"):
    incompatible = disabled.copy()
    incompatible[key] = {}
    refused(incompatible)

for key in ("webGl2:parameters", "webGl2:shaderPrecisionFormats", "webGl2:contextAttributes"):
    incomplete = make_config(True)
    del incomplete[key]
    refused(incomplete)

for value in (None, [], False):
    invalid = make_config(True)
    invalid["webGl2:parameters"] = value
    refused(invalid)

stray = make_config(False)
stray["webGl2:supportedExtensions"] = ["spoof"]
refused(stray)

print("Camouhost exact GPU capability contract: enabled / disabled / partial / mismatched PASS")
