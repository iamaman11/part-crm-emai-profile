# CRM — Browser Profiles

Browser Profiles is one module of **CRM**, not an independent product or another CRM service.

**CURRENT physical testing:** [#266 pointer](https://github.com/iamaman11/part-crm-emai-profile/issues/266) → [#849 Windows 559 P0 issues](https://github.com/iamaman11/part-crm-emai-profile/issues/849). Release 559 has **not** passed physical acceptance: slow Open and Camoufox tab crash at VK ID after login-button click. CI and staging deployment SUCCESS are not product GO.

## First read

- [Authority index](docs/INDEX.md) — current contracts vs historical evidence
- [Product and ownership](docs/PRODUCT.md), [architecture](docs/ARCHITECTURE.md), [local profile lifecycle](docs/LOCAL_PROFILE_LIFECYCLE.md)
- [AGENTS.md](AGENTS.md), [CONTRIBUTING.md](CONTRIBUTING.md) — developer and security guardrails

## Simple ownership

CRM owns UI/business profiles/permissions. Rust ProfileBridge owns one local folder, native file access and closed-folder encryption/Sync/Restore. Camouhost is the thin official Camoufox+Playwright adapter. Camoufox+Playwright own browser contexts, windows, tabs, navigation, cookies, SessionStore and closure. Worker/D1/R2 own cloud admission and committed backups, but ordinary local Open must not wait on them.

There must be **one implementation and one owner per function**. Old stage plans, historical browser controls, migration proposals and completed CI claims are not a current development queue. No backward compatibility or data migration is needed for disposable test profiles.
