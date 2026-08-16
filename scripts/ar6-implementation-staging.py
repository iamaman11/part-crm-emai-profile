#!/usr/bin/env python3
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def replace_once(path: str, old: str, new: str) -> None:
    target = ROOT / path
    text = target.read_text(encoding="utf-8")
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"{path}: expected one marker {old!r}, found {count}")
    target.write_text(text.replace(old, new, 1), encoding="utf-8", newline="\n")


# The estate generator must classify itself; unknown future paths remain fail-closed.
replace_once(
    "scripts/python-estate-ar6.py",
    '    if path.startswith("tests/"):\n        return keep(path, "test_or_fixture", "test_only", "tests and fixtures remain legitimate Python")',
    '    if path == "scripts/python-estate-ar6.py":\n'
    '        return keep(path, "estate_generator", "repository_validation_and_inventory_generation", "AR-6 canonical Python estate generator/checker remains legitimate Python")\n'
    '    if path.startswith("tests/"):\n'
    '        return keep(path, "test_or_fixture", "test_only", "tests and fixtures remain legitimate Python")',
)

quality = " .github/workflows/quality-gate.yml".strip()
replace_once(
    quality,
    "          scripts/generate-architecture-inventory.py\n          scripts/cloudflare-deploy-config.py",
    "          scripts/generate-architecture-inventory.py\n          scripts/python-estate-ar6.py\n          scripts/check-opsctl-readonly.py\n          scripts/cloudflare-deploy-config.py",
)
replace_once(
    quality,
    "      - name: Prove architecture inventory drift is detected\n        run: python scripts/generate-architecture-inventory.py --self-test\n\n      - name: Verify canonical Cloudflare deployment configuration",
    "      - name: Prove architecture inventory drift is detected\n        run: python scripts/generate-architecture-inventory.py --self-test\n\n"
    "      - name: Verify AR-6 full Python estate\n"
    "        run: python scripts/python-estate-ar6.py --check\n\n"
    "      - name: Prove AR-6 Python estate drift fails closed\n"
    "        run: python scripts/python-estate-ar6.py --self-test\n\n"
    "      - name: Enforce AR-6 read-only Rust opsctl boundary\n"
    "        run: |\n"
    "          python scripts/check-opsctl-readonly.py\n"
    "          python scripts/check-opsctl-readonly.py --self-test\n"
    "          cargo fmt --manifest-path tools/opsctl/Cargo.toml -- --check\n"
    "          cargo clippy --locked --manifest-path tools/opsctl/Cargo.toml --all-targets -- -D warnings\n"
    "          cargo test --locked --manifest-path tools/opsctl/Cargo.toml\n"
    "          cargo run --locked --quiet --manifest-path tools/opsctl/Cargo.toml -- --root . doctor | python -m json.tool >/dev/null\n"
    "          cargo run --locked --quiet --manifest-path tools/opsctl/Cargo.toml -- --root . status | python -m json.tool >/dev/null\n"
    "          cargo run --locked --quiet --manifest-path tools/opsctl/Cargo.toml -- --root . inventory | python -m json.tool >/dev/null\n\n"
    "      - name: Verify canonical Cloudflare deployment configuration",
)

repo = ".github/workflows/repository-quality-audit-gate.yml"
replace_once(
    repo,
    "          python -m py_compile scripts/generate-architecture-inventory.py\n          python -m py_compile scripts/test-architecture-inventory-negative.py",
    "          python -m py_compile scripts/generate-architecture-inventory.py\n          python -m py_compile scripts/python-estate-ar6.py\n          python -m py_compile scripts/check-opsctl-readonly.py\n          python -m py_compile scripts/test-architecture-inventory-negative.py",
)
replace_once(
    repo,
    "      - name: Prove architecture inventory drift is rejected\n        run: python scripts/test-architecture-inventory-negative.py\n\n      - name: Enforce Phase 1A event/outbox ownership and scope",
    "      - name: Prove architecture inventory drift is rejected\n        run: python scripts/test-architecture-inventory-negative.py\n\n"
    "      - name: Audit AR-6 Python estate and read-only opsctl boundary\n"
    "        run: |\n"
    "          python scripts/python-estate-ar6.py --check\n"
    "          python scripts/python-estate-ar6.py --self-test\n"
    "          python scripts/check-opsctl-readonly.py\n"
    "          python scripts/check-opsctl-readonly.py --self-test\n"
    "          cargo test --locked --manifest-path tools/opsctl/Cargo.toml\n\n"
    "      - name: Enforce Phase 1A event/outbox ownership and scope",
)
