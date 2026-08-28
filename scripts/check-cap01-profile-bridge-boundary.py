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
SHIPPING_BIN_NAME = "profile-bridge"
SHIPPING_BIN_PATH = "src/main.rs"
SYNTHETIC_BIN_NAME = "profile-bridge-synthetic"
SYNTHETIC_BIN_PATH = "src/bin/profile-bridge-synthetic.rs"
SYNTHETIC_FEATURE = "synthetic-test-bin"

REQUIRED_MAIN_MARKERS = (
    "use bridge_domain::ClaimUri;",
    "ClaimUri::parse(&uri)",
)

FORBIDDEN_CLAIM_ONLY_SUCCESS_MARKERS = (
    'Ok("claim-uri-accepted")',
    'println!("claim-uri-accepted")',
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
            fail(f"shipping ingress marker missing: {marker}")
    for marker in FORBIDDEN_CLAIM_ONLY_SUCCESS_MARKERS:
        if marker in main:
            fail(f"claim-only shipping success is forbidden: {marker}")
    for marker in FORBIDDEN_UNGOVERNED_EFFECT_MARKERS:
        if marker in main:
            fail(f"production entrypoint contains ungoverned effect marker: {marker}")

    manifest = tomllib.loads(manifest_path.read_text(encoding="utf-8"))
    validate_binary_inventory(manifest)
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
        "fn run(uri: String) -> Result<(), ()> {\n"
        "    ClaimUri::parse(&uri).map_err(|_| ())?;\n"
        "    Err(())\n"
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
    raise BoundaryError(f"CAP-01 Profile Bridge boundary self-test failed: {label} unexpectedly passed")


def self_test() -> None:
    with tempfile.TemporaryDirectory(prefix="cap01-profile-bridge-") as temporary:
        root = Path(temporary)
        write_fixture(root)
        validate(root)

        main = root / PRODUCTION_MAIN
        safe_main = main.read_text(encoding="utf-8")

        main.write_text(
            safe_main + "use profile_bridge::operator_flow::ProfileBridgeOperator;\n",
            encoding="utf-8",
        )
        validate(root)
        main.write_text(safe_main, encoding="utf-8")

        main.write_text(
            safe_main + 'fn predecessor() -> Result<&\'static str, ()> { Ok("claim-uri-accepted") }\n',
            encoding="utf-8",
        )
        expect_rejected(root, "claim-only success predecessor")
        main.write_text(safe_main, encoding="utf-8")

        main.write_text(
            safe_main + 'fn effect() { let _ = std::process::Command::new("camouhost"); }\n',
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
                "CAP-01 Profile Bridge keeps one governed shipping ingress; "
                "claim-only success is forbidden and synthetic executors remain production-unreachable."
            )
    except BoundaryError as error:
        print(error)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
