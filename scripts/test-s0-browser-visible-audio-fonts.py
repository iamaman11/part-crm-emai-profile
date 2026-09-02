#!/usr/bin/env python3
"""Empirical S0 proof for real audio and font-output stability across portable restore.

The canonical browser identity remains owned by browser-execution-domain. This acceptance
never treats `audio:seed` or `fonts:spacing_seed` hashes as browser-output proof. It launches
the exact pinned Camoufox runtime, completes the required pre-navigation observation/admission
protocol, observes deterministic OfflineAudioContext samples plus browser-visible canvas/DOM
font metrics, and requires byte-identical output across a cold relaunch and an exact restored
generation workspace rebound to a fresh Bridge writer lock.

The hashes emitted by this script are acceptance evidence only. They are not a runtime identity
registry, a shipping admission owner, or a substitute for generation-owned Profile-Stable inputs.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import math
import os
import queue
import shutil
import subprocess
import tempfile
import threading
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from typing import Any

MAX_EVIDENCE_BYTES = 256 * 1024
IPC_VERSION = "3"
SESSION = "session_01JAS0AUDIOFONTPORTABLE"
BRIDGE_LOCK = "profile-platform-bridge-lock-v1\ndevice_01JAS0AUDIOFONTPORTABLE\n1\n"
LOCK_MARKERS = (".parentlock", "parent.lock", "lock")

PAGE = b"""<!doctype html><meta charset='utf-8'><body><script>
(async () => {
  const text = 'CAP-EXEC S0 portability mmmmmmmmmmwwwwwwww 0123456789';
  const families = ['Arial', 'Times New Roman', 'Courier New', 'Verdana', 'Georgia', 'Tahoma'];
  const canvas = document.createElement('canvas');
  canvas.width = 1024;
  canvas.height = 128;
  const ctx = canvas.getContext('2d');
  if (!ctx) throw new Error('2d canvas unavailable for font metrics');

  const finiteOrNull = (value) => Number.isFinite(value) ? value : null;
  const fonts = families.map((family) => {
    ctx.font = `32px "${family}"`;
    const metrics = ctx.measureText(text);
    const span = document.createElement('span');
    span.textContent = text;
    span.style.position = 'absolute';
    span.style.left = '-100000px';
    span.style.top = '0';
    span.style.visibility = 'hidden';
    span.style.whiteSpace = 'nowrap';
    span.style.fontFamily = `"${family}"`;
    span.style.fontSize = '32px';
    span.style.fontKerning = 'normal';
    span.style.letterSpacing = 'normal';
    document.body.appendChild(span);
    const rect = span.getBoundingClientRect();
    span.remove();
    return {
      family,
      available: document.fonts.check(`32px "${family}"`, text),
      canvas_width: finiteOrNull(metrics.width),
      actual_left: finiteOrNull(metrics.actualBoundingBoxLeft),
      actual_right: finiteOrNull(metrics.actualBoundingBoxRight),
      actual_ascent: finiteOrNull(metrics.actualBoundingBoxAscent),
      actual_descent: finiteOrNull(metrics.actualBoundingBoxDescent),
      dom_width: finiteOrNull(rect.width),
      dom_height: finiteOrNull(rect.height),
    };
  });

  const Offline = window.OfflineAudioContext || window.webkitOfflineAudioContext;
  if (!Offline) throw new Error('OfflineAudioContext unavailable');
  const frameCount = 4096;
  const sampleRate = 44100;
  const audioContext = new Offline(1, frameCount, sampleRate);
  const oscillator = audioContext.createOscillator();
  oscillator.type = 'triangle';
  oscillator.frequency.value = 1000;
  const compressor = audioContext.createDynamicsCompressor();
  compressor.threshold.value = -50;
  compressor.knee.value = 40;
  compressor.ratio.value = 12;
  compressor.attack.value = 0;
  compressor.release.value = 0.25;
  oscillator.connect(compressor);
  compressor.connect(audioContext.destination);
  oscillator.start(0);
  oscillator.stop(frameCount / sampleRate);
  const rendered = await audioContext.startRendering();
  const channel = rendered.getChannelData(0);
  const indices = [0, 1, 2, 3, 5, 7, 11, 17, 31, 63, 127, 255, 511, 1023, 2047, 3071, 4095];
  const audio = {
    channels: rendered.numberOfChannels,
    length: rendered.length,
    sample_rate: rendered.sampleRate,
    samples: indices.map((index) => [index, finiteOrNull(channel[index])]),
  };

  const response = await fetch('/evidence', {
    method: 'POST',
    headers: {'Content-Type': 'application/json'},
    body: JSON.stringify({audio, fonts}),
  });
  if (!response.ok) throw new Error(`empirical evidence rejected: ${response.status}`);
})();
</script><title>S0 audio and font portability proof</title></body>"""


class EvidenceServer(ThreadingHTTPServer):
    def __init__(self) -> None:
        self.observations: queue.Queue[tuple[int, str, str, str]] = queue.Queue()
        super().__init__(("127.0.0.1", 0), Handler)


class Handler(BaseHTTPRequestHandler):
    server: EvidenceServer

    def log_message(self, _format: str, *_args: object) -> None:
        return

    def do_GET(self) -> None:  # noqa: N802 - BaseHTTPRequestHandler contract
        self.send_response(200)
        self.send_header("Content-Type", "text/html; charset=utf-8")
        self.send_header("Content-Length", str(len(PAGE)))
        self.end_headers()
        self.wfile.write(PAGE)

    def do_POST(self) -> None:  # noqa: N802 - BaseHTTPRequestHandler contract
        if self.path != "/evidence":
            self.send_response(404)
            self.end_headers()
            return
        raw_length = self.headers.get("Content-Length")
        try:
            length = int(raw_length or "")
        except ValueError:
            length = -1
        if length <= 0 or length > MAX_EVIDENCE_BYTES:
            self.send_response(413)
            self.end_headers()
            return
        payload = self.rfile.read(length)
        if len(payload) != length:
            self.send_response(400)
            self.end_headers()
            return
        try:
            evidence = json.loads(payload.decode("utf-8"))
            audio = evidence["audio"]
            fonts = evidence["fonts"]
            validate_audio(audio)
            validate_fonts(fonts)
            canonical = canonical_json(evidence)
            audio_canonical = canonical_json(audio)
            fonts_canonical = canonical_json(fonts)
        except (KeyError, TypeError, ValueError, UnicodeDecodeError, json.JSONDecodeError):
            self.send_response(400)
            self.end_headers()
            return
        self.server.observations.put(
            (
                len(canonical),
                hashlib.sha256(canonical).hexdigest(),
                hashlib.sha256(audio_canonical).hexdigest(),
                hashlib.sha256(fonts_canonical).hexdigest(),
            )
        )
        self.send_response(204)
        self.end_headers()


def canonical_json(value: Any) -> bytes:
    return (
        json.dumps(value, ensure_ascii=True, sort_keys=True, separators=(",", ":")) + "\n"
    ).encode("utf-8")


def finite_number(value: Any) -> bool:
    return (
        not isinstance(value, bool)
        and isinstance(value, (int, float))
        and math.isfinite(float(value))
    )


def validate_audio(audio: Any) -> None:
    if not isinstance(audio, dict) or set(audio) != {
        "channels",
        "length",
        "sample_rate",
        "samples",
    }:
        raise ValueError("audio evidence shape is invalid")
    if audio["channels"] != 1 or audio["length"] != 4096 or audio["sample_rate"] != 44100:
        raise ValueError("audio render contract drifted")
    samples = audio["samples"]
    if not isinstance(samples, list) or len(samples) != 17:
        raise ValueError("audio sample evidence is invalid")
    values: list[float] = []
    previous = -1
    for row in samples:
        if (
            not isinstance(row, list)
            or len(row) != 2
            or not isinstance(row[0], int)
            or isinstance(row[0], bool)
            or row[0] <= previous
            or row[0] >= 4096
            or not finite_number(row[1])
        ):
            raise ValueError("audio sample row is invalid")
        previous = row[0]
        values.append(float(row[1]))
    if not any(abs(value) > 1e-7 for value in values):
        raise ValueError("audio output is trivially silent")


def validate_fonts(fonts: Any) -> None:
    expected = ["Arial", "Times New Roman", "Courier New", "Verdana", "Georgia", "Tahoma"]
    if not isinstance(fonts, list) or len(fonts) != len(expected):
        raise ValueError("font metric evidence shape is invalid")
    widths: list[float] = []
    for row, family in zip(fonts, expected, strict=True):
        if not isinstance(row, dict) or set(row) != {
            "actual_ascent",
            "actual_descent",
            "actual_left",
            "actual_right",
            "available",
            "canvas_width",
            "dom_height",
            "dom_width",
            "family",
        }:
            raise ValueError("font metric row shape is invalid")
        if row["family"] != family or not isinstance(row["available"], bool):
            raise ValueError("font metric family identity drifted")
        for key in (
            "actual_ascent",
            "actual_descent",
            "actual_left",
            "actual_right",
            "canvas_width",
            "dom_height",
            "dom_width",
        ):
            if row[key] is None or not finite_number(row[key]):
                raise ValueError(f"font metric {key} is unavailable")
        if float(row["canvas_width"]) <= 0 or float(row["dom_width"]) <= 0:
            raise ValueError("font width evidence is non-positive")
        widths.append(float(row["canvas_width"]))
    if len({round(value, 6) for value in widths}) < 2:
        raise ValueError("font metric surface collapsed to one trivial width")


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--python", type=Path, required=True)
    parser.add_argument("--camouhost", type=Path, required=True)
    parser.add_argument("--runtime-lock", type=Path, required=True)
    parser.add_argument("--headless", choices=("false", "virtual"), required=True)
    return parser.parse_args()


def require_regular(path: Path, label: str) -> Path:
    resolved = path.resolve(strict=True)
    if path.is_symlink() or not resolved.is_file() or resolved.stat().st_size == 0:
        raise AssertionError(f"{label} is not a non-empty regular file")
    return resolved


def write_bridge_lock(root: Path) -> None:
    (root / ".profile-platform.lock").write_text(BRIDGE_LOCK, encoding="utf-8", newline="\n")


def runtime_env(runtime_lock: Path, headless: str) -> dict[str, str]:
    env = os.environ.copy()
    env.update(
        {
            "CAMOUHOST_RUNTIME_LOCK": str(runtime_lock),
            "CAMOUHOST_HEADLESS_MODE": headless,
            "PYTHONUNBUFFERED": "1",
        }
    )
    return env


def materialize(
    python: Path,
    camouhost: Path,
    runtime_lock: Path,
    headless: str,
    root: Path,
) -> dict[str, str]:
    root.mkdir()
    write_bridge_lock(root)
    completed = subprocess.run(
        [str(python), str(camouhost), "--materialize-identity", str(root)],
        env=runtime_env(runtime_lock, headless),
        text=True,
        capture_output=True,
        timeout=240,
        check=False,
    )
    if completed.returncode != 0:
        raise AssertionError(
            "candidate identity materialization failed: "
            f"rc={completed.returncode} err={completed.stderr[-3000:]!r}"
        )
    report = json.loads(completed.stdout)
    required = {
        "fingerprint_config_sha256",
        "fingerprint_policy_version",
        "profile_stable_probe_sha256",
        "runtime_lock_sha256",
    }
    if set(report) != required or any(
        not isinstance(value, str) or not value for value in report.values()
    ):
        raise AssertionError(f"unexpected materialization report: {report}")
    return report


def portable_restore(source: Path, target: Path) -> None:
    if target.exists() or target.is_symlink():
        raise AssertionError("portable restore target already exists")
    shutil.copytree(
        source,
        target,
        ignore=shutil.ignore_patterns(".profile-platform.lock"),
        symlinks=True,
    )
    for marker in LOCK_MARKERS:
        path = target / "user_data" / marker
        if path.exists() or path.is_symlink():
            path.unlink()
    for path in target.rglob("*"):
        if path.is_symlink():
            raise AssertionError(f"portable generation contains symlink: {path.relative_to(target)}")
    write_bridge_lock(target)
    source_config = (source / "camoufox-config.json").read_bytes()
    target_config = (target / "camoufox-config.json").read_bytes()
    if source_config != target_config:
        raise AssertionError("portable restore changed canonical Camoufox config bytes")


def exchange(process: subprocess.Popen[str], frame: str) -> str:
    assert process.stdin is not None
    assert process.stdout is not None
    process.stdin.write(frame + "\n")
    process.stdin.flush()
    response = process.stdout.readline().rstrip("\n")
    if not response:
        stderr = process.stderr.read()[-3000:] if process.stderr is not None else ""
        raise AssertionError(f"Camouhost produced no response for {frame!r}: {stderr}")
    return response


def complete_pre_navigation_protocol(process: subprocess.Popen[str], url: str) -> None:
    response = exchange(process, f"observe_browser_visible|{SESSION}")
    prefix = f"browser_visible|{SESSION}|"
    if not response.startswith(prefix):
        raise AssertionError(f"unexpected browser-visible frame: {response[:160]!r}")
    try:
        payload = bytes.fromhex(response[len(prefix) :])
        observation = json.loads(payload.decode("utf-8"))
    except (ValueError, UnicodeDecodeError, json.JSONDecodeError) as error:
        raise AssertionError("browser-visible wire payload is invalid") from error
    if not isinstance(observation, dict) or not observation:
        raise AssertionError("browser-visible wire payload has invalid top-level shape")

    encoded_target = url.encode("utf-8").hex()
    admission = exchange(process, f"admit_navigation|{SESSION}|{encoded_target}")
    if admission != f"navigation_admitted|{SESSION}":
        raise AssertionError(f"navigation admission failed: {admission}")


def observe_empirical_outputs(
    python: Path,
    camouhost: Path,
    runtime_lock: Path,
    headless: str,
    root: Path,
    report: dict[str, str],
    url: str,
    server: EvidenceServer,
) -> tuple[int, str, str, str]:
    env = runtime_env(runtime_lock, headless)
    env.update(
        {
            "CAMOUHOST_PROFILE_ROOT": str(root.resolve(strict=True)),
            "CAMOUHOST_EXPECTED_RUNTIME_LOCK_SHA256": report["runtime_lock_sha256"],
            "CAMOUHOST_EXPECTED_CONFIG_SHA256": report["fingerprint_config_sha256"],
            "CAMOUHOST_EXPECTED_PROBE_SHA256": report["profile_stable_probe_sha256"],
        }
    )
    process = subprocess.Popen(
        [str(python), str(camouhost)],
        env=env,
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
        bufsize=1,
    )
    try:
        if exchange(process, f"hello|{IPC_VERSION}") != f"hello_ack|{IPC_VERSION}":
            raise AssertionError("Camouhost IPC negotiation failed")
        launch = exchange(process, f"launch|{SESSION}")
        if launch != f"ready|{SESSION}":
            raise AssertionError(f"Camouhost launch failed: {launch}")
        complete_pre_navigation_protocol(process, url)
        evidence = server.observations.get(timeout=30)
        close = exchange(process, f"close|{SESSION}")
        if close != f"closed|{SESSION}|true":
            raise AssertionError(f"Camouhost clean close failed: {close}")
        if process.wait(timeout=60) != 0:
            stderr = process.stderr.read()[-3000:] if process.stderr is not None else ""
            raise AssertionError(f"Camouhost exited non-zero: {stderr}")
        return evidence
    finally:
        if process.poll() is None:
            process.kill()
            process.wait(timeout=30)


def main() -> int:
    args = parse_args()
    python = require_regular(args.python, "runtime Python")
    camouhost = require_regular(args.camouhost, "Camouhost entrypoint")
    runtime_lock = require_regular(args.runtime_lock, "runtime lock")

    server = EvidenceServer()
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    url = f"http://127.0.0.1:{server.server_address[1]}/"
    try:
        with tempfile.TemporaryDirectory(prefix="s0-audio-font-portability-") as temporary:
            base = Path(temporary)
            host_a = base / "host-a-generation"
            host_b = base / "host-b-generation"
            report = materialize(python, camouhost, runtime_lock, args.headless, host_a)
            portable_restore(host_a, host_b)

            first = observe_empirical_outputs(
                python, camouhost, runtime_lock, args.headless, host_a, report, url, server
            )
            second = observe_empirical_outputs(
                python, camouhost, runtime_lock, args.headless, host_a, report, url, server
            )
            restored = observe_empirical_outputs(
                python, camouhost, runtime_lock, args.headless, host_b, report, url, server
            )
            if first != second:
                raise AssertionError(
                    "audio/font browser outputs drifted across cold relaunch: "
                    f"{first} != {second}"
                )
            if first != restored:
                raise AssertionError(
                    "audio/font browser outputs drifted after exact portable restore: "
                    f"{first} != {restored}"
                )
            print(
                json.dumps(
                    {
                        "audio_output_sha256": first[2],
                        "cold_relaunch_equal": True,
                        "empirical_payload_bytes": first[0],
                        "empirical_payload_sha256": first[1],
                        "font_metrics_sha256": first[3],
                        "portable_restore_equal": True,
                    },
                    sort_keys=True,
                    separators=(",", ":"),
                )
            )
    finally:
        server.shutdown()
        server.server_close()
        thread.join(timeout=5)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
