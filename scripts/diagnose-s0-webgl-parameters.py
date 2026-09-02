#!/usr/bin/env python3
"""Bounded real-Camoufox WebGL parameter diagnostics for S0 acceptance.

This is test-only evidence. It imports the exact shipping Camouhost adapter and uses its
existing materialization and launch functions; it does not define another launcher, identity
owner, admission verdict, fallback, or runtime path.
"""

from __future__ import annotations

import argparse
import contextlib
import importlib.util
import json
import os
import tempfile
from pathlib import Path
from types import ModuleType
from typing import Any

DIAGNOSTIC_PROBE = r"""
(request) => {
  const normalize = (value) => {
    if (value === undefined) return null;
    if (value === null || typeof value === 'string' || typeof value === 'number' || typeof value === 'boolean') {
      return value;
    }
    if (Array.isArray(value) || ArrayBuffer.isView(value)) return Array.from(value, normalize);
    if (typeof value === 'object') {
      const output = {};
      for (const [key, nested] of Object.entries(value)) output[key] = normalize(nested);
      return output;
    }
    return null;
  };

  const jsType = (value) => {
    if (value === null) return 'null';
    if (ArrayBuffer.isView(value)) {
      const ctor = value.constructor;
      return ctor && typeof ctor.name === 'string' && ctor.name ? ctor.name : 'TypedArray';
    }
    if (Array.isArray(value)) return 'Array';
    return typeof value;
  };

  const errorFields = (phase, error) => ({
    error_phase: phase,
    error_name: error && error.name ? String(error.name) : 'Error',
    error_message: error && error.message ? String(error.message) : '',
  });

  const inspect = (kind, configured) => {
    const canvas = document.createElement('canvas');
    const gl = canvas.getContext(kind);
    if (!gl) return {available: false, rows: []};
    const rows = [];
    for (const [key, expected] of Object.entries(configured)) {
      const numericKey = Number(key);
      const glEnum = Number.isInteger(numericKey) ? numericKey : gl[key];
      if (!Number.isInteger(glEnum)) {
        rows.push({key, configured: normalize(expected), observed: null, js_type: 'unresolved-enum', equal: false});
        continue;
      }

      let raw;
      try {
        raw = gl.getParameter(glEnum);
      } catch (error) {
        rows.push({
          key,
          configured: normalize(expected),
          observed: null,
          js_type: 'exception',
          equal: false,
          ...errorFields('getParameter', error),
        });
        continue;
      }

      let observed;
      let observedType;
      try {
        observedType = jsType(raw);
        observed = normalize(raw);
      } catch (error) {
        rows.push({
          key,
          configured: normalize(expected),
          observed: null,
          js_type: 'exception',
          equal: false,
          ...errorFields('normalize', error),
        });
        continue;
      }

      rows.push({
        key,
        configured: normalize(expected),
        observed,
        js_type: observedType,
        equal: JSON.stringify(normalize(expected)) === JSON.stringify(observed),
      });
    }
    return {available: true, rows};
  };

  return {
    webgl: inspect('webgl', request.webgl),
    webgl2: inspect('webgl2', request.webgl2),
  };
}
"""


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--camouhost", type=Path, required=True)
    parser.add_argument("--runtime-lock", type=Path, required=True)
    parser.add_argument("--headless", choices=("false", "virtual"), required=True)
    return parser.parse_args()


def load_camouhost(path: Path) -> ModuleType:
    path = path.resolve(strict=True)
    spec = importlib.util.spec_from_file_location("s0_exact_camouhost", path)
    if spec is None or spec.loader is None:
        raise RuntimeError("unable to load exact Camouhost module")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def canonical_line(prefix: str, value: object) -> None:
    encoded = json.dumps(value, ensure_ascii=True, separators=(",", ":"), sort_keys=True)
    print(f"{prefix}={encoded}")


def main() -> int:
    args = parse_args()
    runtime_lock = args.runtime_lock.resolve(strict=True)
    camouhost = load_camouhost(args.camouhost)

    os.environ[camouhost.RUNTIME_LOCK_ENV] = str(runtime_lock)
    os.environ[camouhost.HEADLESS_MODE_ENV] = args.headless
    os.environ.pop(camouhost.EXPECTED_RUNTIME_LOCK_SHA256_ENV, None)
    os.environ.pop(camouhost.EXPECTED_CONFIG_SHA256_ENV, None)
    os.environ.pop(camouhost.EXPECTED_PROBE_SHA256_ENV, None)
    os.environ.pop(camouhost.PROXY_CONFIG_ENV, None)

    with tempfile.TemporaryDirectory(prefix="s0-webgl-diagnostic-") as temporary:
        root = Path(temporary)
        (root / camouhost.BRIDGE_LOCK_NAME).write_text(
            "profile-platform-bridge-lock-v1\ndiagnostic-device\n1\n",
            encoding="utf-8",
        )

        report = camouhost.materialize_candidate_identity(root)
        config_path = root / camouhost.CONFIG_NAME
        config = json.loads(config_path.read_text(encoding="utf-8"))
        os.environ[camouhost.EXPECTED_CONFIG_SHA256_ENV] = report["fingerprint_config_sha256"]

        lock, _ = camouhost.load_runtime_lock()
        manager: Any | None = None
        context: Any | None = None
        try:
            manager, context = camouhost.launch_verified_context(
                lock,
                root,
                config,
                report["profile_stable_probe_sha256"],
            )
            page = context.pages[0] if context.pages else context.new_page()
            result = page.evaluate(
                DIAGNOSTIC_PROBE,
                {
                    "webgl": config["webGl:parameters"],
                    "webgl2": config["webGl2:parameters"],
                },
            )
        finally:
            if manager is not None and context is not None:
                with contextlib.suppress(BaseException):
                    camouhost.close_context(manager, context, root)

    if not isinstance(result, dict):
        raise RuntimeError("diagnostic browser result is invalid")

    total_rows = 0
    typed_array_rows = 0
    mismatch_rows = 0
    emitted_rows = 0
    for context_name in ("webgl", "webgl2"):
        context_result = result.get(context_name)
        if not isinstance(context_result, dict) or not context_result.get("available"):
            canonical_line(
                "S0_WEBGL_PARAMETER_CONTEXT",
                {"context": context_name, "available": False},
            )
            continue
        rows = context_result.get("rows")
        if not isinstance(rows, list):
            raise RuntimeError("diagnostic WebGL rows are invalid")
        for row in rows:
            if not isinstance(row, dict):
                raise RuntimeError("diagnostic WebGL row is invalid")
            total_rows += 1
            js_type = row.get("js_type")
            equal = row.get("equal") is True
            is_typed_array = isinstance(js_type, str) and js_type.endswith("Array") and js_type != "Array"
            if is_typed_array:
                typed_array_rows += 1
            if not equal:
                mismatch_rows += 1
            if is_typed_array or not equal:
                canonical_line(
                    "S0_WEBGL_PARAMETER_OBSERVATION",
                    {"context": context_name, **row},
                )
                emitted_rows += 1

    canonical_line(
        "S0_WEBGL_PARAMETER_SUMMARY",
        {
            "emitted_rows": emitted_rows,
            "mismatch_rows": mismatch_rows,
            "total_rows": total_rows,
            "typed_array_rows": typed_array_rows,
        },
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
