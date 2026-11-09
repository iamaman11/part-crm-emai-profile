# CRM Browser Profiles — responsibility boundary

**Normative intended model, not evidence that Windows 559 conforms.** Current failure and physical acceptance: [#849](https://github.com/iamaman11/part-crm-emai-profile/issues/849). Earlier standalone-product design is preserved in [Git history](https://github.com/iamaman11/part-crm-emai-profile/blob/0d827a73dcad84680d9fe97bb5fa35f77564e470/docs/ARCHITECTURE.md).

| Owner | Single responsibility |
|---|---|
| CRM | UI, customers and profiles, grants/ACL and authorized Open request |
| ProfileBridge (Rust) | One local profile folder, native exclusive access, closed-folder snapshot/encryption, existing Sync/Restore and retry/dirty state |
| Camouhost | Thin, documented adapter to official Camoufox SDK and Playwright; pass settings and receive public Ready/Close/crash events |
| Camoufox + Playwright | Persistent context, browser processes, windows/tabs, navigation, cookies, Firefox SessionStore and native exit |
| Worker / D1 / R2 | Server-side ACL, authoritative cloud committed state and encrypted backups |

## Ordinary Open

Use the **existing** local profile folder directly. Create it only for a new disposable profile. Native file lock admission and a single official SDK persistent-context launch are sufficient. Do not select/generate/copy browser generations, hash giant runtime artifacts again, block on Worker/R2/D1 or implement process/window/tab inspection. Browser Ready is not the same as browser Close; UI acknowledgement must not await the whole session lifetime.

## Native Close → one Sync

User closes the Camoufox window. Playwright reports a supported close event; Rust separately checks native file locks are released. The **existing single** streaming Sync reads the closed local folder, encrypts/upload it and confirms the remote receipt; returning to CRM is not required. If interrupted, keep local dirty data and retry through the existing owner. A page crash is not a browser Close event.

## Physical correctness

Open on installed Windows 559 logged `BROWSER_UNAVAILABLE` after **733339 ms**, while the user reports VK ID login-click tab crashes across profiles. No amount of green CI changes this NO-GO. Source changes need exact official-SDK parity tests and fresh installed-Windows results in #849.
