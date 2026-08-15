# Architecture Re-baseline v3 — AR-2 Runtime Topology + D3 Compatibility

**Document status:** AR-2 DECISION / ACCEPTANCE CANDIDATE  
**Program authority:** `docs/ARCHITECTURE_REBASELINE_V3_PLAN.md`  
**Tracking issue:** #266  
**Decision base:** `5d4a0d4a653539c6ae2aaff7d0ee38d2ecb79dbf`  
**Machine authority:** `architecture/runtime-topology-ar2.json`

## 1. Scope and invariant

AR-2 is a topology decision and compatibility gate, not a provider mutation slice. It classifies relevant Worker, D1, R2, Queue/DLQ, Durable Object, service-binding, schedule and mailbox-provider lanes as `KEEP`, `DEFER` or `DELETE`; proves the `GENERATION_VERIFICATION` decision; and reconciles historical D3/#251 sequencing with Architecture Re-baseline v3.

The binding state remains:

```text
architecture_complete = false
production_core_gate = BLOCKED
production_ready = false
production mutation during AR = forbidden
```

No Cloudflare resource is created, updated, deleted, deployed or promoted by AR-2. A `DELETE` decision is architecture input only: source/Wrangler cleanup belongs to AR-5, historical cleanup to AR-10 where applicable, and production resource mutation is outside the AR program.

## 2. Normalized topology decisions

| Resource/lane | Kind | Decision | Reason / next authority |
|---|---|---:|---|
| control-plane Worker + Static Assets | Worker | KEEP | Canonical web/API composition and SPA delivery |
| mailbox-secret-resolver Worker | Worker | KEEP | Private credential boundary with separate D1 and service-binding reachability |
| catalog D1 | D1 | KEEP | Current business/catalog authority |
| resolver D1 | D1 | KEEP | Deliberate credential-storage isolation |
| profile generation R2 | R2 | KEEP | Immutable profile-generation object boundary |
| ProfileCoordinator | Durable Object | KEEP | Per-profile coordination/fencing owner |
| NotificationHub | Durable Object | KEEP | Realtime notification overlay owner |
| `INTEGRATION_EVENTS` | Queue | KEEP | Real scheduled producer + real queue consumer; D1 delivery state remains authoritative |
| `MAILBOX_JOBS` | Queue | KEEP | Real scheduled producer + real queue consumer with versioned D1 job authority |
| mailbox jobs DLQ | Queue/DLQ | KEEP | Retry-exhaustion boundary; future mutable recovery belongs to PC-3 |
| `GENERATION_VERIFICATION` | Queue | **DELETE** | Producer binding exists, but no Queue consumer/envelope/handler; verification is synchronous |
| `MAILBOX_SECRET_RESOLVER` | service binding | KEEP | Internal control-plane -> resolver boundary |
| control-plane schedule | Cron | KEEP | Dispatches integration events and mailbox jobs |
| resolver schedule | Cron | KEEP | Bounded key reconciliation |
| Gmail API lane | Provider | KEEP | Accepted source topology; activation separately gated |
| IMAP read lane | Provider | KEEP | Accepted standards mailbox topology |
| SMTP send lane | Provider | KEEP | Source topology retained; production outbound activation is PC-4 |
| Microsoft Graph OAuth/read/delta | Provider | KEEP | Accepted Graph read/onboarding lane |
| Microsoft Graph `Mail.Send` | Provider | DEFER | Requires separate implementation/acceptance |
| browser/Bridge mailbox lane | Local runtime | KEEP | Accepted local/browser fallback boundary |

Resolver isolation is intentionally retained. Its private route-free Worker, dedicated D1, independent secret/keyring inventory and service-binding interface are a security boundary, not accidental duplication.

## 3. Final `GENERATION_VERIFICATION` proof

`GENERATION_VERIFICATION` is a legacy producer binding, not an active asynchronous verification lane.

Repository proof at the decision base:

1. `deploy/cloudflare/wrangler.jsonc` declares `GENERATION_VERIFICATION` as a queue producer binding.
2. The same config declares consumers only for `INTEGRATION_EVENTS` and `MAILBOX_JOBS`.
3. `ControlPlaneQueueMessage` contains exactly `IntegrationEvent` and `MailboxJob`; there is no generation-verification envelope.
4. The Worker's queue handler matches only those two variants.
5. `ProfileGenerationVerifyApi` executes synchronously through `profile_generations.rs -> execute_verify_generation`.
6. The remaining binding use is configuration/binding-probe authority, not workload ownership.

Therefore:

```text
GENERATION_VERIFICATION = DELETE
AR-2 = decision/proof only
AR-5 = remove dead source/Wrangler binding while preserving negative proof
PC-1 = do not provision this queue for the production-core release
```

AR-2 intentionally does not delete a Cloudflare Queue or edit the Wrangler binding.

## 4. Queue failure boundaries

`INTEGRATION_EVENTS` keeps durable D1 notification/delivery state as authority; Queue delivery and realtime fanout remain retry/transport layers. `MAILBOX_JOBS` keeps D1 job/binding/version state as authority with bounded Queue retry. Its DLQ is a transport failure boundary, not a second mailbox-domain state machine. The future operator model remains:

```text
inspect envelope
 -> resolve tenant/binding/job/version
 -> load current D1 authority
 -> validate ownership/version/fence
 -> controlled requeue | rerun | retire
 -> metadata-only audit
```

No mutable DLQ operator action is introduced by AR-2.

## 5. D3 / issue #251 compatibility

Historical D3 already produced useful accepted repository assets: resolver isolation, deterministic bootstrap authority, immutable resolver/control-plane releases, exact-artifact verification, protected-environment checks, resolver-before-control-plane ordering, same-bits promotion logic, D1-ledger verification and metadata-only evidence. These are `KEEP` predecessor foundations and become inputs to AR-11 release-set generalization.

What does not survive as forward authority is the old requirement that #251 perform real production promotion before architecture work continues. Architecture Re-baseline v3 requires:

```text
AR-0..AR-17: no production mutation
AR-17: may authorize Production Core gate, still production_ready=false
PC-1: first allowed production provisioning/promotion using AR-11 release-set authority
```

For compatibility the accepted pre-AR-2 promotion implementation is preserved byte-for-byte as `scripts/_mailbox_secret_resolver_promotion_core.py`. The canonical `scripts/mailbox-secret-resolver-promotion.py` becomes a thin AR-2 gate: staging/preproduction behavior delegates to the accepted core, while `production` is rejected before canonical preflight can authorize the environment-backed mutation job. Future PC-1 production promotion must be introduced under the new release-set/gate authority; it must not silently reactivate the old D3 production path.

After AR-2 acceptance, #251 may be closed as `not_planned` for forward execution with a pointer to this decision; its historical evidence remains searchable. #203 stays open as a predecessor blocker lifecycle because accepted exception/freeze gates still consume that lifecycle, but it is not current program authority.

## 6. Mechanical enforcement

The existing `scripts/check-cloudflare-runtime-bindings.py` remains the fitness-gate entrypoint. Its accepted pre-AR-2 implementation is preserved byte-for-byte as `scripts/_cloudflare_runtime_bindings_core.py`; the canonical checker executes that core first and then adds AR-2 fail-closed checks.

It rejects:

- Queue-consumer drift beyond `INTEGRATION_EVENTS` + `MAILBOX_JOBS`;
- any asynchronous `GENERATION_VERIFICATION` envelope/handler/consumer;
- loss of synchronous generation-verification authority;
- loss of resolver private/no-route dedicated-D1 isolation;
- topology-machine drift from the accepted decisions;
- loss/bypass of the canonical D3 preflight;
- loss of the canonical production rejection gate.

Negative fixtures prove generation-verification consumer drift, `DELETE -> KEEP` decision drift, and D3 preflight bypass are rejected.

## 7. Deferred work

AR-2 does not remove the dead binding, delete Cloudflare resources, modernize Durable Object migration syntax, restructure crates, create the AR-3 canonical runtime-resource projection, integrate the opsctl spike, alter OAuth/key concurrency, change D1 compatibility, generalize release sets, or activate mailbox/outbound/automation production capabilities.

AR-3 may project these normalized decisions into the canonical `architecture/inventory.json`; AR-2 does not create a competing inventory.

## 8. Exit criteria

AR-2 is accepted only when one unchanged candidate head proves normalized machine topology agrees with source/Wrangler/runtime consumers; `GENERATION_VERIFICATION=DELETE` is mechanically justified; resolver isolation remains intact; legacy D3 production execution is unavailable; staging/no-rebuild predecessor machinery remains intact; `production_ready=false`, `production_core_gate=BLOCKED`, and `architecture_complete=false`; no provider/Cloudflare/staging/production mutation occurred; all applicable permanent workflows complete successfully; branch is current with `main`; review/threads/Conversation are clean; and exact-head merge interlock is used.

Post-acceptance program state is `accepted_slices=[AR-0, AR-1, AR-2]`; AR-3 is next.
