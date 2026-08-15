#!/usr/bin/env python3
from pathlib import Path

root = Path(__file__).resolve().parents[1]
path = root / "scripts/check-mailbox-job-worker-application-boundary.py"
text = path.read_text(encoding="utf-8")

old = '''FORBIDDEN_JOB_TRANSPORT_TOKENS = (\n    "cloudflare_adapters::d1_",\n    "D1MailboxRepository",\n    "D1IdempotencyRepository",\n    "CreateMailboxJobMutation",\n    "RunMailboxJobMutation",\n    "MutationEnvelope",\n    "MetadataMailboxProviderAdapter",\n    "decide_mailbox_run",\n    "D1Database",\n)'''
new = '''FORBIDDEN_JOB_TRANSPORT_TOKENS = (\n    "cloudflare_adapters::d1_",\n    "cloudflare_adapters::cloud_mailbox_provider::CloudMailboxProviderRouter",\n    "CloudMailboxProviderRouter::new(",\n    "D1MailboxRepository",\n    "D1IdempotencyRepository",\n    "CreateMailboxJobMutation",\n    "RunMailboxJobMutation",\n    "MutationEnvelope",\n    "MetadataMailboxProviderAdapter",\n    "decide_mailbox_run",\n    "D1Database",\n)'''
if text.count(old) != 1:
    raise SystemExit(f"expected one mailbox job forbidden-token block, found {text.count(old)}")
text = text.replace(old, new, 1)

old = '''    "validate_create_mailbox_job_request",\n    "validate_mailbox_job_run_version",\n    "CloudMailboxProviderRouter::new(env)",\n)'''
new = '''    "validate_create_mailbox_job_request",\n    "validate_mailbox_job_run_version",\n    "mailbox_job_provider(env, actor)?",\n)'''
if text.count(old) != 1:
    raise SystemExit(f"expected one stale mailbox job required constructor, found {text.count(old)}")
text = text.replace(old, new, 1)

old = '''    if (\n        "D1MailboxJobApplicationRepository" not in composition\n        or "mailbox_job_application" not in composition\n        or "env.d1(D1_CATALOG_BINDING)?" not in composition\n    ):\n        errors.append("Worker composition root must construct the D1 mailbox job application adapter")'''
new = '''    if (\n        "D1MailboxJobApplicationRepository" not in composition\n        or "mailbox_job_application" not in composition\n        or "env.d1(D1_CATALOG_BINDING)?" not in composition\n    ):\n        errors.append("Worker composition root must construct the D1 mailbox job application adapter")\n    for token in (\n        "pub fn mailbox_job_provider<'a>",\n        "CloudMailboxProviderRouter::new(env)",\n        "with_microsoft_graph_authorization(microsoft_graph_mailbox_authorization(env)?, actor)",\n    ):\n        if token not in composition:\n            errors.append(f"Worker composition root missing mailbox job provider token `{token}`")'''
if text.count(old) != 1:
    raise SystemExit(f"expected one mailbox job composition validation block, found {text.count(old)}")
text = text.replace(old, new, 1)

old = '''        "fn route() { execute_create_mailbox_job(); get_mailbox_job(); execute_run_mailbox_job(); "\n        "mailbox_job_application(env); validate_create_mailbox_job_request(); "\n        "validate_mailbox_job_run_version(); CloudMailboxProviderRouter::new(env); }\\n",'''
new = '''        "CloudMailboxProviderRouter::new(env); "\n        "fn route() { execute_create_mailbox_job(); get_mailbox_job(); execute_run_mailbox_job(); "\n        "mailbox_job_application(env); validate_create_mailbox_job_request(); "\n        "validate_mailbox_job_run_version(); mailbox_job_provider(env, actor)?; }\\n",'''
if text.count(old) != 1:
    raise SystemExit(f"expected one mailbox job self-test transport fixture, found {text.count(old)}")
text = text.replace(old, new, 1)

old = '''    (worker / "composition.rs").write_text(\n        "D1MailboxJobApplicationRepository mailbox_job_application env.d1(D1_CATALOG_BINDING)?\\n",\n        encoding="utf-8",\n    )'''
new = '''    (worker / "composition.rs").write_text(\n        "D1MailboxJobApplicationRepository mailbox_job_application env.d1(D1_CATALOG_BINDING)?\\n"\n        "pub fn mailbox_job_provider<'a> {}\\n"\n        "CloudMailboxProviderRouter::new(env)\\n"\n        "with_microsoft_graph_authorization(microsoft_graph_mailbox_authorization(env)?, actor)\\n",\n        encoding="utf-8",\n    )'''
if text.count(old) != 1:
    raise SystemExit(f"expected one mailbox job self-test composition fixture, found {text.count(old)}")
text = text.replace(old, new, 1)

old = '''            direct_d1 = any("D1MailboxRepository" in error for error in errors)\n            provider = any("MetadataMailboxProviderAdapter" in error for error in errors)\n            if not (direct_d1 and provider):'''
new = '''            direct_d1 = any("D1MailboxRepository" in error for error in errors)\n            legacy_provider = any("MetadataMailboxProviderAdapter" in error for error in errors)\n            concrete_provider = any("CloudMailboxProviderRouter" in error for error in errors)\n            if not (direct_d1 and legacy_provider and concrete_provider):'''
if text.count(old) != 1:
    raise SystemExit(f"expected one mailbox job self-test assertion block, found {text.count(old)}")
text = text.replace(old, new, 1)

text = text.replace(
    'print("negative direct-D1/provider mailbox job fixture rejected as expected")',
    'print("negative direct-D1/provider mailbox job fixture rejected as expected")',
    1,
)

path.write_text(text, encoding="utf-8", newline="\n")
print("Mailbox job Worker boundary now requires AR-4A composition ownership and rejects concrete provider leakage.")
