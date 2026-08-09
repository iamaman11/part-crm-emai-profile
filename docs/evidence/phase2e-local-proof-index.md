# Phase 2E deterministic proof index

This index names the repository-local proof surfaces for Phase 2E. They are deterministic acceptance evidence for architecture, lifecycle, Queue coordination, privacy, and provider adapter behavior; they are deliberately not a substitute for real Gmail API or IMAP External evidence.

| Concern | Deterministic proof surface |
| --- | --- |
| Inner/outer dependency ownership | `scripts/check-phase2e-mailbox-boundaries.py`, `scripts/check-architecture.py` |
| Accepted Phase 2D authorization -> eligibility -> provider ordering | `scripts/check-phase2d-query-boundaries.py`, `scripts/check-phase2e-mailbox-boundaries.py`, `crates/use-cases-query/src/mail.rs` tests |
| Provider-neutral lifecycle and retry decisions | `crates/mailbox-domain/src/job.rs`, `crates/use-cases-mailboxes/src/mailbox_jobs.rs` tests |
| Canonical D1 lifecycle writes and governance | `migrations/d1/0016_mailbox_cloud_lane.sql`, `scripts/test-mailbox-vertical-slice.py`, D1 migration replay job |
| At-least-once Queue coordination and duplicate fencing | `migrations/d1/0017_mailbox_queue_coordination.sql`, `crates/cloudflare-adapters/src/d1_mailbox_scheduling.rs`, `scripts/test-mailbox-vertical-slice.py` |
| Queue wire-format separation | `crates/cloudflare-adapters/src/control_plane_queue.rs` tests and `mailbox_job_queue.rs` tests |
| Opaque credential resolution | `crates/cloudflare-adapters/src/cloud_mailbox_secrets.rs`, Phase 2E boundary checker, Worker binding probe |
| Gmail scheduled adapter | `crates/cloudflare-adapters/src/gmail_mailbox.rs` tests plus Worker WASM compile |
| IMAP scheduled adapter and TLS/session hardening | `crates/cloudflare-adapters/src/imap_session.rs`, `imap_mailbox.rs` tests plus Worker WASM compile |
| Gmail Phase 2D query adapter | `crates/cloudflare-adapters/src/gmail_mail_query.rs` tests plus Worker WASM compile |
| IMAP Phase 2D query adapter | `crates/cloudflare-adapters/src/imap_query.rs` tests plus Worker WASM compile |
| Message-content and credential privacy | Phase 2D + Phase 2E boundary checkers, metadata-only D1 schema tests, Queue envelope tests |
| Deployment Queue/DLQ/service-binding contract | `deploy/cloudflare/wrangler.example.toml`, Worker binding probe, Phase 2E boundary checker |

Real-provider acceptance evidence requirements are defined separately in `docs/evidence/phase2e-cloud-mailbox.md`. Until those environment-dependent records exist for both Gmail API and IMAP on the acceptance candidate source revision, Phase 2E must not be represented as externally verified or production-ready.
