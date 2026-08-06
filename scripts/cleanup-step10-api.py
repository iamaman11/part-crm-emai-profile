#!/usr/bin/env python3
"""Remove the obsolete rollback error after explicit rollback outcomes landed."""

from pathlib import Path

root = Path(__file__).resolve().parents[1]
lib = root / "crates/certification-domain/src/lib.rs"
text = lib.read_text(encoding="utf-8")

old = "    RollbackUnavailable,\n"
if text.count(old) != 1:
    raise SystemExit(f"rollback error variant: expected one match, found {text.count(old)}")
text = text.replace(old, "", 1)

old = "            Self::RollbackUnavailable => \"rollback release is unavailable\",\n"
if text.count(old) != 1:
    raise SystemExit(f"rollback error display: expected one match, found {text.count(old)}")
text = text.replace(old, "", 1)
lib.write_text(text, encoding="utf-8")

policy = root / "scripts/check-step10-certification.py"
text = policy.read_text(encoding="utf-8")
anchor = '    "matrix_digest={}",\n'
addition = anchor + '    "RollbackUnavailable",\n'
if text.count(anchor) != 1:
    raise SystemExit(f"policy forbidden anchor: expected one match, found {text.count(anchor)}")
text = text.replace(anchor, addition, 1)
policy.write_text(text, encoding="utf-8")
