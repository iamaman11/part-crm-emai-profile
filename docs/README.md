# Documentation entrypoint

Use [INDEX.md](INDEX.md) as the documentation authority map.

The short path is:

```text
PRODUCT.md
-> ARCHITECTURE.md
-> APPLICATION_ARCHITECTURE_MANDATORY_REQUIREMENTS.md
-> ARCHITECTURE_REBASELINE_V3_PLAN.md
-> fresh Issue #266
-> exactly one CURRENT stage Issue selected by #266
-> AGENTS.md / CONTRIBUTING.md execution protocol
```

The current implementation program is finite and sequential. Only
[ARCHITECTURE_REBASELINE_V3_PLAN.md](ARCHITECTURE_REBASELINE_V3_PLAN.md) defines its order, stages and
gates; navigation documents deliberately do not copy that sequence.
Only [Issue #266](https://github.com/iamaman11/part-crm-emai-profile/issues/266) selects which stage is
CURRENT. Follow it to exactly one owning stage Issue. Do not copy a moving SHA, workflow count, provider
observation, readiness result or active-stage claim into another document.

For recovery after complete chat/context loss, reference Issue
[#625](https://github.com/iamaman11/part-crm-emai-profile/issues/625) explains the model and bootstrap
path. It is orientation/provenance only and can never select work or authorize effects.

[CAP-INDEX #505](https://github.com/iamaman11/part-crm-emai-profile/issues/505) and CAP-01…CAP-12 Issues
preserve research coverage, findings and accepted decisions. They are provenance/decision inputs, not
runtime or Production authority.

Completed or superseded stage/program Issues and AR/PF/PAS/Functional Closure plans are history. Read
them only when the CURRENT stage or a current natural owner links a specific invariant/evidence
obligation back to that provenance.
