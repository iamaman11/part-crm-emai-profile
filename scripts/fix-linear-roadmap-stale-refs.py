#!/usr/bin/env python3
from pathlib import Path

path = Path(__file__).resolve().parents[1] / "docs" / "DEVELOPMENT_PLAN.md"
text = path.read_text(encoding="utf-8")
replacements = {
    "- frontend capability ships incrementally with the backend/query contract that enables it;\n  Phase 7 is completion/polish, not a big-bang frontend start;":
    "- frontend capability ships incrementally with the backend/query contract that enables it;\n  Phase 2H completes/polishes the already incremental UI rather than starting a big-bang frontend;",
    "- long-lead External work (Cloudflare environments, Windows hosts/signing, key recovery,\n  privacy/license/security review) runs as a parallel operational workstream from now onward,\n  while production promotion remains Phase 10 and still requires accepted evidence.":
    "- long-lead External evidence collection (Cloudflare environments, Windows hosts/signing, key\n  recovery, privacy/license/security review) may continue operationally, but implementation order\n  remains linear and production-readiness promotion belongs only to Phase 2J after accepted evidence.",
}
for old, new in replacements.items():
    if text.count(old) != 1:
        raise SystemExit(f"expected one stale roadmap reference, found {text.count(old)}: {old[:80]}")
    text = text.replace(old, new, 1)

for forbidden in (
    "Phase 7 is completion/polish",
    "production promotion remains Phase 10",
    "Phase 1B may proceed in parallel",
    "Phase 1B is eligible to proceed",
    "### Phase 2A — Client aggregate and contact crypto foundation — NEXT",
):
    if forbidden in text:
        raise SystemExit(f"stale execution wording remains: {forbidden}")

path.write_text(text, encoding="utf-8")
print("Stale phase references removed from normative development plan.")
