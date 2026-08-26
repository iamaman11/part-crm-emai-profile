#!/usr/bin/env python3
"""Fail closed if Profile Bridge gains an ungoverned production execution ingress."""

from __future__ import annotations

import argparse
import tempfile
import tomllib
from pathlib import Path

PRODUCTION_MAIN = Path("apps/profile-bridge/src/main.rs")
MANIFEST = Path("apps/profile-bridge/Cargo.toml")
AUX_BIN_ROOT = Path("apps/profile-bridge/src/bin")
ALLOWED_AUX_BINS = {"profile-bridge-synthetic.rs"}

REQUIRED_MAIN_MARKERS = (
    "use bridge_domain::ClaimUri;",
    "ClaimUri::parse(&uri)",
    'Ok("claim-uri-accepted")',
)

FORBIDDEN_PRODUCTION_MARKERS = (
    "profile_bridge::",
    "ProfileBridgeOperator",
    "RuntimeSessionOrchestrator",
    "ManagedCamouhostProcess",
    "camouhost_process",
    "operator_flow",
    "browser_execution_domain",
    "runtime_bundle_domain",
    "application_ports",
    "session_domain",
    "tokio::process",
    "std::process::Command",
    "Command::new(",
    ".spawn(",
    "std::net::",
    "reqwest::",
)


class BoundaryError(RuntimeError):
    pass


def fail(message: str) -> None:
    raise BoundaryError(f"CAP-01 Profile Bridge production boundary failed: {message}")


def validate(root: Path) -> None:
    main_path = root / PRODUCTION_MAIN
    manifest_path = root / MANIFEST
    if not main_path.is_file():
        fail(f"missing production entrypoint: {PRODUCTION_MAIN}")
    if main_path.is_symlink():
        fail(f"production entrypoint must not be a symlink: {PRODUCTION_MAIN}")
    if not manifest_path.is_file():
        fail(f"missing manifest: {MANIFEST}")

    main = main_path.read_text(encoding="utf-8").replace("\r\n", "\n")
    for marker in REQUIRED_MAIN_MARKERS:
        if marker not in main:
            fail(f"claim-only entrypoint marker missing: {marker}")
    for marker in FORBIDDEN_PRODUCTION_MARKERS:
        if marker in main:
            fail(f"production entrypoint contains executable-effect marker: {marker}")

    manifest = tomllib.loads(manifest_path.read_text(encoding="utf-8"))
    package = manifest.get("package", {})
    if package.get("default-run") != "profile-bridge":
        fail("package default-run must remain profile-bridge")
    if package.get("autobins", True) is not True:
        fail("Cargo automatic binary discovery changed; boundary inventory must be reviewed")
    if manifest.get("bin"):
        fail("explicit Cargo [[bin]] entries require a new executable-boundary review")
    dependencies = manifest.get("dependencies", {})
    if "capability-policy" in dependencies:
        fail("decorative capability-policy dependency found without a production executor")

    bin_root = root / AUX_BIN_ROOT
    observed_aux = {
        path.relative_to(bin_root).as_posix()
        for path in bin_root.rglob("*.rs")
        if path.is_file()
    } if bin_root.is_dir() else set()
    if observed_aux != ALLOWED_AUX_BINS:
        fail(
            "published auxiliary binary inventory changed: "
            f"expected={sorted(ALLOWED_AUX_BINS)} observed={sorted(observed_aux)}"
        )


def write_fixture(root: Path) -> None:
    main = root / PRODUCTION_MAIN
    main.parent.mkdir(parents=True, exist_ok=True)
    main.write_text(
        "use bridge_domain::ClaimUri;\n"
        "fn run(uri: String) -> Result<&'static str, ()> {\n"
        "    ClaimUri::parse(&uri).map_err(|_| ())?;\n"
        "    Ok(\"claim-uri-accepted\")\n"
        "}\n",
        encoding="utf-8",
    )
    manifest = root / MANIFEST
    manifest.parent.mkdir(parents=True, exist_ok=True)
    manifest.write_text(
        "[package]\n"
        "name = \"profile-bridge\"\n"
        "version = \"0.1.0\"\n"
        "edition = \"2024\"\n"
        "default-run = \"profile-bridge\"\n\n"
        "[dependencies]\n"
        "bridge-domain = \"0.1\"\n",
        encoding="utf-8",
    )
    aux = root / AUX_BIN_ROOT / "profile-bridge-synthetic.rs"
    aux.parent.mkdir(parents=True, exist_ok=True)
    aux.write_text("fn main() {}\n", encoding="utf-8")


def expect_rejected(root: Path, label: str) -> None:
    try:
        validate(root)
    except BoundaryError:
        return
    raise BoundaryError(f"CAP-01 Profile Bridge boundary self-test failed: {label} unexpectedly passed")


def self_test() -> None:
    with tempfile.TemporaryDirectory(prefix="cap01-profile-bridge-") as temporary:
        root = Path(temporary)
        write_fixture(root)
        validate(root)

        main = root / PRODUCTION_MAIN
        safe = main.read_text(encoding="utf-8")
        main.write_text(safe + "use profile_bridge::operator_flow::ProfileBridgeOperator;\n", encoding="utf-8")
        expect_rejected(root, "operator composition")
        main.write_text(safe, encoding="utf-8")

        main.write_text(safe + 'fn effect() { let _ = std::process::Command::new("camouhost"); }\n', encoding="utf-8")
        expect_rejected(root, "direct process effect")
        main.write_text(safe, encoding="utf-8")

        extra = root / AUX_BIN_ROOT / "profile-bridge-live.rs"
        extra.write_text("fn main() {}\n", encoding="utf-8")
        expect_rejected(root, "additional executable")

    print("CAP-01 Profile Bridge production-boundary self-test passed.")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[1])
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    try:
        if args.self_test:
            self_test()
        else:
            validate(args.root.resolve())
            print(
                "CAP-01 Profile Bridge production entrypoint remains claim-only; "
                "no independent Camoufox executor is published."
            )
    except BoundaryError as error:
        print(error)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
