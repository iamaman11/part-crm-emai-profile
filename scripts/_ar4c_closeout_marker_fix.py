#!/usr/bin/env python3
"""Temporary AR-4C closeout marker alignment; remove before acceptance."""

from pathlib import Path

path = Path(__file__).resolve().parents[1] / "scripts/_ar3_application_architecture.py"
text = path.read_text(encoding="utf-8")
old = (
    '    AR4C_EVIDENCE: [\n'
    '        "AR-4C Outbound Mail composition extraction",\n'
    '        "OUTBOUND_MAIL_COMPOSITION_EXTRACTION_CANDIDATE",'
)
new = (
    '    AR4C_EVIDENCE: [\n'
    '        "AR-4C Outbound Mail composition extraction",\n'
    '        "OUTBOUND_MAIL_COMPOSITION_EXTRACTION_ACCEPTED",'
)
if text.count(old) != 1:
    raise SystemExit("expected exactly one AR-4C required evidence marker")
path.write_text(text.replace(old, new, 1), encoding="utf-8", newline="\n")
