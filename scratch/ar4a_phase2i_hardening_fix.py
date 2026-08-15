#!/usr/bin/env python3
from pathlib import Path

root = Path(__file__).resolve().parents[1]
path = root / "scripts/check-phase2i-hardening.py"
text = path.read_text(encoding="utf-8")

replacements = [
    (
        '    "worker_mail": Path("apps/control-plane-worker/src/client_mail_query.rs"),\n',
        '    "worker_mail": Path("apps/control-plane-worker/src/client_mail_query.rs"),\n'
        '    "composition": Path("apps/control-plane-worker/src/composition.rs"),\n',
    ),
    (
        '''                "search_client_mailbox_messages",\n                "get_client_mailbox_message",\n                "D1ClientMailboxEligibilityRepository",\n            ),\n            "Client Mail authenticated application ingress",\n        )\n    )\n    for forbidden in ("SELECT ", "INSERT ", "UPDATE ", "DELETE FROM"):\n        if forbidden in worker_mail:\n            errors.append(f"Client Mail Worker ingress contains forbidden direct SQL: {forbidden}")\n\n    realtime = sources["realtime"]''',
        '''                "search_client_mailbox_messages",\n                "get_client_mailbox_message",\n                "query_repository(env)?",\n                "client_mail_eligibility_repository(env)?",\n                "client_mail_query_provider(env, actor.actor(), &client_id)?",\n            ),\n            "Client Mail authenticated application ingress",\n        )\n    )\n    for forbidden in ("SELECT ", "INSERT ", "UPDATE ", "DELETE FROM"):\n        if forbidden in worker_mail:\n            errors.append(f"Client Mail Worker ingress contains forbidden direct SQL: {forbidden}")\n    for forbidden in (\n        "D1ClientMailboxEligibilityRepository",\n        "D1QueryRepository",\n        "CloudMailboxQueryAdapter",\n    ):\n        if forbidden in worker_mail:\n            errors.append(\n                f"Client Mail Worker ingress contains forbidden concrete adapter ownership: {forbidden}"\n            )\n\n    composition = sources["composition"]\n    errors.extend(\n        require_all(\n            composition,\n            (\n                "pub fn query_repository",\n                "pub fn client_mail_eligibility_repository",\n                "pub fn client_mail_query_provider<'a>",\n                "D1ClientMailboxEligibilityRepository::new",\n                "D1QueryRepository::new",\n                "CloudMailboxQueryAdapter::new",\n            ),\n            "Client Mail composition root",\n        )\n    )\n\n    realtime = sources["realtime"]''',
    ),
    (
        '''    source_fixtures = [\n        ("authorization bypass", "query", "if !authorize(actor, authorization, QueryCapability::Clients).await?", "if false"),\n        ("realtime authority", "realtime", "invalidateQueries", "setQueryData"),\n        ("Bridge busy fail-open", "bridge", "OperatorFlowError::Busy", "OperatorFlowError::Stage"),\n    ]''',
        '''    source_fixtures = [\n        ("authorization bypass", "query", "if !authorize(actor, authorization, QueryCapability::Clients).await?", "if false"),\n        ("realtime authority", "realtime", "invalidateQueries", "setQueryData"),\n        ("Bridge busy fail-open", "bridge", "OperatorFlowError::Busy", "OperatorFlowError::Stage"),\n    ]''',
    ),
]

for old, new in replacements:
    if text.count(old) != 1:
        raise SystemExit(f"expected exactly one Phase 2I source marker; found {text.count(old)} for {old[:80]!r}")
    text = text.replace(old, new, 1)

needle = '''        if not validate_sources(mutated):\n            raise ValueError(f"Phase 2I source negative fixture was not rejected: {label}")\n\n\ndef main() -> int:'''
insertion = '''        if not validate_sources(mutated):\n            raise ValueError(f"Phase 2I source negative fixture was not rejected: {label}")\n\n    leaked_adapter = dict(sources)\n    leaked_adapter["worker_mail"] = (\n        leaked_adapter["worker_mail"] + "\\nD1ClientMailboxEligibilityRepository"\n    )\n    if not validate_sources(leaked_adapter):\n        raise ValueError(\n            "Phase 2I source negative fixture was not rejected: Client Mail concrete adapter leakage"\n        )\n\n\ndef main() -> int:'''
if text.count(needle) != 1:
    raise SystemExit(f"expected exactly one Phase 2I self-test insertion point, found {text.count(needle)}")
text = text.replace(needle, insertion, 1)

path.write_text(text, encoding="utf-8", newline="\n")
print("Phase 2I hardening policy now follows the AR-4A Client Mail composition ownership seam.")
