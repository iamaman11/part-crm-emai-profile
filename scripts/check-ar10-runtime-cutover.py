#!/usr/bin/env python3
"""Fail-closed AR-10 real-runtime and executable-retirement policy gate."""

from __future__ import annotations

import argparse
import ast
import json
import re
import sys
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
REAL_RUNTIME = Path("runtime/camouhost/real.py")
SYNTHETIC_RUNTIME = Path("runtime/camouhost/main.py")
RUNTIME_ROOT = Path("runtime")
RUNTIME_LOCK = Path("runtime/camouhost/runtime-lock.json")
OPSCTL_SOURCE = Path("tools/opsctl/src")
ADR = Path("docs/adr/ADR-0001-fingerprint-stability-policy.md")
AR10_EVIDENCE = Path("docs/ARCHITECTURE_REBASELINE_V3_AR10.md")
AR10_ACCEPTANCE_EVIDENCE = Path("docs/evidence/2026-08-19-ar10-final-acceptance.json")
RETIRED_RUNTIME_CUTOVER = Path("architecture/runtime-cutover-ar10.json")
LEGACY_EXECUTABLES = {
    "check_mail.py",
    "profile_manager.py",
    "test_fingerprint_consistency.py",
    "tools/cloud_profile_smoke.py",
    "tools/fingerprint_certify.py",
    "tools/profile_browser.py",
}
EXPECTED_LOCK: dict[str, Any] = {
    "browser": {
        "release_commit": "0583c3ec94f5a9df5cb2d09553fbfe80589b6e2d",
        "repository": "daijro/camoufox",
        "version": "152.0.4-beta.28",
    },
    "camouhost_ipc_version": 2,
    "components": {
        "browserforge": "1.2.4",
        "camoufox_python": "0.5.5",
        "playwright": "1.60.0",
    },
    "fingerprint_config_schema": "camoufox-canonical-config-v1",
    "fingerprint_policy_version": "profile-stability-v1",
    "python": "3.12",
    "python_source": {
        "commit": "cd83f7fd2fdf631dfde0c7eb53bd3d30f102ec4a",
        "repository": "daijro/camoufox",
    },
    "runtime_role": "real_camoufox",
    "schema_version": 1,
}
EXPECTED_WINDOWS_DISTRIBUTION_BASE: dict[str, Any] = {
    "architecture": "x86_64",
    "browser": {
        "artifact_sha256": "386fc2f41139685f9a1a9cef0d024bc041d899c315ea538d561171b5b282e57d",
        "artifact_url": "https://github.com/daijro/camoufox/releases/download/v152.0.4-beta.28/camoufox-152.0.4-beta.28-win.x86_64.zip",
        "executable_path": "browser/camoufox.exe",
    },
    "python": {
        "artifact_sha256": "4acbed6dd1c744b0376e3b1cf57ce906f9dc9e95e68824584c8099a63025a3c3",
        "artifact_url": "https://www.python.org/ftp/python/3.12.10/python-3.12.10-embed-amd64.zip",
        "version": "3.12.10",
    },
}
MAX_WINDOWS_PYTHON_PACKAGES = 256
PYPI_FILES_PREFIX = "https://files.pythonhosted.org/packages/"
PACKAGE_NAME_RE = re.compile(r"^[a-z0-9][a-z0-9._-]*$")
SHA256_RE = re.compile(r"^[0-9a-f]{64}$")

NETWORK_IMPORT_PREFIXES = (
    "aiohttp",
    "http.client",
    "httpx",
    "requests",
    "socket",
    "urllib.request",
)
BROWSER_RUNTIME_IMPORT_PREFIXES = ("camoufox", "playwright")
SENSITIVE_ENV_MARKERS = (
    "ACCESS_KEY",
    "API_KEY",
    "AUTH_TOKEN",
    "CLIENT_SECRET",
    "PASSWORD",
    "PRIVATE_KEY",
    "SECRET",
    "TOKEN",
)
PROVIDER_MUTATION_MARKERS = (
    "deploy",
    "delete",
    "d1 execute",
    "kv key put",
    "r2 object put",
    "secret put",
    "secret bulk",
    "versions deploy",
)


class GateError(ValueError):
    pass


def fail(message: str) -> None:
    raise GateError(message)


def read_regular(root: Path, relative: Path) -> str:
    path = root / relative
    if path.is_symlink() or not path.is_file():
        fail(f"required AR-10 file is missing/not regular: {relative.as_posix()}")
    return path.read_text(encoding="utf-8")


def read_regular_bytes(root: Path, relative: Path) -> bytes:
    path = root / relative
    if path.is_symlink() or not path.is_file():
        fail(f"required AR-10 file is missing/not regular: {relative.as_posix()}")
    return path.read_bytes()


def canonical_json_bytes(value: object) -> bytes:
    return (
        json.dumps(value, ensure_ascii=True, separators=(",", ":"), sort_keys=True) + "\n"
    ).encode("utf-8")


def function_node(tree: ast.Module, name: str) -> ast.FunctionDef:
    for node in tree.body:
        if isinstance(node, ast.FunctionDef) and node.name == name:
            return node
    fail(f"real Camouhost function is missing: {name}")


def called_names(node: ast.AST) -> set[str]:
    names: set[str] = set()
    for child in ast.walk(node):
        if not isinstance(child, ast.Call):
            continue
        target = child.func
        if isinstance(target, ast.Name):
            names.add(target.id)
        elif isinstance(target, ast.Attribute):
            names.add(target.attr)
    return names


def imported_modules(tree: ast.AST) -> set[str]:
    imports: set[str] = set()
    for node in ast.walk(tree):
        if isinstance(node, ast.Import):
            imports.update(alias.name for alias in node.names)
        elif isinstance(node, ast.ImportFrom) and node.module:
            imports.add(node.module)
    return imports


def string_literals(tree: ast.AST) -> set[str]:
    return {
        node.value
        for node in ast.walk(tree)
        if isinstance(node, ast.Constant) and isinstance(node.value, str)
    }


def call_name(node: ast.Call) -> str:
    target = node.func
    parts: list[str] = []
    while isinstance(target, ast.Attribute):
        parts.append(target.attr)
        target = target.value
    if isinstance(target, ast.Name):
        parts.append(target.id)
    return ".".join(reversed(parts))


def call_literal_text(node: ast.Call) -> str:
    literals: list[str] = []
    for child in ast.walk(node):
        if isinstance(child, ast.Constant) and isinstance(child.value, str):
            literals.append(child.value)
    return " ".join(literals).lower()


def python_runtime_role(relative: Path) -> str | None:
    if relative == REAL_RUNTIME:
        return "CROSS_LANGUAGE_RUNTIME_ADAPTER"
    if relative == SYNTHETIC_RUNTIME:
        return "TEST_FIXTURE_RUNTIME_STUB"
    return None


def python_runtime_effects(tree: ast.AST) -> set[str]:
    effects: set[str] = set()
    imports = imported_modules(tree)
    if any(
        module == prefix or module.startswith(prefix + ".")
        for module in imports
        for prefix in NETWORK_IMPORT_PREFIXES
    ):
        effects.add("NetworkAccess")
    if any(
        module == prefix or module.startswith(prefix + ".")
        for module in imports
        for prefix in BROWSER_RUNTIME_IMPORT_PREFIXES
    ):
        effects.add("RuntimeExecution")
    if any(module == "subprocess" or module.startswith("subprocess.") for module in imports):
        effects.add("ProcessExecution")

    for node in ast.walk(tree):
        if isinstance(node, ast.Subscript):
            target = node.value
            if (
                isinstance(target, ast.Attribute)
                and isinstance(target.value, ast.Name)
                and target.value.id == "os"
                and target.attr == "environ"
                and isinstance(node.slice, ast.Constant)
                and isinstance(node.slice.value, str)
                and any(marker in node.slice.value.upper() for marker in SENSITIVE_ENV_MARKERS)
            ):
                effects.add("SecretResolve")
            continue
        if not isinstance(node, ast.Call):
            continue
        name = call_name(node)
        if name in {"os.getenv", "os.environ.get"} and any(
            marker in value.upper()
            for value in string_literals(node)
            for marker in SENSITIVE_ENV_MARKERS
        ):
            effects.add("SecretResolve")
        if name.startswith("subprocess."):
            effects.add("ProcessExecution")
            literal = call_literal_text(node)
            if "wrangler" in literal and any(marker in literal for marker in PROVIDER_MUTATION_MARKERS):
                effects.add("DeploymentMutation")
    return effects


def classify_python_runtime_source(relative: Path, source: str) -> tuple[str, set[str]]:
    try:
        tree = ast.parse(source, filename=relative.as_posix())
    except SyntaxError as error:
        fail(f"Python runtime source does not parse: {relative.as_posix()}: {error}")
    role = python_runtime_role(relative)
    effects = python_runtime_effects(tree)
    if role is None:
        fail(f"unclassified Python product-runtime entrypoint: {relative.as_posix()}")
    forbidden = effects.intersection(
        {"DeploymentMutation", "NetworkAccess", "ProcessExecution", "SecretResolve"}
    )
    if forbidden:
        fail(
            f"Python runtime role {role} acquired forbidden direct effects "
            f"{sorted(forbidden)}: {relative.as_posix()}"
        )
    if role == "TEST_FIXTURE_RUNTIME_STUB" and "RuntimeExecution" in effects:
        fail("synthetic Camouhost must never acquire real browser runtime execution")
    return role, effects


def validate_python_runtime_boundary(root: Path) -> None:
    runtime_root = root / RUNTIME_ROOT
    if runtime_root.is_symlink() or not runtime_root.is_dir():
        fail("product runtime root is missing")
    observed: set[Path] = set()
    for path in sorted(runtime_root.rglob("*.py")):
        if path.is_symlink() or not path.is_file():
            fail(f"Python runtime source is not a regular file: {path.relative_to(root)}")
        relative = path.relative_to(root)
        classify_python_runtime_source(relative, path.read_text(encoding="utf-8"))
        observed.add(relative)
    if observed != {REAL_RUNTIME, SYNTHETIC_RUNTIME}:
        fail(
            "supported Python product-runtime surface drifted; "
            f"observed={sorted(path.as_posix() for path in observed)}"
        )


def validate_windows_package_graph(packages: object) -> None:
    if (
        not isinstance(packages, list)
        or not packages
        or len(packages) > MAX_WINDOWS_PYTHON_PACKAGES
    ):
        fail("Windows Python package graph is invalid")
    observed_names: set[str] = set()
    observed_filenames: set[str] = set()
    ordering: list[tuple[str, str, str]] = []
    versions: dict[str, str] = {}
    for row in packages:
        if not isinstance(row, dict) or set(row) != {
            "filename",
            "name",
            "sha256",
            "url",
            "version",
        }:
            fail("Windows Python package graph row shape is invalid")
        filename = row.get("filename")
        name = row.get("name")
        digest = row.get("sha256")
        url = row.get("url")
        version = row.get("version")
        if (
            not isinstance(filename, str)
            or not filename.endswith(".whl")
            or Path(filename).name != filename
            or "\\" in filename
            or ":" in filename
        ):
            fail("Windows Python package graph filename is invalid")
        if not isinstance(name, str) or PACKAGE_NAME_RE.fullmatch(name) is None:
            fail("Windows Python package graph name is invalid")
        if not isinstance(digest, str) or SHA256_RE.fullmatch(digest) is None:
            fail("Windows Python package graph SHA-256 is invalid")
        if (
            not isinstance(url, str)
            or not url.startswith(PYPI_FILES_PREFIX)
            or url.rsplit("/", 1)[-1] != filename
        ):
            fail("Windows Python package graph URL is invalid")
        if (
            not isinstance(version, str)
            or not version
            or len(version) > 64
            or any(character.isspace() for character in version)
        ):
            fail("Windows Python package graph version is invalid")
        if name in observed_names or filename.casefold() in observed_filenames:
            fail("Windows Python package graph contains duplicate identity")
        observed_names.add(name)
        observed_filenames.add(filename.casefold())
        ordering.append((name, version, filename))
        versions[name] = version
    if ordering != sorted(ordering):
        fail("Windows Python package graph is not deterministically ordered")
    expected_roots = {
        "browserforge": EXPECTED_LOCK["components"]["browserforge"],
        "camoufox": EXPECTED_LOCK["components"]["camoufox_python"],
        "playwright": EXPECTED_LOCK["components"]["playwright"],
    }
    if any(versions.get(name) != version for name, version in expected_roots.items()):
        fail("Windows Python package graph root versions drifted from AR-10 identity")


def validate_windows_distribution(distribution: object) -> None:
    if not isinstance(distribution, dict) or set(distribution) != {
        "architecture",
        "browser",
        "python",
    }:
        fail("runtime lock Windows distribution shape drifted")
    if distribution.get("architecture") != EXPECTED_WINDOWS_DISTRIBUTION_BASE["architecture"]:
        fail("runtime lock Windows distribution drifted from the exact S0 delivery identity")
    if distribution.get("browser") != EXPECTED_WINDOWS_DISTRIBUTION_BASE["browser"]:
        fail("runtime lock Windows distribution drifted from the exact S0 delivery identity")
    python = distribution.get("python")
    if not isinstance(python, dict) or set(python) != {
        "artifact_sha256",
        "artifact_url",
        "packages",
        "version",
    }:
        fail("runtime lock Windows Python distribution shape drifted")
    expected_python = EXPECTED_WINDOWS_DISTRIBUTION_BASE["python"]
    projection = {key: python.get(key) for key in expected_python}
    if projection != expected_python:
        fail("runtime lock Windows distribution drifted from the exact S0 delivery identity")
    validate_windows_package_graph(python.get("packages"))


def validate_runtime_lock_value(parsed: object) -> None:
    if not isinstance(parsed, dict):
        fail("runtime lock must be a JSON object")
    expected_keys = set(EXPECTED_LOCK) | {"windows_distribution"}
    if set(parsed) != expected_keys:
        fail("runtime lock shape drifted outside AR-10 + S0 authorities")
    ar10_projection = {key: parsed[key] for key in EXPECTED_LOCK}
    if ar10_projection != EXPECTED_LOCK:
        fail("runtime lock drifted from the exact AR-10 candidate component identity")
    validate_windows_distribution(parsed.get("windows_distribution"))


def validate_runtime_lock(root: Path) -> None:
    raw = read_regular_bytes(root, RUNTIME_LOCK)
    try:
        parsed = json.loads(raw.decode("utf-8"))
    except (UnicodeDecodeError, json.JSONDecodeError) as error:
        fail(f"runtime lock is invalid JSON: {error}")
    validate_runtime_lock_value(parsed)
    if canonical_json_bytes(parsed) != raw:
        fail("runtime lock must use canonical JSON encoding")


def validate_browser_selector_policy(source: str, tree: ast.Module) -> None:
    packaged = function_node(tree, "packaged_windows_browser")
    selector = function_node(tree, "camoufox_browser_selector")
    kwargs = function_node(tree, "camoufox_kwargs")
    materializer = function_node(tree, "materialize_candidate_identity")

    if not {"is_symlink", "is_file", "resolve"}.issubset(called_names(packaged)):
        fail("Windows packaged Camoufox executable must be a regular resolved file")
    if "packaged_windows_browser" not in called_names(selector):
        fail("Windows runtime must select the packaged Camoufox executable")
    if "camoufox_browser_selector" not in called_names(kwargs):
        fail("normal Camoufox launch must use the governed browser selector")
    if "camoufox_browser_selector" not in called_names(materializer):
        fail("candidate identity materialization must use the governed browser selector")

    packaged_source = ast.get_source_segment(source, packaged) or ""
    for marker in (
        'browser.get("executable_path")',
        '"browser/camoufox.exe"',
        "Path(__file__)",
        "runtime_entrypoint.resolve(strict=True).parent.parent",
    ):
        if marker not in packaged_source:
            fail(f"Windows packaged Camoufox resolver lost required invariant: {marker}")

    selector_source = ast.get_source_segment(source, selector) or ""
    for marker in (
        'os.name == "nt"',
        'return {"executable_path": packaged_windows_browser(lock)}',
        'os.name == "posix"',
        'return {"browser": browser["version"]}',
    ):
        if marker not in selector_source:
            fail(f"Camoufox browser selector lost required platform invariant: {marker}")


def validate_real_runtime(root: Path) -> None:
    text = read_regular(root, REAL_RUNTIME)
    try:
        tree = ast.parse(text)
    except SyntaxError as error:
        fail(f"real Camouhost source does not parse: {error}")
    for marker in (
        'CONFIG_NAME = "camoufox-config.json"',
        'USER_DATA_NAME = "user_data"',
        'BRIDGE_LOCK_NAME = ".profile-platform.lock"',
        'EXPECTED_RUNTIME_LOCK_SHA256_ENV',
        'EXPECTED_CONFIG_SHA256_ENV',
        'EXPECTED_PROBE_SHA256_ENV',
        'persistent_context": True',
        '"i_know_what_im_doing": True',
        "fingerprint config digest mismatch",
        "Bridge writer ownership evidence is missing",
        "materialize_candidate_identity",
        "default_addon_exclusions",
        "DefaultAddons.UBO",
        '"exclude_addons": default_addon_exclusions()',
        "exclude_addons=default_addon_exclusions()",
        'emit(f"ready|{active_session}")',
        'emit(f"closed|{active_session}|true")',
    ):
        if marker not in text:
            fail(f"real Camouhost lost required AR-10/S0 runtime invariant: {marker}")

    validate_browser_selector_policy(text, tree)
    ipc = function_node(tree, "run_ipc")
    materializer = function_node(tree, "materialize_candidate_identity")
    launch = function_node(tree, "launch_verified_context")
    kwargs = function_node(tree, "camoufox_kwargs")
    materializer_source = ast.get_source_segment(text, materializer) or ""
    kwargs_source = ast.get_source_segment(text, kwargs) or ""
    if '"geoip"' in kwargs_source or "geoip=" in materializer_source:
        fail("real Camouhost may not acquire autonomous GeoIP/network-identity authority")
    if "launch_options" in called_names(ipc):
        fail("normal active-generation launch may not regenerate BrowserForge identity")
    if "launch_options" not in called_names(materializer):
        fail("candidate identity materializer must explicitly create the initial exact config")
    if "stable_probe_digest" not in called_names(materializer):
        fail("candidate identity materializer must retain aggregate probe evidence")
    if "stable_probe_digest" in called_names(launch):
        fail("normal launch may not use aggregate probe as semantic admission authority")
    for marker in (
        "print(config",
        "print(proxy",
        "print(os.environ",
        "requests.get(",
        "urllib.request",
        "subprocess.run(",
        "pip install",
    ):
        if marker in text:
            fail(f"real Camouhost contains forbidden authority/leak marker: {marker}")


def validate_synthetic_runtime(root: Path) -> None:
    text = read_regular(root, SYNTHETIC_RUNTIME)
    if "Deterministic fake Camouhost process" not in text:
        fail("synthetic Camouhost must remain mechanically identified as fake")
    for marker in ("from camoufox", "import camoufox", "persistent_context"):
        if marker in text:
            fail("synthetic Camouhost must never become real runtime authority")


def production_rust(text: str) -> str:
    return text.split("#[cfg(test)]", 1)[0]


def validate_opsctl(root: Path) -> None:
    source_root = root / OPSCTL_SOURCE
    if source_root.is_symlink() or not source_root.is_dir():
        fail("opsctl source root is missing")
    production = "\n".join(
        production_rust(path.read_text(encoding="utf-8"))
        for path in sorted(source_root.rglob("*.rs"))
        if path.is_file() and not path.is_symlink()
    )
    if "Command::new(" in production or "std::process::Command" in production:
        fail("AR-10 requires zero opsctl child-process spawn authority")
    for marker in ("reqwest::", "ureq::", "std::net::", "TcpStream", "worker::", "cloudflare::"):
        if marker in production:
            fail(f"opsctl acquired forbidden provider/network execution authority: {marker}")


def validate_legacy_retirement(root: Path) -> None:
    present = sorted(relative for relative in LEGACY_EXECUTABLES if (root / relative).exists())
    if present:
        fail(f"AR-6 DELETE_AFTER_SEQUENCE executable retirement is incomplete: {present}")
    active_roots = [root / ".github/workflows", root / "apps", root / "crates", root / "runtime"]
    for scan_root in active_roots:
        if not scan_root.exists():
            continue
        for path in scan_root.rglob("*"):
            if not path.is_file() or path.is_symlink():
                continue
            if path.suffix.lower() not in {".py", ".rs", ".yml", ".yaml", ".toml", ".json", ".md"}:
                continue
            text = path.read_text(encoding="utf-8", errors="strict")
            for legacy in LEGACY_EXECUTABLES:
                if legacy in text:
                    fail(
                        f"active runtime/workflow still references retired executable {legacy}: "
                        f"{path.relative_to(root)}"
                    )


def validate_retired_runtime_authority(root: Path) -> None:
    path = root / RETIRED_RUNTIME_CUTOVER
    if path.exists() or path.is_symlink():
        fail("retired AR-10 runtime-cutover semantic authority was reintroduced")


def validate_acceptance_projection(root: Path) -> None:
    adr = read_regular(root, ADR)
    if "**Статус:** accepted" not in adr:
        fail("ADR-0001 must be accepted only with AR-10 executable evidence")
    for marker in (
        "Profile-Stable",
        "Origin-Deterministic",
        "Network-Bound",
        "Session-Dynamic",
        "## Runtime И Browser Upgrades",
        "Автоматический silent upgrade запрещен.",
    ):
        if marker not in adr:
            fail(f"ADR-0001 lost required policy class/upgrade invariant: {marker}")
    read_regular(root, AR10_EVIDENCE)
    evidence = json.loads(read_regular(root, AR10_ACCEPTANCE_EVIDENCE))
    if evidence.get("kind") != "AR10_FINAL_ACCEPTANCE" or evidence.get("implementation_merge") != "7ab5edf583f541d08ff732624af25881d430d427":
        fail("AR-10 final acceptance evidence identity drifted")
    if evidence.get("applicable_permanent_workflows") != "16/16" or evidence.get("production_mutation") is not False:
        fail("AR-10 final acceptance evidence is incomplete or production-mutating")


def validate_preflight(root: Path) -> None:
    """Validate the supported runtime boundary before runtime parity proof."""
    validate_retired_runtime_authority(root)
    validate_runtime_lock(root)
    validate_real_runtime(root)
    validate_synthetic_runtime(root)
    validate_python_runtime_boundary(root)
    validate_opsctl(root)


def validate_closeout(root: Path) -> None:
    """Validate accepted AR-10 state after successor parity has already been proved."""
    validate_preflight(root)
    validate_legacy_retirement(root)
    validate_acceptance_projection(root)


def expect_runtime_source_failure(relative: Path, source: str, marker: str) -> None:
    try:
        classify_python_runtime_source(relative, source)
    except GateError as error:
        if marker not in str(error):
            fail(f"runtime-boundary negative fixture failed for the wrong reason: {error}")
        return
    fail(f"runtime-boundary negative fixture unexpectedly passed: {relative.as_posix()}")


def expect_runtime_lock_failure(value: object, marker: str) -> None:
    try:
        validate_runtime_lock_value(value)
    except GateError as error:
        if marker not in str(error):
            fail(f"runtime-lock negative fixture failed for the wrong reason: {error}")
        return
    fail("runtime-lock negative fixture unexpectedly passed")


def expect_browser_selector_failure(source: str, marker: str) -> None:
    try:
        validate_browser_selector_policy(source, ast.parse(source))
    except GateError as error:
        if marker not in str(error):
            fail(f"browser-selector negative fixture failed for the wrong reason: {error}")
        return
    fail("browser-selector negative fixture unexpectedly passed")


def package_fixture(name: str, version: str, digit: str) -> dict[str, str]:
    filename = f"{name}-{version}-py3-none-any.whl"
    return {
        "filename": filename,
        "name": name,
        "sha256": digit * 64,
        "url": f"{PYPI_FILES_PREFIX}fixture/{filename}",
        "version": version,
    }


def exact_runtime_lock_fixture() -> dict[str, Any]:
    value = json.loads(json.dumps(EXPECTED_LOCK))
    distribution = json.loads(json.dumps(EXPECTED_WINDOWS_DISTRIBUTION_BASE))
    distribution["python"]["packages"] = [
        package_fixture("browserforge", "1.2.4", "1"),
        package_fixture("camoufox", "0.5.5", "2"),
        package_fixture("playwright", "1.60.0", "3"),
    ]
    value["windows_distribution"] = distribution
    return value


def self_test() -> None:
    exact = exact_runtime_lock_fixture()
    validate_runtime_lock_value(exact)

    mutated_ar10 = exact_runtime_lock_fixture()
    mutated_ar10["components"]["browserforge"] = "latest"
    expect_runtime_lock_failure(mutated_ar10, "AR-10 candidate component identity")

    mutated_windows = exact_runtime_lock_fixture()
    mutated_windows["windows_distribution"]["browser"]["artifact_sha256"] = "0" * 64
    expect_runtime_lock_failure(mutated_windows, "S0 delivery identity")

    mutated_package = exact_runtime_lock_fixture()
    mutated_package["windows_distribution"]["python"]["packages"][0]["sha256"] = "invalid"
    expect_runtime_lock_failure(mutated_package, "package graph")

    unknown = exact_runtime_lock_fixture()
    unknown["compatibility_fallback"] = True
    expect_runtime_lock_failure(unknown, "shape drifted")

    source = "def run_ipc():\n    return launch_options()\n"
    node = function_node(ast.parse(source), "run_ipc")
    if "launch_options" not in called_names(node):
        fail("normal-launch regeneration negative self-test failed")
    if re.fullmatch(r"[0-9a-f]{64}", "0" * 64) is None:
        fail("digest self-test failed")

    predecessor_selector = '''
def packaged_windows_browser(lock):
    browser = lock["windows_distribution"]["browser"]
    executable_path = browser.get("executable_path")
    if executable_path != "browser/camoufox.exe":
        raise ValueError
    runtime_entrypoint = Path(__file__)
    if runtime_entrypoint.is_symlink() or not runtime_entrypoint.is_file():
        raise ValueError
    runtime_root = runtime_entrypoint.resolve(strict=True).parent.parent
    executable = runtime_root.joinpath(*executable_path.split("/"))
    if executable.is_symlink() or not executable.is_file():
        raise ValueError
    return executable.resolve(strict=True)

def camoufox_browser_selector(lock):
    browser = lock["browser"]
    return {"browser": browser["version"]}

def camoufox_kwargs(lock, root, config):
    return camoufox_browser_selector(lock)

def materialize_candidate_identity(root):
    return camoufox_browser_selector({})
'''
    expect_browser_selector_failure(predecessor_selector, "packaged Camoufox executable")

    expect_runtime_source_failure(
        SYNTHETIC_RUNTIME,
        "from camoufox.sync_api import Camoufox\n",
        "synthetic Camouhost",
    )
    expect_runtime_source_failure(
        Path("runtime/rogue.py"),
        "import requests\nrequests.get('https://example.invalid')\n",
        "unclassified Python product-runtime entrypoint",
    )
    expect_runtime_source_failure(
        REAL_RUNTIME,
        "import subprocess\nsubprocess.run(['wrangler', 'r2', 'object', 'put', 'x'])\n",
        "forbidden direct effects",
    )
    expect_runtime_source_failure(
        REAL_RUNTIME,
        "import os\ntoken = os.getenv('CLOUDFLARE_API_TOKEN')\n",
        "forbidden direct effects",
    )

    if LEGACY_EXECUTABLES != {
        "check_mail.py",
        "profile_manager.py",
        "test_fingerprint_consistency.py",
        "tools/cloud_profile_smoke.py",
        "tools/fingerprint_certify.py",
        "tools/profile_browser.py",
    }:
        fail("legacy retirement cohort self-test drifted")


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=ROOT)
    parser.add_argument("--closeout", action="store_true")
    parser.add_argument("--self-test", action="store_true")
    return parser.parse_args()


def main() -> int:
    arguments = parse_args()
    try:
        if arguments.self_test:
            self_test()
            print("AR-10 runtime/Python role-effect policy negative self-test passed.")
        elif arguments.closeout:
            validate_closeout(arguments.root.resolve())
            print("AR-10 real runtime, Python runtime boundary, opsctl and executable-retirement closeout policy passed.")
        else:
            validate_preflight(arguments.root.resolve())
            print("AR-10 successor runtime and Python runtime role/effect preflight policy passed.")
    except (GateError, OSError, UnicodeError, json.JSONDecodeError, ValueError) as error:
        print(f"AR-10 runtime cutover policy failed: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
