#!/usr/bin/env python3
"""Supported real Camouhost adapter for generation-scoped Camoufox execution.

Stdout is reserved for the versioned Bridge IPC protocol. Runtime/library diagnostics
are redirected to stderr and never include profile payload, fingerprint config, proxy
credentials, or entropy material.
"""

from __future__ import annotations

import asyncio
import contextlib
import copy
import hashlib
import importlib.metadata
import json
import os
import re
import secrets
import sys
import time
import traceback
from pathlib import Path
from typing import Any
from urllib.parse import urlsplit

IPC_VERSION = "3"
MAX_FRAME_LENGTH = 8 * 1024
MAX_CONFIG_BYTES = 1024 * 1024
MAX_BROWSER_VISIBLE_BYTES = 512 * 1024
MAX_URL_BYTES = 2048
BROWSER_CLOSE_QUIESCENCE_SECONDS = 10.0
BROWSER_CLOSE_POLL_SECONDS = 0.05
SESSION_PATTERN = re.compile(r"[A-Za-z0-9_-]{8,96}\Z")
CONFIG_NAME = "camoufox-config.json"
USER_DATA_NAME = "user_data"
BRIDGE_LOCK_NAME = ".profile-platform.lock"
RUNTIME_LOCK_NAME = "runtime-lock.json"

PROFILE_ROOT_ENV = "CAMOUHOST_PROFILE_ROOT"
RUNTIME_LOCK_ENV = "CAMOUHOST_RUNTIME_LOCK"
EXPECTED_RUNTIME_LOCK_SHA256_ENV = "CAMOUHOST_EXPECTED_RUNTIME_LOCK_SHA256"
EXPECTED_CONFIG_SHA256_ENV = "CAMOUHOST_EXPECTED_CONFIG_SHA256"
EXPECTED_PROBE_SHA256_ENV = "CAMOUHOST_EXPECTED_PROBE_SHA256"
HEADLESS_MODE_ENV = "CAMOUHOST_HEADLESS_MODE"
EXECUTION_MODE_ENV = "CAMOUHOST_EXECUTION_MODE"
USER_NATIVE_MODE = "user-native"
AUTOMATION_PLAYWRIGHT_MODE = "automation-playwright"
ISOLATED_COLD_PROBE_MODE = "isolated-cold-probe"
PROXY_CONFIG_ENV = "CAMOUHOST_PROXY_CONFIG_PATH"
AGENT_DIRECTORY_ENV = "CAMOUHOST_AGENT_DIRECTORY"
# Distinguishable only app-owned Firefox bootstrap, never a generic user about:blank tab.
MANAGED_BOOTSTRAP_TAB_URL = "about:blank#camouhost-playwright-bootstrap-v1"
AGENT_REQUEST_PATTERN = re.compile(r"[0-9a-f]{32}\.py\Z")

FINGERPRINT_PROBE = """
() => ({
  userAgent: navigator.userAgent,
  platform: navigator.platform,
  language: navigator.language,
  languages: Array.from(navigator.languages || []),
  hardwareConcurrency: navigator.hardwareConcurrency,
  deviceMemory: navigator.deviceMemory,
  maxTouchPoints: navigator.maxTouchPoints,
  screen: {
    width: screen.width,
    height: screen.height,
    availWidth: screen.availWidth,
    availHeight: screen.availHeight,
    availLeft: screen.availLeft,
    availTop: screen.availTop,
    colorDepth: screen.colorDepth,
    pixelDepth: screen.pixelDepth,
    devicePixelRatio: window.devicePixelRatio,
  },
  webgl: (() => {
    const canvas = document.createElement('canvas');
    const gl = canvas.getContext('webgl');
    if (!gl) return null;
    const debug = gl.getExtension('WEBGL_debug_renderer_info');
    return {
      vendor: debug ? gl.getParameter(debug.UNMASKED_VENDOR_WEBGL) : gl.getParameter(gl.VENDOR),
      renderer: debug ? gl.getParameter(debug.UNMASKED_RENDERER_WEBGL) : gl.getParameter(gl.RENDERER),
    };
  })(),
})
"""

BROWSER_VISIBLE_PROBE = r"""
async (request) => {
  const collectGraphicsInPageRealm = (graphicsRequest) => {
    const requestId = '__camouhost_browser_visible_graphics_request__';
    const resultId = '__camouhost_browser_visible_graphics_result__';
    for (const id of [requestId, resultId]) {
      const prior = document.getElementById(id);
      if (prior) prior.remove();
    }

    const requestNode = document.createElement('script');
    requestNode.id = requestId;
    requestNode.type = 'application/json';
    requestNode.textContent = JSON.stringify(graphicsRequest);

    const resultNode = document.createElement('script');
    resultNode.id = resultId;
    resultNode.type = 'application/json';
    resultNode.textContent = '';

    const runner = document.createElement('script');
    runner.textContent = `
      (() => {
        const requestNode = document.getElementById('${requestId}');
        const resultNode = document.getElementById('${resultId}');
        const write = (value) => { resultNode.textContent = JSON.stringify(value); };
        try {
          const request = JSON.parse(requestNode.textContent);
          const normalize = (value) => {
            if (value === undefined) return null;
            if (value === null || typeof value === 'string' || typeof value === 'number' || typeof value === 'boolean') {
              return value;
            }
            if (Array.isArray(value) || ArrayBuffer.isView(value)) {
              return Array.from(value, normalize);
            }
            if (typeof value === 'object') {
              const output = {};
              for (const [key, nested] of Object.entries(value)) output[key] = normalize(nested);
              return output;
            }
            return null;
          };

          const collectContext = (kind, spec) => {
            const canvas = document.createElement('canvas');
            const gl = canvas.getContext(kind);
            if (!gl) return null;
            if (!spec) throw new Error('WebGL2 context exists without generated fingerprint data');
            const parameters = {};
            for (const key of spec.parameters) {
              const numericKey = Number(key);
              const glEnum = Number.isInteger(numericKey) ? numericKey : gl[key];
              if (!Number.isInteger(glEnum)) throw new Error('configured WebGL parameter enum is invalid');
              parameters[key] = normalize(gl.getParameter(glEnum));
            }
            const shaderPrecision = {};
            for (const key of spec.shader_precision) {
              const parts = key.split(',').map(Number);
              if (parts.length !== 2 || !parts.every(Number.isInteger)) continue;
              try {
                const value = gl.getShaderPrecisionFormat(parts[0], parts[1]);
                if (value) {
                  shaderPrecision[key] = {
                    rangeMin: value.rangeMin,
                    rangeMax: value.rangeMax,
                    precision: value.precision,
                  };
                }
              } catch (_) {}
            }
            const attributes = {};
            const actualAttributes = gl.getContextAttributes();
            if (actualAttributes) {
              for (const key of spec.context_attributes) {
                if (Object.prototype.hasOwnProperty.call(actualAttributes, key)) {
                  attributes[key] = normalize(actualAttributes[key]);
                }
              }
            }
            const debug = gl.getExtension('WEBGL_debug_renderer_info');
            return {
              vendor: debug ? gl.getParameter(debug.UNMASKED_VENDOR_WEBGL) : gl.getParameter(gl.VENDOR),
              renderer: debug ? gl.getParameter(debug.UNMASKED_RENDERER_WEBGL) : gl.getParameter(gl.RENDERER),
              extensions: Array.from(gl.getSupportedExtensions() || []),
              parameters,
              shader_precision: shaderPrecision,
              context_attributes: attributes,
            };
          };

          const webgl = collectContext('webgl', request.webgl);
          const webgl2 = collectContext('webgl2', request.webgl2);
          write({
            vendor: webgl ? webgl.vendor : (webgl2 ? webgl2.vendor : null),
            renderer: webgl ? webgl.renderer : (webgl2 ? webgl2.renderer : null),
            webgl: webgl ? {
              extensions: webgl.extensions,
              parameters: webgl.parameters,
              shader_precision: webgl.shader_precision,
              context_attributes: webgl.context_attributes,
            } : null,
            webgl2: webgl2 ? {
              extensions: webgl2.extensions,
              parameters: webgl2.parameters,
              shader_precision: webgl2.shader_precision,
              context_attributes: webgl2.context_attributes,
            } : null,
          });
        } catch (error) {
          write({
            error: {
              name: error && error.name ? String(error.name) : 'Error',
              message: error && error.message ? String(error.message) : '',
            },
          });
        }
      })();
    `;

    const host = document.documentElement || document.head || document.body;
    if (!host) throw new Error('browser-visible page realm is unavailable');
    host.appendChild(requestNode);
    host.appendChild(resultNode);
    host.appendChild(runner);
    const encoded = resultNode.textContent;
    runner.remove();
    requestNode.remove();
    resultNode.remove();
    if (!encoded) throw new Error('browser-visible page-realm graphics observation is empty');
    const result = JSON.parse(encoded);
    if (result && result.error) throw new Error('browser-visible page-realm graphics observation failed');
    return result;
  };

  const graphics = collectGraphicsInPageRealm({webgl: request.webgl, webgl2: request.webgl2});
  const fonts = request.fonts.map((family) => {
    const escaped = family.replace(/\\/g, '\\\\').replace(/"/g, '\\"');
    return {
      family,
      available: document.fonts.check(`16px "${escaped}"`, 'mmmmmmmmmmwwwwwwww'),
    };
  });

  const collectVoices = async () => {
    if (!request.voices_applicable) return {status: 'not_applicable'};
    if (!('speechSynthesis' in window)) return {status: 'unavailable'};
    // Gecko initializes this registry asynchronously, especially on cold Windows hosts.
    // Collect a complete, settled snapshot within the native observation deadline.
    // A timeout still returns the actual values; only Rust can admit the identity.
    const deadline = performance.now() + 10000;
    const normalizeVoices = (voices) => voices.map((voice) => ({
      language: voice.lang,
      name: voice.name,
      voice_uri: voice.voiceURI,
      local_service: voice.localService,
      is_default: voice.default,
    }));
    let voices = [];
    let prior = null;
    let stableSince = performance.now();
    while (performance.now() < deadline) {
      voices = speechSynthesis.getVoices();
      const snapshot = voices.length > 0 && voices.length === request.voice_count
        ? JSON.stringify(normalizeVoices(voices)) : null;
      const now = performance.now();
      if (snapshot !== null && snapshot === prior && now - stableSince >= 500) break;
      if (snapshot !== prior) {
        prior = snapshot;
        stableSince = now;
      }
      await new Promise((resolve) => setTimeout(resolve, 100));
    }
    voices = speechSynthesis.getVoices();
    if (voices.length === 0) return {status: 'unavailable'};
    return {
      status: 'available',
      voices: normalizeVoices(voices),
    };
  };

  return {
    user_agent: navigator.userAgent,
    platform: navigator.platform,
    oscpu: typeof navigator.oscpu === 'string' ? navigator.oscpu : null,
    hardware_concurrency: navigator.hardwareConcurrency,
    device_memory_gib: typeof navigator.deviceMemory === 'number' ? navigator.deviceMemory : null,
    max_touch_points: navigator.maxTouchPoints,
    screen: {
      width: screen.width,
      height: screen.height,
      avail_width: screen.availWidth,
      avail_height: screen.availHeight,
      avail_left: screen.availLeft,
      avail_top: screen.availTop,
      color_depth: screen.colorDepth,
      pixel_depth: screen.pixelDepth,
      device_pixel_ratio: window.devicePixelRatio,
    },
    graphics,
    fonts,
    language: navigator.language,
    languages: Array.from(navigator.languages || []),
    speech_voices: await collectVoices(),
  };
}
"""


class RuntimeContractError(ValueError):
    """Raised when the repository-owned runtime contract is not satisfied."""


def emit(frame: str) -> None:
    # IPC v3 owns an LF-only wire format. Writing through TextIOWrapper would translate
    # "\n" to CRLF on Windows and make the same canonical frame platform-dependent.
    sys.stdout.buffer.write((frame + "\n").encode("ascii"))
    sys.stdout.buffer.flush()


def canonical_json(value: object) -> bytes:
    return (
        json.dumps(value, ensure_ascii=True, separators=(",", ":"), sort_keys=True) + "\n"
    ).encode("utf-8")


def sha256_bytes(value: bytes) -> str:
    return hashlib.sha256(value).hexdigest()


def valid_sha256(value: str | None) -> bool:
    return value is not None and bool(re.fullmatch(r"[0-9a-f]{64}", value))


def read_regular_file(path: Path, maximum_bytes: int | None = None) -> bytes:
    if path.is_symlink() or not path.is_file():
        raise RuntimeContractError("required runtime file is not a regular file")
    if maximum_bytes is not None and path.stat().st_size > maximum_bytes:
        raise RuntimeContractError("required runtime file exceeds its bounded size")
    return path.read_bytes()


def load_canonical_json(
    path: Path, maximum_bytes: int | None = None
) -> tuple[dict[str, Any], bytes]:
    raw = read_regular_file(path, maximum_bytes)
    try:
        value = json.loads(raw.decode("utf-8"))
    except (UnicodeDecodeError, json.JSONDecodeError) as error:
        raise RuntimeContractError("runtime JSON is invalid") from error
    if not isinstance(value, dict) or canonical_json(value) != raw:
        raise RuntimeContractError("runtime JSON is not canonical")
    return value, raw


def browser_environment() -> dict[str, str]:
    # Windows expands known-folder registry paths through these OS directory variables.
    # Missing SYSTEMDRIVE can create a literal %SystemDrive% tree in the working directory.
    # Windows os.environ iteration uses uppercase keys; credentials remain excluded.
    allowed = {
        "ALLUSERSPROFILE",
        "APPDATA",
        "DBUS_SESSION_BUS_ADDRESS",
        "DISPLAY",
        "GDK_BACKEND",
        "HOME",
        "LANG",
        "LC_ALL",
        "LOCALAPPDATA",
        "LOGNAME",
        "PATH",
        "PROGRAMDATA",
        "SHELL",
        "SYSTEMDRIVE",
        "SYSTEMROOT",
        "TEMP",
        "TMP",
        "TMPDIR",
        "USER",
        "USERPROFILE",
        "WAYLAND_DISPLAY",
        "WINDIR",
        "WSL_DISTRO_NAME",
        "WSL_INTEROP",
        "XDG_CACHE_HOME",
        "XDG_CONFIG_HOME",
        "XDG_RUNTIME_DIR",
    }
    return {key: value for key, value in os.environ.items() if key in allowed}


def runtime_lock_path() -> Path:
    configured = os.environ.get(RUNTIME_LOCK_ENV)
    path = Path(configured) if configured else Path(__file__).with_name(RUNTIME_LOCK_NAME)
    if not path.is_absolute():
        path = path.resolve()
    return path


def validate_windows_distribution(lock: dict[str, Any]) -> None:
    distribution = lock.get("windows_distribution")
    if not isinstance(distribution, dict) or set(distribution) != {
        "architecture",
        "browser",
        "python",
    }:
        raise RuntimeContractError("Windows distribution lock shape is invalid")
    if distribution.get("architecture") != "x86_64":
        raise RuntimeContractError("Windows runtime architecture is unsupported")
    browser = distribution.get("browser")
    python = distribution.get("python")
    if not isinstance(browser, dict) or set(browser) != {
        "artifact_sha256",
        "artifact_url",
        "executable_path",
    }:
        raise RuntimeContractError("Windows browser distribution lock shape is invalid")
    if not isinstance(python, dict) or set(python) != {
        "artifact_sha256",
        "artifact_url",
        "packages",
        "version",
    }:
        raise RuntimeContractError("Windows Python distribution lock shape is invalid")
    for row in (browser, python):
        digest = row.get("artifact_sha256")
        url = row.get("artifact_url")
        if not isinstance(digest, str) or not valid_sha256(digest):
            raise RuntimeContractError("Windows distribution digest is invalid")
        if not isinstance(url, str) or not url.startswith("https://"):
            raise RuntimeContractError("Windows distribution URL is invalid")
    packages = python.get("packages")
    if not isinstance(packages, list) or not packages:
        raise RuntimeContractError("Windows Python dependency graph is invalid")
    for package in packages:
        if not isinstance(package, dict) or set(package) != {
            "filename",
            "name",
            "sha256",
            "url",
            "version",
        }:
            raise RuntimeContractError("Windows Python dependency row shape is invalid")
        filename = package.get("filename")
        name = package.get("name")
        version = package.get("version")
        digest = package.get("sha256")
        url = package.get("url")
        if (
            not isinstance(filename, str)
            or not filename
            or filename in {".", ".."}
            or "/" in filename
            or "\\" in filename
            or not isinstance(name, str)
            or not name
            or not isinstance(version, str)
            or not version
            or not isinstance(digest, str)
            or not valid_sha256(digest)
            or not isinstance(url, str)
            or not url.startswith("https://")
        ):
            raise RuntimeContractError("Windows Python dependency row is invalid")
    if browser.get("executable_path") != "browser/camoufox.exe":
        raise RuntimeContractError("Windows browser executable identity is unsupported")
    if python.get("version") != "3.12.10" or lock.get("python") != "3.12":
        raise RuntimeContractError("Windows Python distribution identity is unsupported")


def load_runtime_lock() -> tuple[dict[str, Any], str]:
    lock, raw = load_canonical_json(runtime_lock_path(), MAX_CONFIG_BYTES)
    required_top = {
        "browser",
        "camouhost_ipc_version",
        "components",
        "fingerprint_config_schema",
        "fingerprint_policy_version",
        "python",
        "python_source",
        "runtime_role",
        "schema_version",
        "windows_distribution",
    }
    if set(lock) != required_top:
        raise RuntimeContractError("runtime lock shape is invalid")
    if lock.get("schema_version") != 1 or lock.get("runtime_role") != "real_camoufox":
        raise RuntimeContractError("runtime lock identity is unsupported")
    if lock.get("camouhost_ipc_version") != int(IPC_VERSION):
        raise RuntimeContractError("runtime IPC identity is unsupported")
    validate_windows_distribution(lock)
    expected = os.environ.get(EXPECTED_RUNTIME_LOCK_SHA256_ENV)
    digest = sha256_bytes(raw)
    if expected is not None and (not valid_sha256(expected) or expected != digest):
        raise RuntimeContractError("runtime lock digest mismatch")
    return lock, digest


def verify_python_components(lock: dict[str, Any]) -> None:
    components = lock.get("components")
    if not isinstance(components, dict) or set(components) != {
        "camoufox_python",
        "fpgen",
        "playwright",
    }:
        raise RuntimeContractError("runtime component lock is invalid")
    distributions = {
        "camoufox_python": "camoufox",
        "fpgen": "fpgen",
        "playwright": "playwright",
    }
    for key, distribution in distributions.items():
        expected = components.get(key)
        if not isinstance(expected, str) or importlib.metadata.version(distribution) != expected:
            raise RuntimeContractError("installed runtime component does not match lock")

    # The pinned upstream model must be present before Camoufox can import fpgen.
    # No first-launch network fetch or hidden refresh is allowed.
    from camoufox.fpgen_model import is_pinned

    if not is_pinned():
        raise RuntimeContractError("official Camoufox fpgen model missing or corrupt")


def resolve_headless_mode() -> bool | str:
    value = os.environ.get(HEADLESS_MODE_ENV, "false")
    if value == "false":
        return False
    if value == "true":
        return True
    if value == "virtual":
        return "virtual"
    raise RuntimeContractError("unsupported Camouhost headless mode")


def resolve_proxy() -> dict[str, str] | None:
    configured = os.environ.get(PROXY_CONFIG_ENV)
    if configured is None:
        return None
    path = Path(configured)
    if not path.is_absolute():
        raise RuntimeContractError("proxy config path must be absolute")
    proxy, _ = load_canonical_json(path, 16 * 1024)
    if not set(proxy).issubset({"server", "username", "password", "bypass"}) or "server" not in proxy:
        raise RuntimeContractError("proxy config shape is invalid")
    if not all(isinstance(value, str) and value for value in proxy.values()):
        raise RuntimeContractError("proxy config values are invalid")
    return proxy


def require_bridge_writer_lock(root: Path) -> None:
    bridge_lock = root / BRIDGE_LOCK_NAME
    if bridge_lock.is_symlink() or not bridge_lock.is_file():
        raise RuntimeContractError("Bridge writer ownership evidence is missing")


def resolve_profile_root(require_bridge_lock: bool) -> Path:
    raw = os.environ.get(PROFILE_ROOT_ENV)
    if raw is None:
        raise RuntimeContractError("profile root is missing")
    candidate = Path(raw)
    if not candidate.is_absolute() or candidate.is_symlink() or not candidate.is_dir():
        raise RuntimeContractError("profile root is invalid")
    root = candidate.resolve(strict=True)
    if require_bridge_lock:
        require_bridge_writer_lock(root)
    return root


def load_generation_config(root: Path) -> tuple[dict[str, Any], str]:
    config, raw = load_canonical_json(root / CONFIG_NAME, MAX_CONFIG_BYTES)
    generation_webgl2_enabled(config)
    expected = os.environ.get(EXPECTED_CONFIG_SHA256_ENV)
    digest = sha256_bytes(raw)
    if not valid_sha256(expected) or expected != digest:
        raise RuntimeContractError("fingerprint config digest mismatch")
    return config, digest


def stable_probe_digest(page: Any) -> str:
    probe = page.evaluate(FINGERPRINT_PROBE)
    return sha256_bytes(canonical_json(probe))


# Only check that persisted Camoufox configuration is structurally coherent.
# Do not force browser WebGL capabilities from these fields.
WEBGL_REQUIRED_KEYS = (
    "webGl:parameters",
    "webGl:shaderPrecisionFormats",
    "webGl:contextAttributes",
)
WEBGL2_OPTIONAL_KEYS = (
    "webGl2:parameters",
    "webGl2:shaderPrecisionFormats",
    "webGl2:contextAttributes",
)


def generation_webgl2_enabled(config: dict[str, Any]) -> bool:
    for key in WEBGL_REQUIRED_KEYS:
        if not isinstance(config.get(key), dict):
            raise RuntimeContractError(f"materialized {key} observation surface is invalid")
    webgl2_found = [key in config for key in WEBGL2_OPTIONAL_KEYS]
    if any(webgl2_found) and not all(webgl2_found):
        raise RuntimeContractError("materialized WebGL2 observation surfaces are inconsistent")
    if all(webgl2_found):
        for key in WEBGL2_OPTIONAL_KEYS:
            if not isinstance(config[key], dict):
                raise RuntimeContractError(f"materialized {key} observation surface is invalid")
        return True
    # A disabled WebGL2 generation must not retain stray WebGL2 config keys.
    if any(key.startswith("webGl2:") for key in config):
        raise RuntimeContractError("materialized disabled WebGL2 has stray spoofing values")
    return False


def browser_visible_probe_request(config: dict[str, Any]) -> dict[str, Any]:
    enabled_webgl2 = generation_webgl2_enabled(config)
    fonts = config.get("fonts")
    if not isinstance(fonts, list) or not fonts or any(not isinstance(value, str) or not value for value in fonts):
        raise RuntimeContractError("materialized font observation surface is invalid")
    return {
        "webgl": {
            "parameters": list(config["webGl:parameters"].keys()),
            "shader_precision": list(config["webGl:shaderPrecisionFormats"].keys()),
            "context_attributes": list(config["webGl:contextAttributes"].keys()),
        },
        "webgl2": {
            "parameters": list(config["webGl2:parameters"].keys()),
            "shader_precision": list(config["webGl2:shaderPrecisionFormats"].keys()),
            "context_attributes": list(config["webGl2:contextAttributes"].keys()),
        } if enabled_webgl2 else None,
        "fonts": copy.deepcopy(fonts),
        "voices_applicable": "voices" in config,
        "voice_count": len(config.get("voices", [])),
    }


def browser_visible_observation(page: Any, config: dict[str, Any]) -> bytes:
    value = page.evaluate(BROWSER_VISIBLE_PROBE, browser_visible_probe_request(config))
    if not isinstance(value, dict):
        raise RuntimeContractError("browser-visible observation shape is invalid")
    payload = canonical_json(value)
    if not payload or len(payload) > MAX_BROWSER_VISIBLE_BYTES:
        raise RuntimeContractError("browser-visible observation exceeds bounded size")
    return payload


def emit_browser_visible(session_id: str, payload: bytes) -> None:
    emit(f"browser_visible|{session_id}|{payload.hex()}")


def decode_navigation_target(encoded: str) -> str | None:
    if encoded == "":
        return None
    if len(encoded) > MAX_URL_BYTES * 2 or len(encoded) % 2 != 0 or re.fullmatch(r"[0-9a-f]+", encoded) is None:
        raise RuntimeContractError("navigation target encoding is invalid")
    try:
        value = bytes.fromhex(encoded).decode("utf-8")
    except (ValueError, UnicodeDecodeError) as error:
        raise RuntimeContractError("navigation target encoding is invalid") from error
    if len(value.encode("utf-8")) > MAX_URL_BYTES or "\n" in value or "\r" in value or "\0" in value:
        raise RuntimeContractError("navigation target is invalid")
    if not value.startswith(("https://", "http://", "about:")):
        raise RuntimeContractError("navigation target scheme is not permitted")
    return value


def retire_managed_bootstrap_tabs(context: Any) -> None:
    """Remove only Camouhost's marked launch tab, never a user-created blank tab.

    Playwright 1.60 requires a normal page during Firefox persistent launch. The
    -new-tab argument creates one on every cold Open; native Firefox SessionStore
    also restores prior tabs. A distinct about:blank fragment lets us identify
    our own bootstrap without guessing whether the user's about:blank is safe to
    close. Do not clean up before Rust identity admission.
    """
    pages = list(context.pages)
    owned = [page for page in pages if page.url == MANAGED_BOOTSTRAP_TAB_URL]
    if not owned:
        return
    other = [page for page in pages if page.url != MANAGED_BOOTSTRAP_TAB_URL]
    if not other:
        # Brand-new/otherwise empty window: preserve exactly one visible tab.
        # This about:blank transition cannot send an external request.
        owned[0].goto("about:blank", wait_until="domcontentloaded", timeout=5_000)
        owned = owned[1:]
    for page in owned:
        page.close()


def disposable_probe_origin(target: str) -> bool:
    """Only the separately authorized C09 test's own ephemeral loopback origin."""
    try:
        parsed = urlsplit(target)
        port = parsed.port
    except ValueError:
        return False
    return (
        parsed.scheme == "http"
        and parsed.hostname == "127.0.0.1"
        and port is not None
        and 1024 <= port <= 65535
        and parsed.netloc == f"127.0.0.1:{port}"
        and parsed.path == "/"
        and not parsed.query
        and not parsed.fragment
        and parsed.username is None
        and parsed.password is None
    )


def admit_navigation(
    context: Any, target: str | None, *, isolated_loopback_probe: bool = False
) -> None:
    """Admit an already-authorized isolated context, never a NativeUser browser.

    The disposable C09 portable-state fixture may visit only its own exact
    loopback test origin. Open a NEW tab: never navigate any restored user tab
    or modify SessionStore ourselves. No user-browser IPC calls this function.
    """
    if target is not None and not (
        isolated_loopback_probe and disposable_probe_origin(target)
    ):
        raise RuntimeContractError("Profile launch cannot navigate the browser")
    # Retire only the adapter-created bootstrap, after Rust identity admission.
    retire_managed_bootstrap_tabs(context)
    context.set_offline(False)
    if target is not None:
        # This is a disposable, separately admitted C09 fixture, not live Open.
        context.new_page().goto(target, wait_until="domcontentloaded", timeout=15_000)
    print(
        "camouhost_session_restore native_retry_attempts=0 failures=0",
        file=sys.stderr,
        flush=True,
    )

def extract_camoufox_config(options: dict[str, Any]) -> dict[str, Any]:
    env = options.get("env")
    if not isinstance(env, dict):
        raise RuntimeContractError("Camoufox did not materialize config environment")
    chunks: list[tuple[int, str]] = []
    for name, value in env.items():
        if not isinstance(name, str) or not name.startswith("CAMOU_CONFIG_"):
            continue
        suffix = name.rsplit("_", 1)[-1]
        if not suffix.isdigit():
            raise RuntimeContractError("Camoufox config chunk is invalid")
        chunks.append((int(suffix), str(value)))
    if not chunks:
        raise RuntimeContractError("Camoufox did not materialize fingerprint config")
    try:
        value = json.loads("".join(chunk for _, chunk in sorted(chunks)))
    except json.JSONDecodeError as error:
        raise RuntimeContractError("materialized Camoufox config is invalid") from error
    if not isinstance(value, dict):
        raise RuntimeContractError("materialized Camoufox config shape is invalid")
    return value


def locked_firefox_major(lock: dict[str, Any]) -> int:
    """Project the managed Firefox major only from the immutable runtime lock."""
    browser = lock.get("browser")
    version = browser.get("version") if isinstance(browser, dict) else None
    if not isinstance(version, str):
        raise RuntimeContractError("browser lock is invalid")
    match = re.fullmatch(r"([1-9][0-9]*)\.[0-9]+\.[0-9]+-beta\.[0-9]+", version)
    if match is None:
        raise RuntimeContractError("browser lock version is unsupported")
    return int(match.group(1))


def packaged_windows_browser(lock: dict[str, Any]) -> Path:
    distribution = lock.get("windows_distribution")
    if not isinstance(distribution, dict):
        raise RuntimeContractError("Windows distribution lock is invalid")
    browser = distribution.get("browser")
    if not isinstance(browser, dict):
        raise RuntimeContractError("Windows browser distribution lock is invalid")
    executable_path = browser.get("executable_path")
    if executable_path != "browser/camoufox.exe":
        raise RuntimeContractError("Windows browser executable identity is unsupported")

    runtime_entrypoint = Path(__file__)
    if runtime_entrypoint.is_symlink() or not runtime_entrypoint.is_file():
        raise RuntimeContractError("Camouhost runtime entrypoint is not a regular file")
    runtime_root = runtime_entrypoint.resolve(strict=True).parent.parent
    executable = runtime_root.joinpath(*executable_path.split("/"))
    if executable.is_symlink() or not executable.is_file():
        raise RuntimeContractError("packaged Windows browser executable is unavailable")
    return executable.resolve(strict=True)


def camoufox_browser_selector(lock: dict[str, Any]) -> dict[str, str | Path]:
    browser = lock.get("browser")
    if not isinstance(browser, dict) or not isinstance(browser.get("version"), str):
        raise RuntimeContractError("browser lock is invalid")
    if os.name == "nt":
        return {"executable_path": packaged_windows_browser(lock)}
    if os.name == "posix":
        return {"browser": browser["version"]}
    raise RuntimeContractError("unsupported Camoufox runtime platform")


def accessibility_preferences(platform_name: str) -> dict[str, int]:
    """Keep standard OS accessibility available; no remote debugging, no page inspection."""
    return {"accessibility.force_disabled": 0} if platform_name == "nt" else {}


def report_live_page_count(context: Any) -> None:
    """Best-effort count only. Observed Playwright pages are not a restored-tab proof."""
    try:
        pages = len(context.pages)
        if 0 <= pages <= 512:
            print(f"camouhost_pages count={pages}", file=sys.stderr, flush=True)
    except Exception:
        # Diagnostics must never fail or change the browser's navigation outcome.
        return


def firefox_native_first_run_args(user_data_dir: Path) -> list[str]:
    """Ask Firefox itself for its ordinary blank tab only on a virgin profile.

    Playwright's persistent context needs an initial native Page event; native
    session restore (startup.page=3) has no previous page on first Open. Use
    Firefox's standard CLI switch rather than opening, inspecting, closing or
    navigating tabs through the application. All subsequent Opens preserve the
    browser's own SessionStore without CLI startup overrides.
    """
    return ["-new-tab", "about:blank"] if not any(user_data_dir.iterdir()) else []


def camoufox_launch_options(
    lock: dict[str, Any],
    root: Path,
    config: dict[str, Any],
    *,
    isolated: bool = False,
) -> dict[str, Any]:
    """Build one pinned option set through the official Camoufox SDK.

    Normal user Open uses upstream default extensions and the persistent
    profile, without bootstrap tabs, offline toggles or page inspection.
    The isolated C09 probe alone opts into its pre-admission offline boundary.
    """
    from camoufox.utils import get_env_vars, launch_options

    user_data_dir = root / USER_DATA_NAME
    if user_data_dir.is_symlink():
        raise RuntimeContractError("browser user-data directory may not be a symlink")
    user_data_dir.mkdir(exist_ok=True)
    browser_selector = camoufox_browser_selector(lock)
    browser_path = browser_selector.get("executable_path")
    firefox_prefs: dict[str, Any] = {
        "browser.startup.page": 3,
        "privacy.baselineFingerprintingProtection": False,
        **accessibility_preferences(os.name),
    }
    if isolated:
        firefox_prefs["network.disable-localhost-when-offline"] = True
    try:
        options = launch_options(
            **browser_selector,
            config=copy.deepcopy(config),
            enable_cache=True,
            env=browser_environment(),
            ff_version=locked_firefox_major(lock),
            firefox_user_prefs=firefox_prefs,
            headless=resolve_headless_mode(),
            i_know_what_im_doing=True,
            os="windows",
            proxy=resolve_proxy(),
            user_data_dir=str(user_data_dir),
        )
    except (OSError, ValueError, TypeError) as error:
        raise RuntimeContractError("official Camoufox launch options rejected") from error

    if isolated:
        options["offline"] = True
        options["args"] = ["-new-tab", MANAGED_BOOTSTRAP_TAB_URL]
    else:
        # Firefox itself creates the one native first-run tab. Existing
        # profiles receive no startup override and retain native SessionStore.
        first_run_args = firefox_native_first_run_args(user_data_dir)
        if first_run_args:
            options["args"] = first_run_args

    official_env = options.get("env")
    if not isinstance(official_env, dict) or not any(
        isinstance(key, str) and key.startswith("CAMOU_CONFIG_") for key in official_env
    ):
        raise RuntimeContractError("official Camoufox fingerprint environment missing")
    if any(not isinstance(key, str) for key in official_env):
        raise RuntimeContractError("official Camoufox environment invalid")

    # Preserve the generation's exact sealed identity while retaining the
    # official SDK's generated default-addon list and all its other options.
    generated = extract_camoufox_config(options)
    pinned = copy.deepcopy(config)
    addons = generated.get("addons")
    if addons is not None:
        if not isinstance(addons, list) or any(not isinstance(value, str) for value in addons):
            raise RuntimeContractError("official Camoufox addon list invalid")
        if "addons" in pinned and pinned["addons"] != addons:
            raise RuntimeContractError("persisted addon identity conflicts with upstream")
        pinned["addons"] = addons
    try:
        pinned_env = get_env_vars(pinned, "win", path=browser_path)
    except (OSError, TypeError, ValueError) as error:
        raise RuntimeContractError("pinned Camoufox fingerprint unavailable") from error
    if not isinstance(pinned_env, dict) or not any(
        isinstance(key, str) and key.startswith("CAMOU_CONFIG_") for key in pinned_env
    ):
        raise RuntimeContractError("pinned Camoufox fingerprint serialization missing")
    merged_env = {
        key: str(value) for key, value in official_env.items()
        if not key.startswith("CAMOU_CONFIG_")
    }
    merged_env.update({key: str(value) for key, value in pinned_env.items()})
    options["env"] = merged_env
    return options




def launch_verified_context(
    lock: dict[str, Any],
    root: Path,
    config: dict[str, Any],
    expected_probe_sha256: str,
) -> tuple[Any, Any]:
    from camoufox.sync_api import Camoufox

    if not valid_sha256(expected_probe_sha256):
        raise RuntimeContractError("profile-stable probe digest is invalid")
    options = camoufox_launch_options(lock, root, config, isolated=True)
    manager = Camoufox(
        from_options=options,
        headless=resolve_headless_mode(),
        persistent_context=True,
    )
    try:
        report_runtime_stage("launch", "before_browser_enter")
        with contextlib.redirect_stdout(sys.stderr):
            context = manager.__enter__()
        report_runtime_stage("launch", "browser_entered")
        # Shipping browser starts network-offline. Rust/browser-execution-domain is the sole
        # semantic identity admission owner and the only authority that can release navigation.
        # The aggregate probe digest remains materialization/integrity evidence only; Python must
        # not compare it to browser behavior and independently veto typed Rust admission.
        context.set_offline(True)
        report_runtime_stage("launch", "network_blocked")
        return manager, context
    except BaseException:
        with contextlib.suppress(BaseException):
            with contextlib.redirect_stdout(sys.stderr):
                manager.__exit__(*sys.exc_info())
        raise


def install_context_close_observation(context: Any) -> dict[str, Any]:
    """Subscribe to Playwright's public context-close event; no process/window probes."""
    state: dict[str, Any] = {"closed": False, "event_count": 0}

    def on_close(_context: Any = None) -> None:
        state["closed"] = True
        state["event_count"] += 1

    context.on("close", on_close)
    try:
        if context.is_closed():
            state["closed"] = True
    except Exception:
        # The event remains authoritative; an unreadable context is not Save proof.
        pass
    return state


def context_close_observed(context: Any, state: dict[str, Any] | None) -> bool:
    """Read the official event state and Playwright's public is_closed property."""
    if state is None:
        return False
    if state.get("closed") is True:
        return True
    try:
        closed = bool(context.is_closed())
    except Exception:
        return False
    if closed:
        state["closed"] = True
    return closed


def close_context(manager: Any, _context: Any) -> None:
    """Release an isolated automation context through the official SDK manager."""
    with contextlib.redirect_stdout(sys.stderr):
        manager.__exit__(None, None, None)


def materialize_candidate_identity(root: Path) -> dict[str, str]:
    """Create exact generation identity once under an already acquired Bridge writer lock."""
    from camoufox.fingerprints import generate_fingerprint
    from camoufox.utils import get_screen_cons, launch_options

    lock, runtime_lock_sha256 = load_runtime_lock()
    verify_python_components(lock)
    if root.is_symlink() or not root.is_dir():
        raise RuntimeContractError("candidate generation root is invalid")
    root = root.resolve(strict=True)
    require_bridge_writer_lock(root)
    config_path = root / CONFIG_NAME
    user_data_dir = root / USER_DATA_NAME
    if config_path.exists() or config_path.is_symlink() or user_data_dir.is_symlink():
        raise RuntimeContractError("candidate identity already exists or path is unsafe")
    user_data_dir.mkdir(exist_ok=True)
    browser_selector = camoufox_browser_selector(lock)
    browser_env = browser_environment()
    headless_mode = resolve_headless_mode()
    fingerprint = generate_fingerprint(
        screen=get_screen_cons(headless_mode or "DISPLAY" in browser_env),
        os="windows",
    )
    with contextlib.redirect_stdout(sys.stderr):
        options = launch_options(
            **browser_selector,
            enable_cache=True,
            env=browser_env,
            ff_version=locked_firefox_major(lock),
            fingerprint=fingerprint,
            headless=headless_mode,
            i_know_what_im_doing=True,
            os="windows",
            user_data_dir=str(user_data_dir),
        )
    config = extract_camoufox_config(options)
    # Persist the canonical Camoufox configuration once; never draw again on Reopen.
    generation_webgl2_enabled(config)
    config_bytes = canonical_json(config)
    config_path.write_bytes(config_bytes)
    config_sha256 = sha256_bytes(config_bytes)

    # Native Firefox belongs to the subsequent user Open, never to a
    # preparatory first-generation task. The SDK/fpgen owns the exact pinned
    # fingerprint; no browser/Playwright window is spawned merely to sample
    # its own configuration. Preserve this domain-separated provenance in the
    # encrypted base generation and validate it again from the saved bytes.
    materialization_digest = sha256_bytes(
        b"profile-stability-v2-fpgen-config\0" + config_bytes
    )
    return {
        "fingerprint_config_sha256": config_sha256,
        "fingerprint_policy_version": str(lock["fingerprint_policy_version"]),
        "fingerprint_materialization_sha256": materialization_digest,
        "runtime_lock_sha256": runtime_lock_sha256,
    }


def valid_frame(raw: str) -> bool:
    return (
        0 < len(raw) <= MAX_FRAME_LENGTH
        and "\r" not in raw
        and "\0" not in raw
        and raw.endswith("\n")
    )


def runtime_failure_category(error: BaseException) -> str:
    if isinstance(error, importlib.metadata.PackageNotFoundError):
        return "dependency"
    if isinstance(error, RuntimeContractError):
        return "contract"
    elif isinstance(error, TimeoutError) or type(error).__name__ == "TimeoutError":
        # Playwright's TimeoutError is not the builtin TimeoutError.
        return "timeout"
    elif isinstance(error, OSError):
        return "io"
    elif type(error).__name__ == "TargetClosedError":
        return "browser_closed"
    return "unexpected"


def report_runtime_failure(phase: str, error: BaseException) -> None:
    """Report only closed diagnostic categories; exception text may contain profile data."""
    if phase not in {"startup", "launch", "browser_visible", "navigation", "close", "cleanup"}:
        phase = "unknown"
    category = runtime_failure_category(error)
    print(f"camouhost_failure phase={phase} category={category}", file=sys.stderr, flush=True)


def report_runtime_stage(phase: str, step: str) -> None:
    """Only fixed protocol milestones, never Python/Playwright's arbitrary stderr."""
    allowed = {
        ("startup", "begin"),
        ("startup", "ready"),
        ("launch", "before_browser_enter"),
        ("launch", "options_ready"),
        ("launch", "sdk_enter"),
        ("launch", "browser_entered"),
        ("launch", "network_blocked"),
        ("launch", "ready"),
        ("close", "browser_context_closed"),
        ("close", "playwright_driver_released"),
    }
    if (phase, step) in allowed:
        print(f"camouhost_stage phase={phase} step={step}", file=sys.stderr, flush=True)



def durable_agent_directory(path: Path) -> None:
    """Persist file creation/rename on Unix; Windows file contents are fsynced separately."""
    if os.name == "posix":
        descriptor = os.open(path, os.O_RDONLY)
        try:
            os.fsync(descriptor)
        finally:
            os.close(descriptor)


def current_process_instance() -> dict[str, str]:
    """OS process creation identity; a PID alone can later belong to another process."""
    if os.name == "nt":
        import ctypes
        from ctypes import wintypes

        class FILETIME(ctypes.Structure):
            _fields_ = [("low", wintypes.DWORD), ("high", wintypes.DWORD)]

        kernel = ctypes.WinDLL("kernel32", use_last_error=True)
        kernel.GetCurrentProcess.restype = wintypes.HANDLE
        handle = kernel.GetCurrentProcess()
        get_times = kernel.GetProcessTimes
        get_times.argtypes = [wintypes.HANDLE] + [ctypes.POINTER(FILETIME)] * 4
        get_times.restype = wintypes.BOOL
        created, exited, kernel_time, user_time = (FILETIME() for _ in range(4))
        if not get_times(handle, ctypes.byref(created), ctypes.byref(exited),
                         ctypes.byref(kernel_time), ctypes.byref(user_time)):
            raise OSError(ctypes.get_last_error(), "Camouhost process creation time unavailable")
        image = ctypes.create_unicode_buffer(32768)
        length = wintypes.DWORD(len(image))
        query = kernel.QueryFullProcessImageNameW
        query.argtypes = [wintypes.HANDLE, wintypes.DWORD, wintypes.LPWSTR,
                          ctypes.POINTER(wintypes.DWORD)]
        query.restype = wintypes.BOOL
        if not query(handle, 0, image, ctypes.byref(length)):
            raise OSError(ctypes.get_last_error(), "Camouhost process image unavailable")
        return {
            "created": str((created.high << 32) | created.low),
            "image": os.path.normcase(image.value),
        }
    pid = os.getpid()
    stat = Path(f"/proc/{pid}/stat").read_text(encoding="utf-8")
    return {
        "created": stat[stat.rfind(")") + 2:].split()[19],
        "image": os.readlink(f"/proc/{pid}/exe"),
    }


class LocalPlaywrightAgent:
    """Explicitly opted-in local test runner; the live Camouhost thread owns Playwright.

    Files are an operator-controlled, on-machine command inbox, not a browser port,
    a second browser writer, or a server-side control plane. Source and raw output
    are deliberately retained for full-fidelity test diagnostics.
    """

    def __init__(self, directory: str, session_id: str) -> None:
        base = Path(directory)
        if not base.is_absolute() or base.is_symlink() or not base.is_dir():
            raise RuntimeContractError("local agent directory must exist and be a regular absolute directory")
        self.base = base
        self.session_id = session_id
        self.run = base / f"camouhost-{os.getpid()}-{secrets.token_hex(12)}"
        self.run.mkdir(mode=0o700)
        self.requests = self.run / "requests"
        self.results = self.run / "results"
        self.requests.mkdir(mode=0o700)
        self.results.mkdir(mode=0o700)
        # In-process, session-scoped negative evidence. Never redispatch a job
        # after an uncertain rename or directory durability failure.
        self.unconfirmed_claims: set[str] = set()
        self.descriptor = base / f"active-{os.getpid()}.json"
        descriptor_temp = base / f".active-{os.getpid()}-{secrets.token_hex(8)}.tmp"
        with descriptor_temp.open("x", encoding="utf-8") as output:
            output.write(json.dumps({"pid": os.getpid(), "session": session_id, "run": str(self.run), "process": current_process_instance()}))
            output.flush()
            os.fsync(output.fileno())
        os.replace(descriptor_temp, self.descriptor)
        durable_agent_directory(base)

    @contextlib.contextmanager
    def executing_witness(self) -> Any:
        """Transient busy witness for Bridge close observation, not a Save receipt."""
        marker = self.run / "agent-executing"
        with marker.open("x", encoding="utf-8") as output:
            output.write(self.session_id)
            output.flush()
            os.fsync(output.fileno())
        try:
            yield
        finally:
            # The .running request and result receipt survive independently.
            marker.unlink(missing_ok=True)

    def run_one(self, context: Any) -> bool:
        # Never accept half-written jobs. The agent commits a request by atomic rename.
        for request in sorted(self.requests.glob("*.py")):
            if not AGENT_REQUEST_PATTERN.fullmatch(request.name) or request.is_symlink():
                continue
            job_id = request.stem
            if job_id in self.unconfirmed_claims:
                continue
            claimed = self.requests / f"{job_id}.running"
            phase = "claim_rename"
            try:
                os.replace(request, claimed)
                phase = "claim_directory_sync"
                durable_agent_directory(self.requests)
            except OSError as error:
                # Rename may already have happened. Never automatically replay this
                # request, even when the failed claim still has its .py name.
                self.unconfirmed_claims.add(job_id)
                print(
                    f"camouhost_agent_claim_unconfirmed phase={phase} "
                    f"job={job_id} errno={error.errno} "
                    f"winerror={getattr(error, 'winerror', None)}",
                    file=sys.stderr, flush=True,
                )
                # Best effort typed receipt for the original CLI, not an
                # acknowledgement that the script ran or may be retried.
                try:
                    outcome = {
                        "status": "unconfirmed", "phase": phase, "job_id": job_id,
                        "errno": error.errno,
                        "winerror": getattr(error, "winerror", None),
                    }
                    staged = self.results / f"{job_id}.part"
                    with staged.open("x", encoding="utf-8") as output:
                        output.write(json.dumps(outcome))
                        output.flush()
                        os.fsync(output.fileno())
                    os.replace(staged, self.results / f"{job_id}.json")
                    durable_agent_directory(self.results)
                except OSError:
                    # The original request, claimed file, and stderr diagnosis
                    # remain intact even if the diagnostic volume is unavailable.
                    pass
                continue
            stdout_path = self.results / f"{job_id}.stdout.txt"
            stderr_path = self.results / f"{job_id}.stderr.txt"
            status_path = self.results / f"{job_id}.json"
            outcome: dict[str, Any] = {"status": "ok"}
            with stdout_path.open("w", encoding="utf-8") as stdout, \
                    stderr_path.open("w", encoding="utf-8") as stderr:
                try:
                    # This is deliberately unrestricted local Python for the owner's
                    # TEST environment: all existing profiles and Playwright APIs
                    # are available. Only the already-owned context is supplied.
                    scope: dict[str, Any] = {
                        "__name__": "__camouhost_agent__",
                        "context": context,
                        "pages": context.pages,
                        "session_id": self.session_id,
                        "agent_artifacts_dir": self.results,
                        "result": None,
                    }
                    with contextlib.redirect_stdout(stdout), contextlib.redirect_stderr(stderr):
                        exec(compile(claimed.read_text(encoding="utf-8"), str(claimed), "exec"), scope)
                    outcome["result"] = scope.get("result")
                except BaseException:
                    traceback.print_exc(file=stderr)
                    outcome["status"] = "error"
                # A success cannot be confirmed before raw evidence is durable.
                stdout.flush()
                stderr.flush()
                os.fsync(stdout.fileno())
                os.fsync(stderr.fileno())
            staging = self.results / f"{job_id}.part"
            try:
                with staging.open("x", encoding="utf-8") as output:
                    output.write(json.dumps(outcome, ensure_ascii=False, default=str))
                    output.flush()
                    os.fsync(output.fileno())
                os.replace(staging, status_path)
                durable_agent_directory(self.results)
            except OSError as error:
                # The job is already consumed; a failed result publication is
                # UNKNOWN, not permission to execute it again. Original source,
                # stdout and stderr remain local for forensic readback.
                print(
                    f"camouhost_agent_result_write_failed phase=receipt_publication "
                    f"job={job_id} errno={error.errno} "
                    f"winerror={getattr(error, 'winerror', None)}",
                    file=sys.stderr, flush=True,
                )
            return True
        return False

    def close(self) -> None:
        # The complete test evidence remains. Only the live-session locator is retired.
        try:
            current = json.loads(self.descriptor.read_text(encoding="utf-8"))
            if current.get("run") == str(self.run):
                self.descriptor.unlink()
        except (OSError, ValueError, AttributeError):
            pass


class BridgeInputPoller:
    """Nonblocking framed stdin, used only in explicit test mode on the owner thread.

    Do not use a second stdin thread on Windows: it can prevent the Camoufox
    launch handshake from reaching Ready. The normal blocking path is untouched.
    Use raw os.read rather than TextIO.readline so Python cannot retain frames
    in an invisible internal buffer after the OS pipe has become empty.
    """

    def __init__(self) -> None:
        self.fd = sys.stdin.fileno()
        self.pending = bytearray()
        self.eof = False
        self.peek = None
        self.handle = None
        if os.name == "nt":
            import ctypes
            import msvcrt

            self.ctypes = ctypes
            self.handle = ctypes.c_void_p(msvcrt.get_osfhandle(self.fd))
            self.peek = ctypes.WinDLL("kernel32", use_last_error=True).PeekNamedPipe
            self.peek.argtypes = [
                ctypes.c_void_p, ctypes.c_void_p, ctypes.c_uint32,
                ctypes.c_void_p, ctypes.POINTER(ctypes.c_uint32), ctypes.c_void_p,
            ]
            self.peek.restype = ctypes.c_int

    def read(self) -> str | None:
        """Return one complete line, empty string on EOF, or None when idle."""
        while True:
            end = self.pending.find(b"\n")
            if end >= 0:
                if end + 1 > MAX_FRAME_LENGTH:
                    raise RuntimeContractError("agent stdin frame exceeded protocol limit")
                raw = bytes(self.pending[:end + 1])
                del self.pending[:end + 1]
                return raw.decode("utf-8")
            if len(self.pending) > MAX_FRAME_LENGTH:
                raise RuntimeContractError("agent stdin frame exceeded protocol limit")
            if self.eof:
                if self.pending:
                    raw = bytes(self.pending)
                    self.pending.clear()
                    return raw.decode("utf-8")  # Missing newline fails valid_frame.
                return ""
            if os.name == "nt":
                assert self.peek is not None and self.handle is not None
                available = self.ctypes.c_uint32()
                if not self.peek(self.handle, None, 0, None, self.ctypes.byref(available), None):
                    error = self.ctypes.get_last_error()
                    if error in (6, 109, 233):  # Invalid/broken/disconnected pipe.
                        self.eof = True
                        continue
                    raise OSError(error, "PeekNamedPipe failed")
                if available.value == 0:
                    time.sleep(0.05)
                    return None
                chunk = os.read(self.fd, min(available.value, 4096))
            else:
                import select

                if not select.select([self.fd], [], [], 0.05)[0]:
                    return None
                chunk = os.read(self.fd, 4096)
            if not chunk:
                self.eof = True
            else:
                self.pending.extend(chunk)


def run_ipc() -> int:
    report_runtime_stage("startup", "begin")
    try:
        lock, _ = load_runtime_lock()
        verify_python_components(lock)
        root = resolve_profile_root(require_bridge_lock=True)
        config, _ = load_generation_config(root)
        expected_probe = os.environ.get(EXPECTED_PROBE_SHA256_ENV)
        if not valid_sha256(expected_probe):
            raise RuntimeContractError("expected profile-stable probe digest is missing")
    except Exception as error:
        report_runtime_failure("startup", error)
        emit("error|identity")
        return 4

    report_runtime_stage("startup", "ready")
    negotiated = False
    active_session: str | None = None
    manager: Any | None = None
    context: Any | None = None
    context_close_state: dict[str, Any] | None = None
    browser_visible_observed = False
    navigation_admitted = False
    post_navigation_browser_visible_observed = False
    agent: LocalPlaywrightAgent | None = None
    # Normal user launches retain the original blocking stdin path. In local
    # test mode the SAME thread polls the Bridge pipe and services agent jobs.
    poller = BridgeInputPoller() if os.environ.get(AGENT_DIRECTORY_ENV) else None

    try:
        while True:
            if poller is None:
                raw = sys.stdin.readline()
            else:
                try:
                    raw = poller.read()
                except (OSError, UnicodeError, RuntimeContractError):
                    emit("error|protocol")
                    return 2
                if raw is None:
                    if agent is not None and context is not None and navigation_admitted:
                        try:
                            # Lifecycle IPC wins whenever a frame is ready. Mark execution
                            # before claiming any pending test job; a concurrent Bridge
                            # ObserveClose must see BUSY rather than a false IPC death.
                            if any(agent.requests.glob("*.py")):
                                with agent.executing_witness():
                                    agent.run_one(context)
                        except OSError as error:
                            print(
                                f"camouhost_agent_io_failure phase=execution "
                                f"errno={error.errno} winerror={getattr(error, 'winerror', None)}",
                                file=sys.stderr, flush=True,
                            )
                    continue
            if not raw:
                break
            if not valid_frame(raw):
                emit("error|protocol")
                return 2
            parts = raw[:-1].split("|")

            if parts == ["hello", IPC_VERSION] and not negotiated and active_session is None:
                negotiated = True
                emit(f"hello_ack|{IPC_VERSION}")
                continue

            if (
                len(parts) == 2
                and parts[0] == "launch"
                and negotiated
                and active_session is None
                and SESSION_PATTERN.fullmatch(parts[1]) is not None
            ):
                try:
                    manager, context = launch_verified_context(lock, root, config, expected_probe)
                    context_close_state = install_context_close_observation(context)
                except BaseException as error:
                    report_runtime_failure("launch", error)
                    emit("error|runtime")
                    return 5
                active_session = parts[1]
                # Only an explicitly opted-in local test Bridge publishes an agent
                # directory. Ordinary user sessions have no executable-script inbox.
                if os.environ.get(AGENT_DIRECTORY_ENV):
                    try:
                        agent = LocalPlaywrightAgent(os.environ[AGENT_DIRECTORY_ENV], active_session)
                    except (OSError, RuntimeContractError) as error:
                        report_runtime_failure("launch", error)
                        emit("error|runtime")
                        return 5
                report_runtime_stage("launch", "ready")
                emit(f"ready|{active_session}")
                continue

            if (
                len(parts) == 2
                and parts[0] == "observe_browser_visible"
                and active_session is not None
                and parts[1] == active_session
                and manager is not None
                and context is not None
                and (
                    (not browser_visible_observed and not navigation_admitted)
                    or (
                        browser_visible_observed
                        and navigation_admitted
                        and not post_navigation_browser_visible_observed
                    )
                )
            ):
                try:
                    # The initial pre-navigation observation may create the
                    # first page as before. A later live-session witness must
                    # not create or resurrect a browser page: a user-closed
                    # browser is not an independently working reserve.
                    if context.pages:
                        page = context.pages[0]
                    elif not navigation_admitted:
                        page = context.new_page()
                    else:
                        raise RuntimeContractError("post-navigation browser page is absent")
                    payload = browser_visible_observation(page, config)
                except BaseException as error:
                    report_runtime_failure("browser_visible", error)
                    emit("error|runtime")
                    return 5
                if navigation_admitted:
                    post_navigation_browser_visible_observed = True
                else:
                    browser_visible_observed = True
                emit_browser_visible(active_session, payload)
                continue

            if (
                len(parts) == 3
                and parts[0] == "admit_navigation"
                and active_session is not None
                and parts[1] == active_session
                and manager is not None
                and context is not None
                and browser_visible_observed
                and not navigation_admitted
            ):
                try:
                    target = decode_navigation_target(parts[2])
                    admit_navigation(
                        context,
                        target,
                        isolated_loopback_probe=(
                            os.environ.get(EXECUTION_MODE_ENV) == ISOLATED_COLD_PROBE_MODE
                        ),
                    )
                except BaseException as error:
                    report_runtime_failure("navigation", error)
                    emit("error|runtime")
                    return 5
                navigation_admitted = True
                # Observes numbers only, never restored-tab identity or page content.
                report_live_page_count(context)
                emit(f"navigation_admitted|{active_session}")
                continue

            if (
                len(parts) == 2
                and parts[0] == "observe_close"
                and active_session is not None
                and parts[1] == active_session
                and manager is not None
                and context is not None
                and navigation_admitted
            ):
                try:
                    controlled = context_close_observed(context, context_close_state)
                except BaseException as error:
                    report_runtime_failure("close", error)
                    emit("error|runtime")
                    return 5
                emit(f"close_observed|{active_session}|{'true' if controlled else 'false'}")
                continue

            if (
                len(parts) == 2
                and parts[0] == "close"
                and active_session is not None
                and parts[1] == active_session
                and manager is not None
                and context is not None
                and navigation_admitted
            ):
                try:
                    close_context(manager, context)
                except BaseException as error:
                    report_runtime_failure("close", error)
                    emit(f"closed|{active_session}|false")
                    return 6
                manager = None
                context = None
                emit(f"closed|{active_session}|true")
                return 0

            emit("error|protocol")
            return 2

        return 3 if negotiated or active_session is not None else 0
    finally:
        if agent is not None:
            agent.close()
        if manager is not None and context is not None:
            try:
                close_context(manager, context)
            except BaseException as error:
                report_runtime_failure("cleanup", error)


async def run_local_profile(root: Path) -> int:
    """Launch one local profile and wait passively for Playwright's close event."""
    from native_process import launch_native_camoufox

    session = None
    context_closed = False
    try:
        lock, _ = load_runtime_lock()
        verify_python_components(lock)
        if root.is_symlink() or not root.is_dir():
            raise RuntimeContractError("local browser folder unavailable")
        root = root.resolve(strict=True)
        # Rust holds exclusive local writer admission until Python exits.
        require_bridge_writer_lock(root)
        config, _ = load_canonical_json(root / CONFIG_NAME, MAX_CONFIG_BYTES)
        generation_webgl2_enabled(config)
        options = camoufox_launch_options(lock, root, config)
        session = await launch_native_camoufox(
            workspace=root,
            options=options,
            admission_granted=True,
            headless=False,
        )
        report_runtime_stage("launch", "ready")

        # No native window/process inspection or SessionStore access.
        # Firefox closes itself; Playwright reports the official event.
        await session.wait_for_browser_close()
        context_closed = True
        report_runtime_stage("close", "browser_context_closed")

        # Release only the Playwright driver connection after the browser has
        # already closed. The OS profile lock is the file-safety boundary.
        await session.close_after_browser_exit()
        report_runtime_stage("close", "playwright_driver_released")
        return 0
    except BaseException as error:
        report_runtime_failure("close" if context_closed else "launch", error)
        # Never close or detach a launched user browser because of an
        # application-side error. Keep the SDK connection alive until the
        # browser itself closes, then release only the driver connection.
        if session is not None and not context_closed:
            try:
                await session.wait_for_browser_close()
                context_closed = True
                await session.close_after_browser_exit()
            except BaseException as close_error:
                report_runtime_failure("close", close_error)
        return 5


def main() -> int:
    if len(sys.argv) == 3 and sys.argv[1] == "--local-open":
        if os.environ.get(EXECUTION_MODE_ENV) != USER_NATIVE_MODE:
            print("local browser requires native execution", file=sys.stderr)
            return 2
        if os.environ.get(AGENT_DIRECTORY_ENV):
            print("local browser rejects agent directory", file=sys.stderr)
            return 2
        if os.environ.get(HEADLESS_MODE_ENV, "false") != "false":
            print("local browser requires headful mode", file=sys.stderr)
            return 2
        return asyncio.run(run_local_profile(Path(sys.argv[2])))
    if len(sys.argv) == 3 and sys.argv[1] == "--materialize-identity":
        try:
            with contextlib.redirect_stdout(sys.stderr):
                report = materialize_candidate_identity(Path(sys.argv[2]))
        except Exception as error:
            line = 0
            traceback = error.__traceback__
            while traceback is not None:
                if traceback.tb_frame.f_code.co_filename == __file__:
                    line = traceback.tb_lineno
                traceback = traceback.tb_next
            failure = {"category": runtime_failure_category(error), "line": line}
            print(json.dumps({"failure": failure}, separators=(",", ":")), flush=True)
            return 7
        print(json.dumps(report, ensure_ascii=True, separators=(",", ":"), sort_keys=True))
        return 0
    if len(sys.argv) != 1:
        print("unsupported Camouhost invocation", file=sys.stderr)
        return 2
    # Normal user Open uses the official Camoufox SDK/Playwright persistent
    # context, but never enters the page-inspecting automation IPC path. The
    # Bridge selects the owner only after device/tenant/generation admission.
    mode = os.environ.get(EXECUTION_MODE_ENV)
    if mode == USER_NATIVE_MODE:
        # A user's native Firefox never accepts a Python execution inbox,
        # including direct adapter invocation outside a packaged Rust build.
        if os.environ.get(AGENT_DIRECTORY_ENV):
            print("user browser rejects executable agent directory", file=sys.stderr)
            return 2
        if os.environ.get(HEADLESS_MODE_ENV, "false") != "false":
            print("user browser requires headful mode", file=sys.stderr)
            return 2
        print("user-native runtime requires --local-open", file=sys.stderr)
        return 2
    if mode == ISOLATED_COLD_PROBE_MODE:
        # Explicit isolated test/automation mode only. It must use a separate
        # disposable profile and can never be selected by the product's normal
        # user-native Open/Sync path. Do not expose an arbitrary Python job inbox.
        if os.environ.get(AGENT_DIRECTORY_ENV):
            print("isolated cold probe rejects executable agent directory", file=sys.stderr)
            return 2
        if os.name == "nt" and os.environ.get(HEADLESS_MODE_ENV, "false") != "false":
            # Windows C09 is pinned to a real headful Firefox. Linux CI's
            # isolated automation fixture may use official virtual display.
            print("isolated cold probe requires pinned headful browser", file=sys.stderr)
            return 2
        return run_ipc()
    if mode == AUTOMATION_PLAYWRIGHT_MODE:
        # No empty managed browser session: an explicit local automation
        # job inbox must be bound by the caller. This is a necessary guard,
        # not a substitute for Rust's tenant/device/lease/job authorization.
        if not os.environ.get(AGENT_DIRECTORY_ENV):
            print("Camouhost automation job binding missing", file=sys.stderr)
            return 2
        # run_ipc checks the signed runtime, generation and writer lock.
        # Shipping Windows Open intentionally never selects this mode.
        return run_ipc()
    print("Camouhost execution mode missing or unsupported", file=sys.stderr)
    return 2


if __name__ == "__main__":
    raise SystemExit(main())
