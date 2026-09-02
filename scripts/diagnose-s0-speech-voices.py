#!/usr/bin/env python3
"""Bounded real-Camoufox speech voice diagnostics for S0 Windows acceptance.

Test-only evidence. The script imports the exact packaged shipping Camouhost adapter and uses its
existing materialization/launch path. It does not define another launcher, identity owner,
admission verdict, fallback, or runtime path.
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

VOICE_PROBE = r"""
async (delays) => {
  const normalize = (voice) => ({
    language: String(voice.lang || ''),
    name: String(voice.name || ''),
    voice_uri: String(voice.voiceURI || ''),
    local_service: Boolean(voice.localService),
    is_default: Boolean(voice.default),
  });
  const snapshots = [];
  let voiceChangedEvents = 0;
  const onChanged = () => { voiceChangedEvents += 1; };
  speechSynthesis.addEventListener('voiceschanged', onChanged);
  try {
    const capture = (afterMs) => {
      const voices = Array.from(speechSynthesis.getVoices() || [], normalize);
      snapshots.push({after_ms: afterMs, event_count: voiceChangedEvents, voices});
    };
    capture(0);
    let elapsed = 0;
    for (const delay of delays) {
      await new Promise((resolve) => setTimeout(resolve, delay));
      elapsed += delay;
      capture(elapsed);
    }
  } finally {
    speechSynthesis.removeEventListener('voiceschanged', onChanged);
  }
  return snapshots;
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
    spec = importlib.util.spec_from_file_location("s0_exact_camouhost_voice_diag", path)
    if spec is None or spec.loader is None:
        raise RuntimeError("unable to load exact Camouhost module")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def canonical_line(prefix: str, value: object) -> None:
    encoded = json.dumps(value, ensure_ascii=True, separators=(",", ":"), sort_keys=True)
    print(f"{prefix}={encoded}")


def expected_voices(config: dict[str, Any]) -> list[dict[str, Any]]:
    raw = config.get("voices")
    if not isinstance(raw, list):
        return []
    rows: list[dict[str, Any]] = []
    for voice in raw:
        if not isinstance(voice, dict):
            raise RuntimeError("configured voice row is not an object")
        rows.append(
            {
                "language": voice.get("lang"),
                "name": voice.get("name"),
                "voice_uri": voice.get("voiceUri"),
                "local_service": voice.get("isLocalService"),
                "is_default": voice.get("isDefault"),
            }
        )
    return rows


def sorted_rows(rows: list[dict[str, Any]]) -> list[dict[str, Any]]:
    return sorted(
        rows,
        key=lambda row: (
            str(row.get("language")),
            str(row.get("name")),
            str(row.get("voice_uri")),
            bool(row.get("local_service")),
            bool(row.get("is_default")),
        ),
    )


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

    with tempfile.TemporaryDirectory(prefix="s0-speech-voice-diagnostic-") as temporary:
        root = Path(temporary)
        (root / camouhost.BRIDGE_LOCK_NAME).write_text(
            "profile-platform-bridge-lock-v1\ndiagnostic-device\n1\n",
            encoding="utf-8",
        )
        report = camouhost.materialize_candidate_identity(root)
        config = json.loads((root / camouhost.CONFIG_NAME).read_text(encoding="utf-8"))
        expected = expected_voices(config)
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
            snapshots = page.evaluate(VOICE_PROBE, [250, 500, 750, 1500, 3000, 4000])
        finally:
            if manager is not None and context is not None:
                with contextlib.suppress(BaseException):
                    camouhost.close_context(manager, context, root)

    if not isinstance(snapshots, list):
        raise RuntimeError("speech voice diagnostic snapshots are invalid")
    canonical_line("S0_SPEECH_VOICE_EXPECTED", {"count": len(expected)})
    final_equal = False
    for snapshot in snapshots:
        if not isinstance(snapshot, dict) or not isinstance(snapshot.get("voices"), list):
            raise RuntimeError("speech voice diagnostic snapshot is invalid")
        observed = snapshot["voices"]
        equal = sorted_rows(observed) == sorted_rows(expected)
        canonical_line(
            "S0_SPEECH_VOICE_OBSERVATION",
            {
                "after_ms": snapshot.get("after_ms"),
                "count": len(observed),
                "equal": equal,
                "event_count": snapshot.get("event_count"),
            },
        )
        final_equal = equal
    canonical_line(
        "S0_SPEECH_VOICE_SUMMARY",
        {
            "expected_count": len(expected),
            "final_equal": final_equal,
            "snapshot_count": len(snapshots),
        },
    )
    return 0 if expected and final_equal else 1


if __name__ == "__main__":
    raise SystemExit(main())
