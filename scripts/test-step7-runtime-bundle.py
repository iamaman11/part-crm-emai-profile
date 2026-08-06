#!/usr/bin/env python3
"""Deterministic repository evidence for the Step 7 runtime bundle format."""

from __future__ import annotations

import importlib.util
import json
import os
import tempfile
import zipfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
TOOL_PATH = ROOT / "tools" / "runtime_bundle.py"
SPEC = importlib.util.spec_from_file_location("runtime_bundle", TOOL_PATH)
if SPEC is None or SPEC.loader is None:
    raise RuntimeError("runtime bundle tool could not be loaded")
RUNTIME_BUNDLE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(RUNTIME_BUNDLE)

BundleError = RUNTIME_BUNDLE.BundleError


def write_synthetic_source(root: Path) -> None:
    root.mkdir()
    (root / RUNTIME_BUNDLE.SOURCE_MARKER).write_text(
        RUNTIME_BUNDLE.SOURCE_MARKER_CONTENT, encoding="utf-8"
    )
    (root / "camouhost").mkdir()
    (root / "camouhost" / "main.py").write_text(
        "print('synthetic camouhost')\n", encoding="utf-8", newline="\n"
    )
    (root / "runtime").mkdir()
    (root / "runtime" / "NOTICE.txt").write_text(
        "synthetic runtime fixture\n", encoding="utf-8", newline="\n"
    )


def write_destination(root: Path) -> None:
    root.mkdir()
    (root / RUNTIME_BUNDLE.DESTINATION_MARKER).write_text(
        RUNTIME_BUNDLE.DESTINATION_MARKER_CONTENT, encoding="utf-8"
    )


def expect_bundle_error(operation, expected_fragment: str) -> None:
    try:
        operation()
    except BundleError as error:
        assert expected_fragment in str(error), (expected_fragment, str(error))
    else:
        raise AssertionError(f"expected BundleError containing: {expected_fragment}")


def tamper_payload(source_bundle: Path, target_bundle: Path) -> None:
    with zipfile.ZipFile(source_bundle, "r") as source, zipfile.ZipFile(
        target_bundle, "w", allowZip64=False
    ) as target:
        for info in source.infolist():
            data = source.read(info.filename)
            if info.filename.endswith("runtime/NOTICE.txt"):
                data += b"tampered"
            target.writestr(info, data)


def write_traversal_bundle(path: Path) -> None:
    manifest = {
        "entries": [
            {
                "length": 1,
                "path": "../escape.py",
                "sha256": "a" * 64,
            }
        ],
        "entrypoint": RUNTIME_BUNDLE.ENTRYPOINT,
        "inventory_sha256": "b" * 64,
        "ipc_version": RUNTIME_BUNDLE.IPC_VERSION,
        "platform": RUNTIME_BUNDLE.PLATFORM,
        "python_version": RUNTIME_BUNDLE.PYTHON_VERSION,
        "runtime_version": RUNTIME_BUNDLE.RUNTIME_VERSION,
        "schema_version": RUNTIME_BUNDLE.MANIFEST_SCHEMA_VERSION,
    }
    with zipfile.ZipFile(path, "w", allowZip64=False) as archive:
        archive.writestr(
            RUNTIME_BUNDLE.MANIFEST_NAME,
            RUNTIME_BUNDLE.canonical_json(manifest),
        )
        archive.writestr("payload/../escape.py", b"x")


def main() -> None:
    with tempfile.TemporaryDirectory(prefix="step7-runtime-bundle-") as temporary:
        root = Path(temporary)
        source = root / "source"
        write_synthetic_source(source)

        first = root / "first.runtime-bundle.zip"
        second = root / "second.runtime-bundle.zip"
        first_manifest = RUNTIME_BUNDLE.build_bundle(source, first)
        second_manifest = RUNTIME_BUNDLE.build_bundle(source, second)
        assert first.read_bytes() == second.read_bytes()
        assert first_manifest == second_manifest
        assert len(first_manifest["inventory_sha256"]) == 64

        verified = RUNTIME_BUNDLE.verify_bundle(first)
        assert verified.manifest == first_manifest
        assert [entry.path for entry in verified.entries] == [
            "camouhost/main.py",
            "runtime/NOTICE.txt",
        ]

        destination = root / "destination"
        write_destination(destination)
        RUNTIME_BUNDLE.extract_bundle(first, destination)
        assert (destination / "camouhost" / "main.py").read_bytes() == (
            source / "camouhost" / "main.py"
        ).read_bytes()
        assert not (root / "escape.py").exists()

        modified_source = root / "modified-source"
        write_synthetic_source(modified_source)
        (modified_source / "runtime" / "NOTICE.txt").write_text(
            "changed synthetic runtime fixture\n", encoding="utf-8", newline="\n"
        )
        modified = root / "modified.runtime-bundle.zip"
        modified_manifest = RUNTIME_BUNDLE.build_bundle(modified_source, modified)
        assert modified_manifest["inventory_sha256"] != first_manifest["inventory_sha256"]
        assert modified.read_bytes() != first.read_bytes()

        tampered = root / "tampered.runtime-bundle.zip"
        tamper_payload(first, tampered)
        expect_bundle_error(
            lambda: RUNTIME_BUNDLE.verify_bundle(tampered),
            "payload content does not match",
        )

        traversal = root / "traversal.runtime-bundle.zip"
        write_traversal_bundle(traversal)
        expect_bundle_error(
            lambda: RUNTIME_BUNDLE.verify_bundle(traversal),
            "invalid segment",
        )
        assert not (root / "escape.py").exists()

        collision_source = root / "collision-source"
        write_synthetic_source(collision_source)
        (collision_source / "Camouhost").mkdir()
        (collision_source / "Camouhost" / "Main.py").write_text(
            "print('collision')\n", encoding="utf-8", newline="\n"
        )
        expect_bundle_error(
            lambda: RUNTIME_BUNDLE.build_bundle(
                collision_source, root / "collision.runtime-bundle.zip"
            ),
            "case-colliding",
        )

        symlink_source = root / "symlink-source"
        write_synthetic_source(symlink_source)
        os.symlink(
            symlink_source / "runtime" / "NOTICE.txt",
            symlink_source / "runtime" / "NOTICE.link",
        )
        expect_bundle_error(
            lambda: RUNTIME_BUNDLE.build_bundle(
                symlink_source, root / "symlink.runtime-bundle.zip"
            ),
            "symbolic links",
        )

        unmarked = root / "unmarked"
        unmarked.mkdir()
        expect_bundle_error(
            lambda: RUNTIME_BUNDLE.build_bundle(
                unmarked, root / "unmarked.runtime-bundle.zip"
            ),
            "synthetic runtime root",
        )

        nonempty_destination = root / "nonempty-destination"
        write_destination(nonempty_destination)
        (nonempty_destination / "existing.txt").write_text("occupied", encoding="utf-8")
        expect_bundle_error(
            lambda: RUNTIME_BUNDLE.extract_bundle(first, nonempty_destination),
            "must be empty",
        )

        manifest_bytes = zipfile.ZipFile(first).read(RUNTIME_BUNDLE.MANIFEST_NAME)
        parsed = json.loads(manifest_bytes)
        assert parsed["ipc_version"] == 1
        assert parsed["platform"] == "windows-x86_64"

    print("Repository Step 7 deterministic runtime bundle invariants passed.")


if __name__ == "__main__":
    main()
