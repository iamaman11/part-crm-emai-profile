#!/usr/bin/env python3
"""One-shot AR-4C candidate architecture projection helper.

Temporary branch-only helper; removed before candidate acceptance.
"""

from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def rewrite(relative: str, replacements: list[tuple[str, str]]) -> None:
    path = ROOT / relative
    text = path.read_text(encoding="utf-8")
    for old, new in replacements:
        count = text.count(old)
        if count != 1:
            raise SystemExit(
                f"{relative}: expected exactly one marker, observed {count}: {old[:120]!r}"
            )
        text = text.replace(old, new, 1)
    path.write_text(text, encoding="utf-8", newline="\n")


rewrite(
    "scripts/_ar3_application_architecture.py",
    [
        (
            'AR4B_EVIDENCE = "docs/ARCHITECTURE_REBASELINE_V3_AR4B.md"',
            'AR4B_EVIDENCE = "docs/ARCHITECTURE_REBASELINE_V3_AR4B.md"\nAR4C_EVIDENCE = "docs/ARCHITECTURE_REBASELINE_V3_AR4C.md"',
        ),
        (
            '''    {
        "id": "outbound_mail",
        "transport": "apps/control-plane-worker/src/client_mail_send.rs",
        "application_owner": "crates/use-cases-mailboxes::outbound_mail",
        "ports_owner": "crates/application-ports",
        "composition_seam": "transport currently constructs/selects D1 and concrete Gmail/SMTP providers",
        "status": "TRANSPORT_COMPOSITION_DEBT",
        "remediation_slice": "AR-4C",
    },''',
            '''    {
        "id": "outbound_mail",
        "transport": "apps/control-plane-worker/src/client_mail_send.rs",
        "application_owner": "crates/use-cases-mailboxes::outbound_mail",
        "ports_owner": "crates/application-ports",
        "composition_seam": "apps/control-plane-worker/src/composition.rs::{client_mail_eligibility_repository,query_repository,client_mail_query_provider,outbound_mail_intent_repository,client_mail_outbound_provider}",
        "status": "AR4C_COMPOSITION_EXTRACTION_CANDIDATE",
        "remediation_slice": "AR-4C",
    },''',
        ),
        (
            '''    {
        "id": "outbound_mail_composition",
        "status": "TRANSPORT_COMPOSITION_DEBT",
        "owner_slice": "AR-4C",
        "evidence": [
            "apps/control-plane-worker/src/client_mail_send.rs::D1ClientMailboxEligibilityRepository::new",
            "apps/control-plane-worker/src/client_mail_send.rs::D1OutboundMailIntentRepository::new",
            "apps/control-plane-worker/src/client_mail_send.rs::D1MailboxRepository::new",
            "apps/control-plane-worker/src/client_mail_send.rs::CloudflareGmailOutboundMailProvider::new",
            "apps/control-plane-worker/src/client_mail_send.rs::CloudflareSmtpOutboundMailProvider::new",
        ],
    },''',
            '''    {
        "id": "outbound_mail_composition",
        "status": "AR4C_OUTBOUND_MAIL_COMPOSITION_EXTRACTION_CANDIDATE",
        "owner_slice": "AR-4C",
        "evidence": [
            "apps/control-plane-worker/src/composition/outbound_mail.rs::outbound_mail_intent_repository",
            "apps/control-plane-worker/src/composition/outbound_mail.rs::client_mail_outbound_provider",
            "apps/control-plane-worker/src/composition/outbound_mail.rs::D1OutboundMailIntentRepository::new",
            "apps/control-plane-worker/src/composition/outbound_mail.rs::D1MailboxRepository::new",
            "apps/control-plane-worker/src/composition/outbound_mail.rs::CloudflareGmailOutboundMailProvider::new",
            "apps/control-plane-worker/src/composition/outbound_mail.rs::CloudflareSmtpOutboundMailProvider::new",
        ],
    },''',
        ),
        (
            '''        "pub fn client_mail_query_provider",
        "pub fn notification_operations_repository",''',
            '''        "pub fn client_mail_query_provider",
        "pub use outbound_mail::{",
        "client_mail_outbound_provider",
        "outbound_mail_intent_repository",
        "pub fn notification_operations_repository",''',
        ),
        (
            '''    "apps/control-plane-worker/src/client_mail_send.rs": [
        "D1ClientMailboxEligibilityRepository::new",
        "D1OutboundMailIntentRepository::new",
        "D1MailboxRepository::new",
        "CloudflareGmailOutboundMailProvider::new",
        "CloudflareSmtpOutboundMailProvider::new",
    ],''',
            '''    "apps/control-plane-worker/src/client_mail_send.rs": [
        "client_mail_eligibility_repository",
        "query_repository",
        "client_mail_query_provider",
        "outbound_mail_intent_repository",
        "client_mail_outbound_provider",
    ],
    "apps/control-plane-worker/src/composition/outbound_mail.rs": [
        "pub fn outbound_mail_intent_repository",
        "pub async fn client_mail_outbound_provider",
        "D1OutboundMailIntentRepository::new",
        "D1MailboxRepository::new",
        "CloudflareGmailOutboundMailProvider::new",
        "CloudflareSmtpOutboundMailProvider::new",
    ],''',
        ),
        (
            '''    AR4B_EVIDENCE: [
        "AR-4B Client Mail route ownership",
        "ROUTE_OWNERSHIP_CONSOLIDATION_ACCEPTED",
        "AR-4C",
        "Production Core remains `BLOCKED`",
    ],''',
            '''    AR4B_EVIDENCE: [
        "AR-4B Client Mail route ownership",
        "ROUTE_OWNERSHIP_CONSOLIDATION_ACCEPTED",
        "AR-4C",
        "Production Core remains `BLOCKED`",
    ],
    AR4C_EVIDENCE: [
        "AR-4C Outbound Mail composition extraction",
        "OUTBOUND_MAIL_COMPOSITION_EXTRACTION_CANDIDATE",
        "AR-5",
        "Production Core remains `BLOCKED`",
    ],''',
        ),
        (
            '''    "crates/control-plane-contract/src/routes/clients.rs": [
        "ClientMailSearchApi",
        "ClientMailMessageApi",
        "ClientMailSendApi",
    ],
}''',
            '''    "crates/control-plane-contract/src/routes/clients.rs": [
        "ClientMailSearchApi",
        "ClientMailMessageApi",
        "ClientMailSendApi",
    ],
    "apps/control-plane-worker/src/client_mail_send.rs": [
        "cloudflare_adapters::",
        "D1ClientMailboxEligibilityRepository::new",
        "D1QueryRepository::new",
        "CloudMailboxQueryAdapter::new",
        "D1OutboundMailIntentRepository::new",
        "D1MailboxRepository::new",
        "CloudflareGmailOutboundMailProvider::new",
        "CloudflareSmtpOutboundMailProvider::new",
        "mod provider;",
        "provider::ClientMailProvider",
    ],
}

_FORBIDDEN_PATHS = ["apps/control-plane-worker/src/client_mail_send/provider.rs"]''',
        ),
        (
            '''def validate_source_contract(root: Path) -> None:
    paths = sorted(set(_REQUIRED_SNIPPETS) | set(_FORBIDDEN_SNIPPETS))
    for relative in paths:
        _validate_source_text(relative, _read(root, relative))''',
            '''def validate_source_contract(root: Path) -> None:
    paths = sorted(set(_REQUIRED_SNIPPETS) | set(_FORBIDDEN_SNIPPETS))
    for relative in paths:
        _validate_source_text(relative, _read(root, relative))
    for relative in _FORBIDDEN_PATHS:
        if (root / relative).exists():
            raise SystemExit(
                f"AR-4C source contract drift: forbidden transport composition path exists: {relative}"
            )''',
        ),
        (
            '''            "AR4B_ROUTE_OWNERSHIP_CONSOLIDATION_ACCEPTED",
        ],''',
            '''            "AR4B_ROUTE_OWNERSHIP_CONSOLIDATION_ACCEPTED",
            "AR4C_OUTBOUND_MAIL_COMPOSITION_EXTRACTION_CANDIDATE",
        ],''',
        ),
        (
            '''        "remediation_state": {
            "accepted_through": "AR-4B",
            "status": "ACCEPTED",
            "evidence": AR4B_EVIDENCE,
            "next_required_slice": "AR-4C",
        },''',
            '''        "remediation_state": {
            "accepted_through": "AR-4B",
            "candidate": "AR-4C",
            "candidate_status": "OUTBOUND_MAIL_COMPOSITION_EXTRACTION_CANDIDATE",
            "evidence": AR4C_EVIDENCE,
            "next_after_acceptance": "AR-5",
        },''',
        ),
        (
            '''    remediation = copy.deepcopy(expected)
    remediation["remediation_state"] = {
        "accepted_through": "AR-4A",
        "candidate": "AR-4B",
        "candidate_status": "ROUTE_OWNERSHIP_CONSOLIDATION_CANDIDATE",
        "evidence": AR4B_EVIDENCE,
        "next_after_acceptance": "AR-4C",
    }
    if remediation == expected:
        raise SystemExit("AR-4B negative self-test failed to detect accepted-state rollback to candidate remediation state")''',
            '''    outbound_transport_regression = _read(
        root, "apps/control-plane-worker/src/client_mail_send.rs"
    ) + "\\nD1OutboundMailIntentRepository::new"
    try:
        _validate_source_text(
            "apps/control-plane-worker/src/client_mail_send.rs", outbound_transport_regression
        )
    except SystemExit:
        pass
    else:
        raise SystemExit(
            "AR-4C negative self-test failed to reject transport adapter construction regression"
        )

    missing_outbound_composition = _read(
        root, "apps/control-plane-worker/src/composition/outbound_mail.rs"
    ).replace(
        "pub fn outbound_mail_intent_repository",
        "pub fn missing_outbound_mail_intent_repository",
    )
    try:
        _validate_source_text(
            "apps/control-plane-worker/src/composition/outbound_mail.rs",
            missing_outbound_composition,
        )
    except SystemExit:
        pass
    else:
        raise SystemExit("AR-4C negative self-test failed to reject missing composition seam")

    remediation = copy.deepcopy(expected)
    remediation["remediation_state"] = {
        "accepted_through": "AR-4B",
        "status": "ACCEPTED",
        "evidence": AR4B_EVIDENCE,
        "next_required_slice": "AR-4C",
    }
    if remediation == expected:
        raise SystemExit(
            "AR-4C negative self-test failed to distinguish candidate and accepted remediation state"
        )''',
        ),
        (
            'print("AR-4B accepted Client Mail route ownership negative self-tests passed.")',
            'print("AR-4C Outbound Mail composition candidate negative self-tests passed.")',
        ),
    ],
)

rewrite(
    "scripts/generate-architecture-inventory.py",
    [
        (
            'AR4B_EVIDENCE = "docs/ARCHITECTURE_REBASELINE_V3_AR4B.md"',
            'AR4B_EVIDENCE = "docs/ARCHITECTURE_REBASELINE_V3_AR4B.md"\nAR4C_EVIDENCE = "docs/ARCHITECTURE_REBASELINE_V3_AR4C.md"',
        ),
        (
            '{"path": AR4B_EVIDENCE, "status": "EVIDENCE", "scope": "ar4b_client_mail_route_ownership_accepted"},',
            '{"path": AR4B_EVIDENCE, "status": "EVIDENCE", "scope": "ar4b_client_mail_route_ownership_accepted"},\n    {"path": AR4C_EVIDENCE, "status": "EVIDENCE", "scope": "ar4c_outbound_mail_composition_candidate"},',
        ),
        (
            '"application_architecture_evidence": AR4B_EVIDENCE,',
            '"application_architecture_evidence": AR4B_EVIDENCE,\n            "application_architecture_candidate_evidence": AR4C_EVIDENCE,',
        ),
        (
            'print("Architecture inventory and accepted AR-4B ownership projection are current.")',
            'print("Architecture inventory and AR-4C candidate composition projection are current.")',
        ),
        (
            'print("Architecture inventory accepted AR-4B negative self-test passed.")',
            'print("Architecture inventory AR-4C candidate negative self-test passed.")',
        ),
    ],
)

print("AR-4C candidate architecture sources updated; regenerate canonical inventory next.")
