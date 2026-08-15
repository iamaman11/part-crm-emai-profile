#!/usr/bin/env python3
from pathlib import Path

root = Path(__file__).resolve().parents[1]
path = root / "scripts/check-phase2h-ui-boundaries.py"
text = path.read_text(encoding="utf-8")

old = '    "worker_mail": Path("apps/control-plane-worker/src/client_mail_query.rs"),\n'
new = (
    '    "worker_mail": Path("apps/control-plane-worker/src/client_mail_query.rs"),\n'
    '    "composition": Path("apps/control-plane-worker/src/composition.rs"),\n'
)
if text.count(old) != 1:
    raise SystemExit(f"expected one Phase 2H worker source entry, found {text.count(old)}")
text = text.replace(old, new, 1)

old = '''    worker = sources["worker_mail"]\n    for marker in (\n        "search_client_mailbox_messages",\n        "get_client_mailbox_message",\n        "D1ClientMailboxEligibilityRepository",\n        "CloudMailboxQueryAdapter",\n        "resolve_active_request_actor",\n    ):\n        if marker not in worker:\n            errors.append(f"Client Mail Worker ingress marker missing: {marker}")\n    for forbidden in ("SELECT ", "INSERT ", "UPDATE ", "DELETE FROM"):\n        if forbidden in worker:\n            errors.append(f"SQL must stay out of Client Mail Worker ingress: {forbidden}")\n\n    eligibility = sources["eligibility"]'''
new = '''    worker = sources["worker_mail"]\n    for marker in (\n        "search_client_mailbox_messages",\n        "get_client_mailbox_message",\n        "query_repository(env)?",\n        "client_mail_eligibility_repository(env)?",\n        "client_mail_query_provider(env, actor.actor(), &client_id)?",\n        "resolve_active_request_actor",\n    ):\n        if marker not in worker:\n            errors.append(f"Client Mail Worker ingress marker missing: {marker}")\n    for forbidden in ("SELECT ", "INSERT ", "UPDATE ", "DELETE FROM"):\n        if forbidden in worker:\n            errors.append(f"SQL must stay out of Client Mail Worker ingress: {forbidden}")\n    for forbidden in (\n        "D1ClientMailboxEligibilityRepository",\n        "D1QueryRepository",\n        "CloudMailboxQueryAdapter",\n    ):\n        if forbidden in worker:\n            errors.append(f"concrete Client Mail adapter must stay out of Worker ingress: {forbidden}")\n\n    composition = sources["composition"]\n    for marker in (\n        "pub fn query_repository",\n        "pub fn client_mail_eligibility_repository",\n        "pub fn client_mail_query_provider<'a>",\n        "D1ClientMailboxEligibilityRepository::new",\n        "D1QueryRepository::new",\n        "CloudMailboxQueryAdapter::new",\n    ):\n        if marker not in composition:\n            errors.append(f"Client Mail composition-root marker missing: {marker}")\n\n    eligibility = sources["eligibility"]'''
if text.count(old) != 1:
    raise SystemExit(f"expected one Phase 2H Client Mail ownership block, found {text.count(old)}")
text = text.replace(old, new, 1)

needle = '''        if not validate_sources(mutated):\n            raise ValueError(f"Phase 2H negative fixture was not rejected: {label}")\n\n\ndef main() -> int:'''
insertion = '''        if not validate_sources(mutated):\n            raise ValueError(f"Phase 2H negative fixture was not rejected: {label}")\n\n    leaked_adapter = dict(sources)\n    leaked_adapter["worker_mail"] = leaked_adapter["worker_mail"] + "\\nCloudMailboxQueryAdapter"\n    if not validate_sources(leaked_adapter):\n        raise ValueError("Phase 2H negative fixture was not rejected: Client Mail adapter leakage")\n\n\ndef main() -> int:'''
if text.count(needle) != 1:
    raise SystemExit(f"expected one Phase 2H self-test insertion point, found {text.count(needle)}")
text = text.replace(needle, insertion, 1)

path.write_text(text, encoding="utf-8", newline="\n")
print("Phase 2H Client Mail policy now follows the AR-4A composition ownership seam.")
