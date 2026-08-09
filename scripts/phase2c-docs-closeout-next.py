#!/usr/bin/env python3
from pathlib import Path

path = Path("docs/DEVELOPMENT_PLAN.md")
text = path.read_text(encoding="utf-8")

old = """Phase 2A client-domain split + use-cases-clients + contact protection foundation  ACCEPTED
Phase 2B encrypted contact persistence + client lifecycle commands                 NEXT
Phase 2C merge/assignment/projections + feature-owned routes + Client Registry UI
Phase 2D use-cases-query + CQRS read models + global/client-mail query contracts
"""
new = """Phase 2A client-domain split + use-cases-clients + contact protection foundation  ACCEPTED
Phase 2B encrypted contact persistence + client lifecycle commands                 ACCEPTED
Phase 2C merge/assignment/projections + feature-owned routes + Client Registry UI  ACCEPTED
Phase 2D use-cases-query + CQRS read models + global/client-mail query contracts    NEXT
"""
if text.count(old) != 1:
    raise SystemExit(f"sequential phase block: expected one match, found {text.count(old)}")
text = text.replace(old, new, 1)

heading = "## 19. Immediate Next Action"
if text.count(heading) != 1:
    raise SystemExit(f"Immediate Next Action heading: expected one match, found {text.count(heading)}")
prefix, _ = text.split(heading, 1)
section = """## 19. Immediate Next Action

Start **Phase 2D — CQRS read models, global search and client-mail query contract** from the accepted
Phase 2C closeout. Create the bounded Phase 2D issue/branch/PR before implementation and keep A8 as
the owning architecture obligation.

Execute Phase 2D inward-first in this exact order:

```text
use-cases-query independent application context
  -> capability-owned read-model ports and stable projections
  -> D1 read projections/indexes for supported predicates
  -> authorization-before-projection/provider-fetch sequencing
  -> bounded global client/profile/member/mailbox metadata search
  -> Phase 2B exact-contact HMAC lookup reuse only
  -> provider-neutral SearchClientMailboxMessages/GetClientMailboxMessage contracts
  -> deterministic fake cloud/Bridge query adapters and synthetic full-body proof
  -> stable cursor pagination + cost/query-plan/index evidence
  -> generated public contracts + incremental Client -> Mail UI
  -> permanent Phase 2D positive/negative query-security gates
  -> native + Workers-WASM + frontend proof
  -> exact-head 12/12 acceptance
  -> guarded merge + docs closeout
  -> only then Phase 2E
```

Do not advance Phase 2E or later slices while Phase 2D is unaccepted. Fuzzy/prefix PII search remains
out of scope without a separate accepted privacy/security ADR. External CRM remains future-only after
the standalone completion gate.
"""
text = prefix + section
path.write_text(text, encoding="utf-8")
