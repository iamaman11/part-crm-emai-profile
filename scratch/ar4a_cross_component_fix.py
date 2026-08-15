#!/usr/bin/env python3
from pathlib import Path

root = Path(__file__).resolve().parents[1]
path = root / "scripts/test-cross-component-acceptance.py"
text = path.read_text(encoding="utf-8")

old = '''                "validate_mailbox_job_run_version",\n                "CloudMailboxProviderRouter::new(env)",\n            ),\n            "mailbox job application transport",\n        ),\n        (\n            "crates/use-cases-mailboxes/src/mailbox_jobs.rs",'''
new = '''                "validate_mailbox_job_run_version",\n                "mailbox_job_provider(env, actor)?",\n            ),\n            "mailbox job application transport",\n        ),\n        (\n            "apps/control-plane-worker/src/composition.rs",\n            (\n                "pub fn mailbox_job_provider<'a>",\n                "CloudMailboxProviderRouter::new(env)",\n                "with_microsoft_graph_authorization(microsoft_graph_mailbox_authorization(env)?, actor)",\n            ),\n            "mailbox job composition root",\n        ),\n        (\n            "crates/use-cases-mailboxes/src/mailbox_jobs.rs",'''
if text.count(old) != 1:
    raise SystemExit(f"expected exactly one stale mailbox-job acceptance surface, found {text.count(old)}")
text = text.replace(old, new, 1)

old = '''            "D1Database",\n            "MetadataMailboxProviderAdapter",\n            "decide_mailbox_run",\n        ),\n        "mailbox job Worker transport",\n    )'''
new = '''            "D1Database",\n            "MetadataMailboxProviderAdapter",\n            "decide_mailbox_run",\n            "cloudflare_adapters::cloud_mailbox_provider::CloudMailboxProviderRouter",\n            "CloudMailboxProviderRouter::new(",\n        ),\n        "mailbox job Worker transport",\n    )'''
if text.count(old) != 1:
    raise SystemExit(f"expected exactly one mailbox-job forbidden surface block, found {text.count(old)}")
text = text.replace(old, new, 1)

path.write_text(text, encoding="utf-8", newline="\n")
print("Cross-component acceptance now requires the AR-4A composition seam and rejects provider construction in transport.")
