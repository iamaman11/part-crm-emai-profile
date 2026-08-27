# Documentation entrypoint

Use [INDEX.md](INDEX.md) as the documentation authority map.

The short path is:

```text
PRODUCT.md
-> ARCHITECTURE.md
-> APPLICATION_ARCHITECTURE_MANDATORY_REQUIREMENTS.md
-> ARCHITECTURE_REBASELINE_V3_PLAN.md
-> fresh Issue #266 + owning bounded Issue
-> AGENTS.md / CONTRIBUTING.md execution protocol
```

The current implementation program is finite and sequential:

```text
E0 -> E1 -> E2 -> E3 -> E4
-> P1 -> P2 -> P3
-> V1 -> V2
-> R1 -> R2 -> R3
```

Only [ARCHITECTURE_REBASELINE_V3_PLAN.md](ARCHITECTURE_REBASELINE_V3_PLAN.md) defines those stages.
Only [Issue #266](https://github.com/iamaman11/part-crm-emai-profile/issues/266) records which bounded
transaction is live. Do not copy a moving SHA, workflow count, provider observation, readiness result or
active-stage claim into another document.

[CAP-INDEX #505](https://github.com/iamaman11/part-crm-emai-profile/issues/505) and CAP-01…CAP-12 Issues
preserve research coverage, findings and accepted decisions. They are provenance/decision inputs, not
runtime or Production authority.

Completed AR/PF/PAS/Functional Closure plans are history. Read them only when a current owner links a
specific invariant or evidence obligation back to that provenance.
