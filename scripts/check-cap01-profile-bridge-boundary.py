#!/usr/bin/env python3
"""Fail closed if Profile Bridge gains an ungoverned production execution ingress."""

from __future__ import annotations

import argparse
import tempfile
import tomllib
from pathlib import Path

PRODUCTION_MAIN = Path("apps/profile-bridge/src/main.rs")
PRODUCTION_COMPOSITION = Path("apps/profile-bridge/src/shipping_composition.rs")
MANIFEST = Path("apps/profile-bridge/Cargo.toml")
AUX_BIN_ROOT = Path("apps/profile-bridge/src/bin")
ALLOWED_AUX_BINS = {"profile-bridge-synthetic.rs"}
SHIPPING_BIN_NAME = "profile-bridge"
SHIPPING_BIN_PATH = "src/main.rs"
SYNTHETIC_BIN_NAME = "profile-bridge-synthetic"
SYNTHETIC_BIN_PATH = "src/bin/profile-bridge-synthetic.rs"
SYNTHETIC_FEATURE = "synthetic-test-bin"

REQUIRED_MAIN_MARKERS = (
    "use bridge_domain::ClaimUri;",
    "use profile_bridge::shipping_composition::run_claim;",
    "ClaimUri::parse(&uri)",
    "run_claim(&claim)",
)

REQUIRED_COMPOSITION_MARKERS = (
    "WindowsSchannelMachineHttp::from_system(",
    "WindowsSignedGenerationObjectGet::from_system(",
    "WindowsSignedGenerationObjectPut::from_system(",
    "ControlPlaneEnrollment::new(",
    "ControlPlaneCoordinator::new(",
    "FilesystemRuntimeBundleSelection::open(",
    "ShippingBrowserLaunchPreflight::new(",
    "ManagedCamouhostProcess::pair(",
    ".close_observer()",
    "ProfileBridgeOperator::new(",
    ".open_authoritative(",
    ".observe_controlled_close(&session_id)",
    ".close(now()?)",
    ".save_retained_successor(",
    ".runtime_timing()",
    ".heartbeat(observed_at)",
    "RuntimeDisplayMode::Headful",
    "ShippingCompositionError::CommittedRecoveryRequired",
    "Err(ShippingCompositionError::UnsupportedPlatform)",
)

FORBIDDEN_CLAIM_ONLY_SUCCESS_MARKERS = (
    'Ok("claim-uri-accepted")',
    'println!("claim-uri-accepted")',
    "CompositionUnavailable",
)

FORBIDDEN_UNGOVERNED_EFFECT_MARKERS = (
    "profile_bridge::Fake",
    "profile-bridge-synthetic",
    "FakeCamouhost",
    "FakeProcessControl",
    "tokio::process",
    "std::process::Command",
    "Command::new(",
    ".spawn(",
    "std::net::",
    "reqwest::",
)

FORBIDDEN_COMPOSITION_MARKERS = (
    "CompositionUnavailable",
    "profile-bridge-synthetic",
    "FakeCamouhost",
    "FakeProcessControl",
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


def validate_binary_inventory(manifest: dict) -> None:
    package = manifest.get("package", {})
    if package.get("default-run") != SHIPPING_BIN_NAME:
        fail("package default-run must remain profile-bridge")
    if package.get("autobins") is not False:
        fail("Cargo automatic binary discovery must remain disabled")

    features = manifest.get("features", {})
    if features.get("default") != []:
        fail("default feature set must remain empty")
    if features.get(SYNTHETIC_FEATURE) != []:
        fail("synthetic-test-bin feature must remain an empty opt-in gate")

    bins = manifest.get("bin", [])
    if not isinstance(bins, list):
        fail("Cargo [[bin]] inventory must be an array")
    observed = {entry.get("name"): entry for entry in bins if isinstance(entry, dict)}
    expected_names = {SHIPPING_BIN_NAME, SYNTHETIC_BIN_NAME}
    if set(observed) != expected_names or len(bins) != len(expected_names):
        fail(
            "explicit Cargo binary inventory changed: "
            f"expected={sorted(expected_names)} observed={sorted(str(name) for name in observed)}"
        )

    shipping = observed[SHIPPING_BIN_NAME]
    if shipping.get("path") != SHIPPING_BIN_PATH:
        fail("shipping profile-bridge binary must point to src/main.rs")
    if "required-features" in shipping:
        fail("shipping profile-bridge binary must not be feature-gated")

    synthetic = observed[SYNTHETIC_BIN_NAME]
    if synthetic.get("path") != SYNTHETIC_BIN_PATH:
        fail("synthetic binary must point to src/bin/profile-bridge-synthetic.rs")
    if synthetic.get("required-features") != [SYNTHETIC_FEATURE]:
        fail("synthetic binary must require only synthetic-test-bin")


def validate(root: Path) -> None:
    main_path = root / PRODUCTION_MAIN
    composition_path = root / PRODUCTION_COMPOSITION
    manifest_path = root / MANIFEST
    if not main_path.is_file():
        fail(f"missing production entrypoint: {PRODUCTION_MAIN}")
    if main_path.is_symlink():
        fail(f"production entrypoint must not be a symlink: {PRODUCTION_MAIN}")
    if not composition_path.is_file():
        fail(f"missing production composition: {PRODUCTION_COMPOSITION}")
    if composition_path.is_symlink():
        fail(f"production composition must not be a symlink: {PRODUCTION_COMPOSITION}")
    if not manifest_path.is_file():
        fail(f"missing manifest: {MANIFEST}")

    main = main_path.read_text(encoding="utf-8").replace("\r\n", "\n")
    for marker in REQUIRED_MAIN_MARKERS:
        if marker not in main:
            fail(f"shipping ingress marker missing: {marker}")
    for marker in FORBIDDEN_CLAIM_ONLY_SUCCESS_MARKERS:
        if marker in main:
            fail(f"claim-only shipping predecessor is forbidden: {marker}")
    for marker in FORBIDDEN_UNGOVERNED_EFFECT_MARKERS:
        if marker in main:
            fail(f"production entrypoint contains ungoverned effect marker: {marker}")

    composition = composition_path.read_text(encoding="utf-8").replace("\r\n", "\n")
    for marker in REQUIRED_COMPOSITION_MARKERS:
        if marker not in composition:
            fail(f"shipping composition marker missing: {marker}")
    for marker in FORBIDDEN_COMPOSITION_MARKERS:
        if marker in composition:
            fail(f"shipping composition contains forbidden alternate/effect marker: {marker}")

    close_observe = composition.find(".observe_controlled_close(&session_id)")
    mutating_close = composition.find(".close(now()?)")
    save = composition.find(".save_retained_successor(")
    if min(close_observe, mutating_close, save) < 0 or not close_observe < mutating_close < save:
        fail("shipping controlled-close witness must precede the sole mutating close and canonical save")

    manifest = tomllib.loads(manifest_path.read_text(encoding="utf-8"))
    validate_binary_inventory(manifest)
    dependencies = manifest.get("dependencies", {})
    if "capability-policy" in dependencies:
        fail("decorative capability-policy dependency found without a production executor")

    bin_root = root / AUX_BIN_ROOT
    observed_aux = (
        {
            path.relative_to(bin_root).as_posix()
            for path in bin_root.rglob("*.rs")
            if path.is_file()
        }
        if bin_root.is_dir()
        else set()
    )
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
        "use profile_bridge::shipping_composition::run_claim;\n"
        "fn run(uri: String) -> Result<(), ()> {\n"
        "    let claim = ClaimUri::parse(&uri).map_err(|_| ())?;\n"
        "    run_claim(&claim).map_err(|_| ())?;\n"
        "    Ok(())\n"
        "}\n",
        encoding="utf-8",
    )
    composition = root / PRODUCTION_COMPOSITION
    composition.write_text(
        "// WindowsSchannelMachineHttp::from_system(\n"
        "// WindowsSignedGenerationObjectGet::from_system(\n"
        "// WindowsSignedGenerationObjectPut::from_system(\n"
        "// ControlPlaneEnrollment::new(\n"
        "// ControlPlaneCoordinator::new(\n"
        "// FilesystemRuntimeBundleSelection::open(\n"
        "// ShippingBrowserLaunchPreflight::new(\n"
        "// ManagedCamouhostProcess::pair(\n"
        "// .close_observer()\n"
        "// ProfileBridgeOperator::new(\n"
        "// .open_authoritative(\n"
        "// .observe_controlled_close(&session_id)\n"
        "// .close(now()?)\n"
        "// .save_retained_successor(\n"
        "// .runtime_timing()\n"
        "// .heartbeat(observed_at)\n"
        "// RuntimeDisplayMode::Headful\n"
        "// ShippingCompositionError::CommittedRecoveryRequired\n"
        "// Err(ShippingCompositionError::UnsupportedPlatform)\n",
        encoding="utf-8",
    )
    manifest = root / MANIFEST
    manifest.parent.mkdir(parents=True, exist_ok=True)
    manifest.write_text(
        "[package]\n"
        "name = \"profile-bridge\"\n"
        "version = \"0.1.0\"\n"
        "edition = \"2024\"\n"
        "default-run = \"profile-bridge\"\n"
        "autobins = false\n\n"
        "[features]\n"
        "default = []\n"
        "synthetic-test-bin = []\n\n"
        "[[bin]]\n"
        "name = \"profile-bridge\"\n"
        "path = \"src/main.rs\"\n\n"
        "[[bin]]\n"
        "name = \"profile-bridge-synthetic\"\n"
        "path = \"src/bin/profile-bridge-synthetic.rs\"\n"
        "required-features = [\"synthetic-test-bin\"]\n\n"
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
    raise BoundaryError(
        f"CAP-01 Profile Bridge boundary self-test failed: {label} unexpectedly passed"
    )


def self_test() -> None:
    with tempfile.TemporaryDirectory(prefix="cap01-profile-bridge-") as temporary:
        root = Path(temporary)
        write_fixture(root)
        validate(root)

        main = root / PRODUCTION_MAIN
        safe_main = main.read_text(encoding="utf-8")
        composition = root / PRODUCTION_COMPOSITION
        safe_composition = composition.read_text(encoding="utf-8")

        main.write_text(
            safe_main + "use profile_bridge::operator_flow::ProfileBridgeOperator;\n",
            encoding="utf-8",
        )
        validate(root)
        main.write_text(safe_main, encoding="utf-8")

        main.write_text(
            safe_main.replace("run_claim(&claim).map_err(|_| ())?;", "let _ = &claim;"),
            encoding="utf-8",
        )
        expect_rejected(root, "claim-only predecessor without shipping composition")
        main.write_text(safe_main, encoding="utf-8")

        main.write_text(
            safe_main
            + 'fn predecessor() -> Result<&\'static str, ()> { Ok("claim-uri-accepted") }\n',
            encoding="utf-8",
        )
        expect_rejected(root, "claim-only success predecessor")
        main.write_text(safe_main, encoding="utf-8")

        main.write_text(
            safe_main
            + 'fn effect() { let _ = std::process::Command::new("camouhost"); }\n',
            encoding="utf-8",
        )
        expect_rejected(root, "direct process effect")
        main.write_text(safe_main, encoding="utf-8")

        main.write_text(
            safe_main + "use profile_bridge::FakeCamouhost;\n",
            encoding="utf-8",
        )
        expect_rejected(root, "synthetic runtime import")
        main.write_text(safe_main, encoding="utf-8")

        composition.write_text(
            safe_composition.replace("// ManagedCamouhostProcess::pair(\n", ""),
            encoding="utf-8",
        )
        expect_rejected(root, "missing real managed Camouhost composition")
        composition.write_text(safe_composition, encoding="utf-8")

        composition.write_text(
            safe_composition.replace("// .open_authoritative(\n", "// .open(\n"),
            encoding="utf-8",
        )
        expect_rejected(root, "shipping local-only open predecessor")
        composition.write_text(safe_composition, encoding="utf-8")

        composition.write_text(
            safe_composition.replace("// .observe_controlled_close(&session_id)\n", ""),
            encoding="utf-8",
        )
        expect_rejected(root, "shipping save without controlled-close witness")
        composition.write_text(safe_composition, encoding="utf-8")

        composition.write_text(
            safe_composition.replace("// .save_retained_successor(\n", ""),
            encoding="utf-8",
        )
        expect_rejected(root, "shipping close without canonical successor save")
        composition.write_text(safe_composition, encoding="utf-8")

        composition.write_text(
            safe_composition + "// FakeCamouhost\n",
            encoding="utf-8",
        )
        expect_rejected(root, "synthetic runtime in shipping composition")
        composition.write_text(safe_composition, encoding="utf-8")

        manifest = root / MANIFEST
        safe_manifest = manifest.read_text(encoding="utf-8")
        manifest.write_text(
            safe_manifest.replace("autobins = false", "autobins = true"),
            encoding="utf-8",
        )
        expect_rejected(root, "automatic binary discovery")
        manifest.write_text(safe_manifest, encoding="utf-8")

        manifest.write_text(
            safe_manifest.replace(
                'required-features = ["synthetic-test-bin"]\n',
                "",
            ),
            encoding="utf-8",
        )
        expect_rejected(root, "ungated synthetic binary")
        manifest.write_text(safe_manifest, encoding="utf-8")

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
                "CAP-01 Profile Bridge keeps one real governed authoritative shipping composition; "
                "controlled close and canonical successor save are mandatory, claim-only/local-only success is forbidden, "
                "and synthetic executors remain production-unreachable."
            )
    except BoundaryError as error:
        print(error)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
