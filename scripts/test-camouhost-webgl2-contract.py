#!/usr/bin/env python3
"""Stdlib-only fail-closed Camouhost WebGL2 observation contract."""
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
        "fonts": ["Arial"],
    }
    if enabled:
        config.update({
            "webGl2:parameters": {"3379": 2},
            "webGl2:shaderPrecisionFormats": {},
        })
    return config


def refused(config: dict) -> None:
    try:
        module.browser_visible_probe_request(config)
    except module.RuntimeContractError:
        return
    raise AssertionError("inconsistent WebGL2 fingerprint should fail closed")


for enabled in (False, True):
    cfg = make_config(enabled)
    request = module.browser_visible_probe_request(cfg)
    assert (request["webgl2"] is not None) == enabled
    assert "context_attributes" not in request["webgl"]
    if enabled:
        assert "context_attributes" not in request["webgl2"]
    assert module.generation_webgl2_enabled(cfg) == enabled

disabled = make_config(False)
for key in ("webGl2:parameters", "webGl2:shaderPrecisionFormats"):
    incompatible = disabled.copy()
    incompatible[key] = {}
    refused(incompatible)

for key in ("webGl2:parameters", "webGl2:shaderPrecisionFormats"):
    incomplete = make_config(True)
    del incomplete[key]
    refused(incomplete)

for value in (None, [], False):
    invalid = make_config(True)
    invalid["webGl2:parameters"] = value
    refused(invalid)

for legacy_key in module.LEGACY_WEBGL_CONTEXT_KEYS:
    legacy = make_config(True)
    legacy[legacy_key] = {"alpha": True}
    assert module.generation_webgl2_enabled(legacy)
    assert "context_attributes" not in module.browser_visible_probe_request(legacy)["webgl"]
    for bad_value in (None, [], False):
        invalid = make_config(True)
        invalid[legacy_key] = bad_value
        refused(invalid)

legacy_disabled = make_config(False)
legacy_disabled["webGl2:contextAttributes"] = {}
refused(legacy_disabled)

for missing in module.WEBGL_REQUIRED_KEYS:
    invalid = make_config(True)
    del invalid[missing]
    refused(invalid)

assert "spec.context_attributes" not in module.BROWSER_VISIBLE_PROBE
assert "getContextAttributes()" in module.BROWSER_VISIBLE_PROBE

stray = make_config(False)
stray["webGl2:supportedExtensions"] = ["spoof"]
refused(stray)

print("Camouhost WebGL observation contract: enabled / disabled / partial / legacy PASS")


# The diagnostic projection must not touch browser page content or change
# runtime outcomes when visibility observations fail. Source uses only integers.
import contextlib
import io
from types import SimpleNamespace

assert module.accessibility_preferences("nt") == {"accessibility.force_disabled": 0}
assert module.accessibility_preferences("posix") == {}


class UnavailablePages:
    @property
    def pages(self):
        raise RuntimeError("https://private.invalid/?token=do-not-log")


capture = io.StringIO()
with contextlib.redirect_stderr(capture):
    module.report_live_page_count(SimpleNamespace(pages=[object(), object(), object()]))
    module.report_live_page_count(SimpleNamespace(pages=[object()] * 513))
    module.report_live_page_count(UnavailablePages())
assert capture.getvalue() == "camouhost_pages count=3\n"
assert "private.invalid" not in capture.getvalue()
