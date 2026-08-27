# Developer Capability Matrix — Historical Tombstone

**Status:** SUPERSEDED / NOT_CURRENT_STATUS_AUTHORITY
**Preserved snapshot:** [protected-main snapshot before D0](https://github.com/iamaman11/part-crm-emai-profile/blob/b8d5e31b263d5682aab99c6f17c9a000a2e5210e/docs/DEVELOPER_CAPABILITY_MATRIX.md)

The former hand-maintained matrix mixed accepted historical implementation evidence, AR/Phase progress,
future targets and mutable readiness claims. It is preserved immutably at the link above and in Git
history, but this path no longer attempts to describe current `main`.

For a current capability question, use:

```text
stable product scope          -> PRODUCT.md
stable architecture/owner     -> ARCHITECTURE.md + bounded contract
source/runtime implementation -> fresh protected main and composition root
tests/checks                   -> natural owner tests + applicable exact-head workflows
active transaction            -> fresh Issue #266 + owning Issue
Production applicability      -> exact CAP-08 Release Candidate and target envelope
```

Do not infer `Composed`, `External`, `current phase`, readiness or Production permission from the
preserved snapshot. A future generated view is allowed only if a named durable consumer requires it and
the view is clearly an output projection of natural owners, never a second status registry.
