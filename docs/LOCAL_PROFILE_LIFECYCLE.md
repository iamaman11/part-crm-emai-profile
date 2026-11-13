# ProfileBridge — minimal local profile lifecycle

**Intended contract only.** Physical evidence and active defects: [#849](https://github.com/iamaman11/part-crm-emai-profile/issues/849). The previous local-generation/clone/preflight design is [history](https://github.com/iamaman11/part-crm-emai-profile/blob/0d827a73dcad84680d9fe97bb5fa35f77564e470/docs/LOCAL_PROFILE_LIFECYCLE.md), not an Open requirement.

1. **Open:** Rust admits one existing folder under a native exclusive file lock; for a brand-new test profile it creates that folder. The official Camoufox SDK + Playwright open the browser against it. No cloud wait, old generation selection, repeat archive SHA-256, browser preflight, extra process/window manager, or automatic Restore.
2. **Browser:** Camoufox+Playwright own the real browser. Camouhost passes supported launch options and observes official public events; it does not control windows, tabs, navigation, SessionStore or browser termination. A `page.crash` is not a clean close.
3. **Close:** after native user Close, wait for actual Firefox file-lock release before reading profile files. Do not use guessed process/window state as proof.
4. **Sync:** reuse the one existing closed-folder streaming snapshot/encryption/upload/receipt path; never require CRM return. Preserve unsynced local test data and retry using the existing mechanism.
5. **Restore:** explicit, independent cloud recovery verified on a clean second Windows machine; never silently overwrite an ordinary local Open.

No migrations or backward compatibility for discarded test profiles. Windows 559 remains **NO-GO** pending Open timing and VK-page-crash fixes, followed by installed physical acceptance.
