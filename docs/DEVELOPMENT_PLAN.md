# CRM Browser Profiles — developer navigation

This file is not a roadmap. [#266](https://github.com/iamaman11/part-crm-emai-profile/issues/266) selects one current issue, presently [#849](https://github.com/iamaman11/part-crm-emai-profile/issues/849) for Windows 559 slow Open and VK ID tab crash.

## The one working user path

CRM permissions → one local folder → official Camoufox+Playwright persistent browser → native user Close → existing ProfileBridge closed-folder streaming Sync → verified cloud receipt → explicit Restore only when requested.

## How to fix defects

1. Use fresh protected `main`, exact installed sequence, actual Windows logs and the #849 reproduction matrix.
2. Compare official SDK + Playwright with the exact application configuration on disposable profiles. Measure the time to **first window/Ready**, not just time until the browser later closes.
3. Isolate the proven existing owner, then make the smallest source change; do not add services, daemons, new generation-selection, process/window inspection or parallel Sync.
4. Run targeted positive/negative regressions and exact-head CI. Release and install only after the change passes. Run real browser login/close/sync/retry/restore acceptance on Windows before GO.

Earlier development sequencing is [Git history](https://github.com/iamaman11/part-crm-emai-profile/blob/0d827a73dcad84680d9fe97bb5fa35f77564e470/docs/DEVELOPMENT_PLAN.md) and is not a second plan.
