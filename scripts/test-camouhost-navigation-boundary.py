#!/usr/bin/env python3
"""Offline regression: CRM's shipping admission never controls native Firefox tabs.

Run with the repository Python standard library. This intentionally does not
launch or inspect a saved browser profile, download an addon, or need a site.
"""
from __future__ import annotations

import contextlib
import importlib.util
import io
from pathlib import Path

SOURCE = Path(__file__).resolve().parents[1] / "runtime" / "camouhost" / "real.py"


def load_runtime():
    spec = importlib.util.spec_from_file_location("crm_native_navigation_boundary", SOURCE)
    assert spec is not None and spec.loader is not None
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


class AdmissionGate:
    """Only legacy network admission exists; page/navigation APIs are forbidden."""

    def __init__(self):
        self.admission_calls = []

    def set_offline(self, value):
        self.admission_calls.append(value)

    def __getattr__(self, name):
        raise AssertionError(f"shipping admission tried browser page operation {name}")


def main() -> int:
    runtime = load_runtime()
    # The runtime must never select a third-party extension policy for Firefox.
    assert runtime.default_addon_exclusions() == []

    gate = AdmissionGate()
    with contextlib.redirect_stderr(io.StringIO()):
        runtime.admit_navigation(gate, None)
    assert gate.admission_calls == [False], "legacy network gate was not released exactly once"

    rejected = AdmissionGate()
    try:
        runtime.admit_navigation(rejected, "https://example.com/")
    except runtime.RuntimeContractError:
        pass
    else:
        raise AssertionError("CRM-directed navigation was accepted")
    assert rejected.admission_calls == [], "failed admission changed the browser"

    print("CAMOUFOX_NATIVE_NAVIGATION_BOUNDARY=PASS")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
