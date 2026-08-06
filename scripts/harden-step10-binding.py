#!/usr/bin/env python3
"""Bind update evidence, retain device grant history and pin matrix vector."""

from pathlib import Path


def replace_once(text: str, old: str, new: str, label: str) -> str:
    if new in text:
        return text
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"{label}: expected one match, found {count}")
    return text.replace(old, new, 1)


root = Path(__file__).resolve().parents[1]
lib = root / "crates/certification-domain/src/lib.rs"
text = lib.read_text(encoding="utf-8")

text = replace_once(
    text,
    "#[derive(Default)]\npub struct DeviceAuthorizationRegistry {\n    grants: BTreeMap<DeviceGrantKey, DeviceGrantSnapshot>,\n    history_events: u64,\n}",
    "#[derive(Clone, Debug, Eq, PartialEq)]\npub struct DeviceGrantEvent {\n    key: DeviceGrantKey,\n    snapshot: DeviceGrantSnapshot,\n}\n\nimpl DeviceGrantEvent {\n    #[must_use]\n    pub const fn key(&self) -> &DeviceGrantKey {\n        &self.key\n    }\n\n    #[must_use]\n    pub const fn snapshot(&self) -> &DeviceGrantSnapshot {\n        &self.snapshot\n    }\n}\n\n#[derive(Default)]\npub struct DeviceAuthorizationRegistry {\n    grants: BTreeMap<DeviceGrantKey, DeviceGrantSnapshot>,\n    history: Vec<DeviceGrantEvent>,\n}",
    "device grant event journal",
)

text = replace_once(
    text,
    "        self.grants.insert(key, next.clone());\n        self.history_events = self\n            .history_events\n            .checked_add(1)\n            .ok_or(CertificationError::CounterOverflow)?;\n        Ok(next)",
    "        self.grants.insert(key.clone(), next.clone());\n        self.history.push(DeviceGrantEvent {\n            key,\n            snapshot: next.clone(),\n        });\n        Ok(next)",
    "grant history event",
)

text = replace_once(
    text,
    "        self.grants.insert(key.clone(), next.clone());\n        self.history_events = self\n            .history_events\n            .checked_add(1)\n            .ok_or(CertificationError::CounterOverflow)?;\n        Ok(next)",
    "        self.grants.insert(key.clone(), next.clone());\n        self.history.push(DeviceGrantEvent {\n            key: key.clone(),\n            snapshot: next.clone(),\n        });\n        Ok(next)",
    "revoke history event",
)

anchor = "    pub fn authorize_unwrap(\n        &self,\n        key: &DeviceGrantKey,\n        grant_version: u64,\n    ) -> Result<(), CertificationError> {"
addition = "    #[must_use]\n    pub fn history(&self) -> &[DeviceGrantEvent] {\n        &self.history\n    }\n\n" + anchor
text = replace_once(text, anchor, addition, "grant history getter")

text = replace_once(
    text,
    "            self.history_events,",
    "            self.history.len(),",
    "metadata-only history count",
)

text = replace_once(
    text,
    "pub struct PreverifiedSignatureEvidence {\n    verifier: String,\n    evidence_id: VerificationEvidenceId,\n}",
    "pub struct PreverifiedSignatureEvidence {\n    verifier: String,\n    evidence_id: VerificationEvidenceId,\n    release_id: ReleaseId,\n    release_version: u64,\n    content_digest: ContentDigest,\n}",
    "signature evidence binding fields",
)

text = replace_once(
    text,
    "    pub fn new(\n        verifier: impl Into<String>,\n        evidence_id: VerificationEvidenceId,\n    ) -> Result<Self, CertificationError> {\n        let verifier = parse_name(verifier.into())?;\n        Ok(Self {\n            verifier,\n            evidence_id,\n        })\n    }",
    "    pub fn new(\n        verifier: impl Into<String>,\n        evidence_id: VerificationEvidenceId,\n        release_id: ReleaseId,\n        release_version: u64,\n        content_digest: ContentDigest,\n    ) -> Result<Self, CertificationError> {\n        if release_version == 0 {\n            return Err(CertificationError::InvalidReleaseVersion);\n        }\n        let verifier = parse_name(verifier.into())?;\n        Ok(Self {\n            verifier,\n            evidence_id,\n            release_id,\n            release_version,\n            content_digest,\n        })\n    }",
    "signature evidence constructor binding",
)

anchor = "    pub const fn evidence_id(&self) -> &VerificationEvidenceId {\n        &self.evidence_id\n    }\n}"
addition = "    pub const fn evidence_id(&self) -> &VerificationEvidenceId {\n        &self.evidence_id\n    }\n\n    #[must_use]\n    pub const fn release_id(&self) -> &ReleaseId {\n        &self.release_id\n    }\n\n    #[must_use]\n    pub const fn release_version(&self) -> u64 {\n        self.release_version\n    }\n\n    #[must_use]\n    pub const fn content_digest(&self) -> &ContentDigest {\n        &self.content_digest\n    }\n\n    fn approves(\n        &self,\n        release_id: &ReleaseId,\n        release_version: u64,\n        content_digest: &ContentDigest,\n    ) -> bool {\n        self.release_id == *release_id\n            && self.release_version == release_version\n            && self.content_digest == *content_digest\n    }\n}"
text = replace_once(text, anchor, addition, "signature evidence getters and approval")

text = replace_once(
    text,
    "        if version == 0 {\n            return Err(CertificationError::InvalidReleaseVersion);\n        }\n        Ok(Self {",
    "        if version == 0 {\n            return Err(CertificationError::InvalidReleaseVersion);\n        }\n        if !verification.approves(&release_id, version, &content_digest) {\n            return Err(CertificationError::VerificationEvidenceMismatch);\n        }\n        Ok(Self {",
    "release candidate evidence enforcement",
)

text = replace_once(
    text,
    "    RollbackUnavailable,\n}",
    "    RollbackUnavailable,\n    VerificationEvidenceMismatch,\n}",
    "evidence mismatch error variant",
)

text = replace_once(
    text,
    "            Self::RollbackUnavailable => \"rollback release is unavailable\",\n",
    "            Self::RollbackUnavailable => \"rollback release is unavailable\",\n            Self::VerificationEvidenceMismatch => {\n                \"signature verification evidence does not match the release\"\n            }\n",
    "evidence mismatch error display",
)

text = replace_once(
    text,
    "        ReleaseCandidate::new(\n            ReleaseId::parse(id)?,\n            version,\n            ContentDigest::new([byte; 32])?,\n            PreverifiedSignatureEvidence::new(\n                \"synthetic_verifier_01\",\n                VerificationEvidenceId::parse(format!(\"evidence_{id}\"))?,\n            )?,\n        )",
    "        let release_id = ReleaseId::parse(id)?;\n        let content_digest = ContentDigest::new([byte; 32])?;\n        let verification = PreverifiedSignatureEvidence::new(\n            \"synthetic_verifier_01\",\n            VerificationEvidenceId::parse(format!(\"evidence_{id}\"))?,\n            release_id.clone(),\n            version,\n            content_digest.clone(),\n        )?;\n        ReleaseCandidate::new(release_id, version, content_digest, verification)",
    "test candidate helper binding",
)

anchor = "        assert_eq!(forward.matrix_digest(), reverse.matrix_digest());\n        assert_eq!(forward.matrix_digest().to_hex().len(), 64);\n"
addition = "        assert_eq!(forward.matrix_digest(), reverse.matrix_digest());\n        assert_eq!(\n            forward.matrix_digest().to_hex(),\n            \"6667869272890abc935bdfe135c849e03bcb1ba5c93cd76f588dc390fae9f765\"\n        );\n"
text = replace_once(text, anchor, addition, "deterministic matrix vector")

anchor = "        let regranted = registry.grant(first_device.clone(), 2, UnixMillis::new(4))?;\n        assert_eq!(regranted.version(), 3);\n        registry.authorize_unwrap(&first_device, 3)?;\n        Ok(())\n"
addition = "        let regranted = registry.grant(first_device.clone(), 2, UnixMillis::new(4))?;\n        assert_eq!(regranted.version(), 3);\n        registry.authorize_unwrap(&first_device, 3)?;\n        assert_eq!(registry.history().len(), 4);\n        assert_eq!(registry.history()[0].snapshot().version(), 1);\n        assert_eq!(registry.history()[2].snapshot().status(), DeviceGrantStatus::Revoked);\n        assert_eq!(registry.history()[3].snapshot().version(), 3);\n        assert_eq!(registry.history()[3].key(), &first_device);\n        Ok(())\n"
text = replace_once(text, anchor, addition, "grant history regression")

anchor = "    #[test]\n    fn update_rejects_wrong_digest_and_first_install_has_no_rollback()\n"
proof = "    #[test]\n    fn update_evidence_is_bound_to_exact_release_identity()\n    -> Result<(), Box<dyn std::error::Error>> {\n        let release_id = ReleaseId::parse(\"release_01JSTEP10BOUND\")?;\n        let expected_digest = ContentDigest::new([0x66; 32])?;\n        let wrong_digest = ContentDigest::new([0x67; 32])?;\n        let verification = PreverifiedSignatureEvidence::new(\n            \"synthetic_verifier_01\",\n            VerificationEvidenceId::parse(\"evidence_release_01JSTEP10BOUND\")?,\n            release_id.clone(),\n            7,\n            wrong_digest,\n        )?;\n        assert_eq!(\n            ReleaseCandidate::new(release_id, 7, expected_digest, verification),\n            Err(CertificationError::VerificationEvidenceMismatch)\n        );\n        Ok(())\n    }\n\n" + anchor
text = replace_once(text, anchor, proof, "exact update evidence regression")

lib.write_text(text, encoding="utf-8")

docs = root / "docs/CERTIFICATION_MULTI_DEVICE.md"
text = docs.read_text(encoding="utf-8")
text = replace_once(
    text,
    "Revocation advances the\nversion and immediately denies new authorization using either the current\nrevoked version or any stale version.\n",
    "Revocation advances the\nversion and immediately denies new authorization using either the current\nrevoked version or any stale version. Every successful grant, revoke and regrant\nappends an immutable in-memory event snapshot so prior versions remain available\nto repository-local audit tests instead of being reduced to a counter.\n",
    "grant journal documentation",
)
text = replace_once(
    text,
    "- opaque evidence that an external verifier already approved the signature.\n",
    "- opaque evidence that an external verifier already approved the exact release\n  ID, version and content digest.\n",
    "update evidence binding documentation",
)
text = replace_once(
    text,
    "The pure domain does not parse certificates or verify signatures. The\n`PreverifiedSignatureEvidence` name is intentional: production adapters must\nsupply the proof only after trusted signature and policy verification.\n",
    "The pure domain does not parse certificates or verify signatures. The\n`PreverifiedSignatureEvidence` name is intentional: production adapters must\nsupply the proof only after trusted signature and policy verification. The domain\nthen rejects evidence whose approved release ID, version or content digest differs\nfrom the candidate, preventing reuse of one opaque approval for another artifact.\n",
    "evidence binding behavior documentation",
)
docs.write_text(text, encoding="utf-8")

policy = root / "scripts/check-step10-certification.py"
text = policy.read_text(encoding="utf-8")
text = replace_once(
    text,
    "    \"DeviceAuthorizationRegistry\",\n",
    "    \"DeviceAuthorizationRegistry\",\n    \"DeviceGrantEvent\",\n    \"pub fn history(&self)\",\n",
    "grant journal policy fragments",
)
text = replace_once(
    text,
    "    \"PreverifiedSignatureEvidence\",\n",
    "    \"PreverifiedSignatureEvidence\",\n    \"VerificationEvidenceMismatch\",\n    \"fn approves(\",\n",
    "evidence binding policy fragments",
)
policy.write_text(text, encoding="utf-8")

fixture = root / "tests/certification/fixtures/raw-signal-output/crates/certification-domain/src/lib.rs"
text = fixture.read_text(encoding="utf-8")
text = replace_once(
    text,
    "pub struct DeviceAuthorizationRegistry;\n",
    "pub struct DeviceGrantEvent;\npub struct DeviceAuthorizationRegistry;\n",
    "negative fixture grant event",
)
text = replace_once(
    text,
    "    RollbackUnavailable,\n}",
    "    RollbackUnavailable,\n    VerificationEvidenceMismatch,\n}",
    "negative fixture evidence mismatch",
)
text = replace_once(
    text,
    "impl DeviceAuthorizationRegistry {\n    pub fn authorize_unwrap(&self) -> Result<(), Error> {\n        Ok(())\n    }\n}",
    "impl DeviceAuthorizationRegistry {\n    pub fn history(&self) -> &[DeviceGrantEvent] {\n        &[]\n    }\n\n    pub fn authorize_unwrap(&self) -> Result<(), Error> {\n        Ok(())\n    }\n}",
    "negative fixture history getter",
)
text = replace_once(
    text,
    "pub struct PreverifiedSignatureEvidence;\n",
    "pub struct PreverifiedSignatureEvidence;\n\nimpl PreverifiedSignatureEvidence {\n    fn approves(&self) -> bool {\n        true\n    }\n}\n",
    "negative fixture evidence approval",
)
fixture.write_text(text, encoding="utf-8")
