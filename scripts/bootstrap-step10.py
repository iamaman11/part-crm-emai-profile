#!/usr/bin/env python3
"""Register the Step 10 pure crate and strengthen its public metadata API."""

from pathlib import Path


def replace_once(text: str, old: str, new: str, label: str) -> str:
    if new in text:
        return text
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"{label}: expected one match, found {count}")
    return text.replace(old, new, 1)


root = Path(__file__).resolve().parents[1]
workspace = root / "Cargo.toml"
text = workspace.read_text(encoding="utf-8")
text = replace_once(
    text,
    '    "crates/bridge-domain",\n',
    '    "crates/bridge-domain",\n    "crates/certification-domain",\n',
    "workspace member",
)
text = replace_once(
    text,
    'bridge-domain = { path = "crates/bridge-domain" }\n',
    'bridge-domain = { path = "crates/bridge-domain" }\ncertification-domain = { path = "crates/certification-domain" }\n',
    "workspace dependency",
)
workspace.write_text(text, encoding="utf-8")

architecture = root / "scripts/check-architecture.py"
text = architecture.read_text(encoding="utf-8")
text = replace_once(
    text,
    '    "runtime-bundle-domain": set(),\n    "encrypted-generation-domain": {\n',
    '    "runtime-bundle-domain": set(),\n    "certification-domain": {\n        "profile-platform-primitives",\n        "sha2",\n    },\n    "encrypted-generation-domain": {\n',
    "architecture allowlist",
)
architecture.write_text(text, encoding="utf-8")

lib = root / "crates/certification-domain/src/lib.rs"
text = lib.read_text(encoding="utf-8")
text = replace_once(
    text,
    'impl ReleaseId {\n    pub fn parse(value: impl Into<String>) -> Result<Self, CertificationError> {\n        parse_name(value.into()).map(Self)\n    }\n}\n',
    'impl ReleaseId {\n    pub fn parse(value: impl Into<String>) -> Result<Self, CertificationError> {\n        parse_name(value.into()).map(Self)\n    }\n\n    #[must_use]\n    pub fn as_str(&self) -> &str {\n        &self.0\n    }\n}\n',
    "release ID getter",
)
text = replace_once(
    text,
    'impl ContentDigest {\n    pub fn new(bytes: [u8; 32]) -> Result<Self, CertificationError> {\n        if bytes.iter().all(|byte| *byte == 0) {\n            return Err(CertificationError::InvalidContentDigest);\n        }\n        Ok(Self(bytes))\n    }\n}\n',
    'impl ContentDigest {\n    pub fn new(bytes: [u8; 32]) -> Result<Self, CertificationError> {\n        if bytes.iter().all(|byte| *byte == 0) {\n            return Err(CertificationError::InvalidContentDigest);\n        }\n        Ok(Self(bytes))\n    }\n\n    #[must_use]\n    pub const fn bytes(&self) -> &[u8; 32] {\n        &self.0\n    }\n}\n',
    "content digest getter",
)
text = replace_once(
    text,
    'impl VerificationEvidenceId {\n    pub fn parse(value: impl Into<String>) -> Result<Self, CertificationError> {\n        parse_name(value.into()).map(Self)\n    }\n}\n',
    'impl VerificationEvidenceId {\n    pub fn parse(value: impl Into<String>) -> Result<Self, CertificationError> {\n        parse_name(value.into()).map(Self)\n    }\n\n    #[must_use]\n    pub fn as_str(&self) -> &str {\n        &self.0\n    }\n}\n',
    "verification evidence getter",
)
text = replace_once(
    text,
    '        Ok(Self {\n            verifier,\n            evidence_id,\n        })\n    }\n}\n\n#[derive(Clone, Debug, Eq, PartialEq)]\npub struct ReleaseCandidate {\n',
    '        Ok(Self {\n            verifier,\n            evidence_id,\n        })\n    }\n\n    #[must_use]\n    pub fn verifier(&self) -> &str {\n        &self.verifier\n    }\n\n    #[must_use]\n    pub const fn evidence_id(&self) -> &VerificationEvidenceId {\n        &self.evidence_id\n    }\n}\n\n#[derive(Clone, Debug, Eq, PartialEq)]\npub struct ReleaseCandidate {\n',
    "preverified evidence getters",
)
text = replace_once(
    text,
    '        Ok(Self {\n            release_id,\n            version,\n            content_digest,\n            verification,\n        })\n    }\n}\n\n#[derive(Clone, Copy, Debug, Eq, PartialEq)]\npub enum UpdateState {\n',
    '        Ok(Self {\n            release_id,\n            version,\n            content_digest,\n            verification,\n        })\n    }\n\n    #[must_use]\n    pub const fn release_id(&self) -> &ReleaseId {\n        &self.release_id\n    }\n\n    #[must_use]\n    pub const fn version(&self) -> u64 {\n        self.version\n    }\n\n    #[must_use]\n    pub const fn content_digest(&self) -> &ContentDigest {\n        &self.content_digest\n    }\n\n    #[must_use]\n    pub const fn verification(&self) -> &PreverifiedSignatureEvidence {\n        &self.verification\n    }\n}\n\n#[derive(Clone, Copy, Debug, Eq, PartialEq)]\npub enum UpdateState {\n',
    "release candidate getters",
)
lib.write_text(text, encoding="utf-8")
