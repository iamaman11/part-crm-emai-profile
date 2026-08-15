# `opsctl` read-only foundation

**Status:** engineering spike only; not accepted architecture authority and not eligible to merge ahead of the Architecture Re-baseline sequencing gates.

This directory proves the repository can host and execute the project-specific Rust `opsctl` without introducing a second mutable infrastructure authority.

## Current commands

- `inventory` — prints the tracked canonical `architecture/inventory.json`.
- `plan` — runs the current canonical architecture-inventory/documentation validator and reports `NO_CHANGE` or a read-only drift plan.
- `doctor` — validates repository markers and requires the current canonical validator to be green.
- `drift` — fails closed if the current canonical inventory/documentation validator detects drift.

There are deliberately **no mutation commands**. The binary does not provision Cloudflare resources, mutate D1/R2/Queues, rotate secrets, call provider APIs, or change customer state.

The spike delegates current inventory consistency to `scripts/generate-architecture-inventory.py --check`. This is intentional: Python remains the accepted validator until a later bounded parity/cutover slice moves a specific semantic owner into Rust. `opsctl` must not silently become a second write authority.

## Build and run

```bash
cargo build --locked --manifest-path tools/opsctl/Cargo.toml
./tools/opsctl/target/debug/opsctl doctor
./tools/opsctl/target/debug/opsctl inventory
./tools/opsctl/target/debug/opsctl plan
./tools/opsctl/target/debug/opsctl drift
```

On Windows use `tools\\opsctl\\target\\debug\\opsctl.exe`.

By default the tool invokes `python` for the existing validator. Set `OPSCTL_PYTHON` when the interpreter is exposed under another executable name.

## Promotion rule

This spike may prove feasibility now, but canonical integration into the root Rust workspace, architecture inventory/tool-disposition authority, and permanent developer/operator path belongs to the accepted AR-6 implementation after preceding Architecture Re-baseline gates are closed.
