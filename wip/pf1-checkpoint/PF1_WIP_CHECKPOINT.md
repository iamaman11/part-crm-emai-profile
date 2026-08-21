# PF-1 WIP checkpoint

Status: **WIP / DO NOT MERGE**.

Saved from the 2026-08-22 implementation session for issue #430.
Base main: `d034783824bd3b8bb4771e0a5574ee12790c9429`.
Target branch: `codex/pf1-opsctl-architecture-inventory`.

This checkpoint contains the in-progress PF-1 candidate under `/tmp/pf1`:
- typed `tools/opsctl/src/architecture/{model,authorities,inventory,mod}.rs`;
- CLI/lib/repository integration draft;
- Cargo serde pin/lock draft;
- shared digest re-export draft;
- static architecture-inventory authority draft;
- operator-contract bounded projection-write draft;
- opsctl governance checker draft;
- PF-1 positive/negative integration test draft;
- helper scripts used to assemble the draft.

Important: this candidate was **not compiled or accepted** before checkpointing. Continue by extracting the archive, comparing against live main, restoring/factoring `tools/opsctl/src/canonical.rs` from the current `release/digest.rs` primitive, fixing the known test field mismatch (`current_architecture_slice` vs `current_slice`), then hosted compile/CI convergence before any predecessor retirement.

Do not start PF-2/AR-12 from this checkpoint. PF-1 must be accepted first.
