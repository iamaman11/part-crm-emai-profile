#!/usr/bin/env python3
"""Bind update health results to exact release identity and fail closed."""

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

anchor = "    pub const fn new(\n        tenant_id: TenantId,\n        profile_id: ProfileId,\n        generation_id: GenerationId,\n        device_id: DeviceId,\n    ) -> Self {\n        Self {\n            tenant_id,\n            profile_id,\n            generation_id,\n            device_id,\n        }\n    }\n}"
addition = "    pub const fn new(\n        tenant_id: TenantId,\n        profile_id: ProfileId,\n        generation_id: GenerationId,\n        device_id: DeviceId,\n    ) -> Self {\n        Self {\n            tenant_id,\n            profile_id,\n            generation_id,\n            device_id,\n        }\n    }\n\n    #[must_use]\n    pub const fn tenant_id(&self) -> &TenantId {\n        &self.tenant_id\n    }\n\n    #[must_use]\n    pub const fn profile_id(&self) -> &ProfileId {\n        &self.profile_id\n    }\n\n    #[must_use]\n    pub const fn generation_id(&self) -> &GenerationId {\n        &self.generation_id\n    }\n\n    #[must_use]\n    pub const fn device_id(&self) -> &DeviceId {\n        &self.device_id\n    }\n}"
text = replace_once(text, anchor, addition, "device grant key audit getters")

anchor = "    pub const fn status(&self) -> DeviceGrantStatus {\n        self.status\n    }\n}"
addition = "    pub const fn status(&self) -> DeviceGrantStatus {\n        self.status\n    }\n\n    #[must_use]\n    pub const fn changed_at(&self) -> UnixMillis {\n        self.changed_at\n    }\n}"
text = replace_once(text, anchor, addition, "device grant timestamp getter")

anchor = "    pub const fn verification(&self) -> &PreverifiedSignatureEvidence {\n        &self.verification\n    }\n}"
addition = "    pub const fn verification(&self) -> &PreverifiedSignatureEvidence {\n        &self.verification\n    }\n\n    fn matches_identity(\n        &self,\n        release_id: &ReleaseId,\n        version: u64,\n        content_digest: &ContentDigest,\n    ) -> bool {\n        self.release_id == *release_id\n            && self.version == version\n            && self.content_digest == *content_digest\n    }\n}"
text = replace_once(text, anchor, addition, "release health identity matcher")

text = replace_once(
    text,
    "#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]\npub enum UpdateState {",
    "#[derive(Clone, Copy, Debug, Eq, PartialEq)]\npub enum RollbackOutcome {\n    Restored(u64),\n    NoPreviousRelease,\n}\n\n#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]\npub enum UpdateState {",
    "rollback outcome type",
)

text = replace_once(
    text,
    "    Healthy,\n    RolledBack,\n}",
    "    Healthy,\n    RolledBack,\n    Failed,\n}",
    "failed update state",
)

text = replace_once(
    text,
    "            Self::Healthy => \"healthy\",\n            Self::RolledBack => \"rolled_back\",\n",
    "            Self::Healthy => \"healthy\",\n            Self::RolledBack => \"rolled_back\",\n            Self::Failed => \"failed\",\n",
    "failed update state renderer",
)

text = replace_once(
    text,
    "    pub fn confirm_health(&mut self, release_id: &ReleaseId) -> Result<(), CertificationError> {\n        if self.state != UpdateState::AwaitingHealth {\n            return Err(CertificationError::InvalidUpdateTransition);\n        }\n        let active = self\n            .active\n            .as_ref()\n            .ok_or(CertificationError::MissingActiveRelease)?;\n        if &active.release_id != release_id {\n            return Err(CertificationError::ReleaseIdentityMismatch);\n        }\n        self.state = UpdateState::Healthy;\n        Ok(())\n    }",
    "    pub fn confirm_health(\n        &mut self,\n        release_id: &ReleaseId,\n        version: u64,\n        content_digest: &ContentDigest,\n    ) -> Result<(), CertificationError> {\n        if self.state != UpdateState::AwaitingHealth {\n            return Err(CertificationError::InvalidUpdateTransition);\n        }\n        let active = self\n            .active\n            .as_ref()\n            .ok_or(CertificationError::MissingActiveRelease)?;\n        if !active.matches_identity(release_id, version, content_digest) {\n            return Err(CertificationError::ReleaseIdentityMismatch);\n        }\n        self.state = UpdateState::Healthy;\n        Ok(())\n    }",
    "exact health confirmation identity",
)

text = replace_once(
    text,
    "    pub fn fail_health_and_rollback(\n        &mut self,\n        release_id: &ReleaseId,\n    ) -> Result<u64, CertificationError> {\n        if self.state != UpdateState::AwaitingHealth {\n            return Err(CertificationError::InvalidUpdateTransition);\n        }\n        let active = self\n            .active\n            .as_ref()\n            .ok_or(CertificationError::MissingActiveRelease)?;\n        if &active.release_id != release_id {\n            return Err(CertificationError::ReleaseIdentityMismatch);\n        }\n        let previous = self\n            .previous\n            .take()\n            .ok_or(CertificationError::RollbackUnavailable)?;\n        let restored_version = previous.version;\n        self.active = Some(previous);\n        self.state = UpdateState::RolledBack;\n        Ok(restored_version)\n    }",
    "    pub fn fail_health_and_rollback(\n        &mut self,\n        release_id: &ReleaseId,\n        version: u64,\n        content_digest: &ContentDigest,\n    ) -> Result<RollbackOutcome, CertificationError> {\n        if self.state != UpdateState::AwaitingHealth {\n            return Err(CertificationError::InvalidUpdateTransition);\n        }\n        let active = self\n            .active\n            .as_ref()\n            .ok_or(CertificationError::MissingActiveRelease)?;\n        if !active.matches_identity(release_id, version, content_digest) {\n            return Err(CertificationError::ReleaseIdentityMismatch);\n        }\n        if let Some(previous) = self.previous.take() {\n            let restored_version = previous.version;\n            self.active = Some(previous);\n            self.state = UpdateState::RolledBack;\n            return Ok(RollbackOutcome::Restored(restored_version));\n        }\n        self.active = None;\n        self.state = UpdateState::Failed;\n        Ok(RollbackOutcome::NoPreviousRelease)\n    }",
    "fail-closed health rollback",
)

text = replace_once(
    text,
    "    RollbackUnavailable,\n    VerificationEvidenceMismatch,\n",
    "    VerificationEvidenceMismatch,\n",
    "remove obsolete rollback error",
)

text = replace_once(
    text,
    "            Self::RollbackUnavailable => \"rollback release is unavailable\",\n            Self::VerificationEvidenceMismatch => {\n",
    "            Self::VerificationEvidenceMismatch => {\n",
    "remove obsolete rollback error display",
)

text = replace_once(
    text,
    "        controller.confirm_health(&ReleaseId::parse(\"release_01JSTEP10A\")?)?;",
    "        controller.confirm_health(\n            &ReleaseId::parse(\"release_01JSTEP10A\")?,\n            1,\n            &first_digest,\n        )?;",
    "exact first health confirmation",
)

text = replace_once(
    text,
    "        assert_eq!(\n            controller.fail_health_and_rollback(&ReleaseId::parse(\"release_01JSTEP10B\")?)?,\n            1\n        );",
    "        assert_eq!(\n            controller.fail_health_and_rollback(\n                &ReleaseId::parse(\"release_01JSTEP10B\")?,\n                2,\n                &second_digest,\n            )?,\n            RollbackOutcome::Restored(1)\n        );",
    "exact failed health identity",
)

anchor = "        assert_eq!(controller.activate_staged()?, 2);\n        assert_eq!(\n            controller.fail_health_and_rollback("
addition = "        assert_eq!(controller.activate_staged()?, 2);\n        assert_eq!(\n            controller.confirm_health(\n                &ReleaseId::parse(\"release_01JSTEP10B\")?,\n                1,\n                &second_digest,\n            ),\n            Err(CertificationError::ReleaseIdentityMismatch)\n        );\n        assert_eq!(\n            controller.fail_health_and_rollback("
text = replace_once(text, anchor, addition, "stale health signal regression")

text = replace_once(
    text,
    "        assert_eq!(\n            controller.fail_health_and_rollback(&ReleaseId::parse(\"release_01JSTEP10C\")?),\n            Err(CertificationError::RollbackUnavailable)\n        );\n        Ok(())",
    "        assert_eq!(\n            controller.fail_health_and_rollback(\n                &ReleaseId::parse(\"release_01JSTEP10C\")?,\n                1,\n                &ContentDigest::new([0x33; 32])?,\n            )?,\n            RollbackOutcome::NoPreviousRelease\n        );\n        assert_eq!(controller.state(), UpdateState::Failed);\n        assert_eq!(controller.active_version(), None);\n        Ok(())",
    "first install fail-closed regression",
)

anchor = "        assert_eq!(registry.history()[3].key(), &first_device);\n"
addition = "        assert_eq!(registry.history()[3].key(), &first_device);\n        assert_eq!(\n            registry.history()[3].key().device_id(),\n            &DeviceId::parse(\"device_01JSTEP10A\")?\n        );\n        assert_eq!(registry.history()[3].snapshot().changed_at(), UnixMillis::new(4));\n"
text = replace_once(text, anchor, addition, "auditable grant event proof")

lib.write_text(text, encoding="utf-8")

docs = root / "docs/CERTIFICATION_MULTI_DEVICE.md"
text = docs.read_text(encoding="utf-8")
text = replace_once(
    text,
    "The pure domain does not parse certificates or verify signatures. The\n`PreverifiedSignatureEvidence` name is intentional: production adapters must\nsupply the proof only after trusted signature and policy verification. The domain\nthen rejects evidence whose approved release ID, version or content digest differs\nfrom the candidate, preventing reuse of one opaque approval for another artifact.\n",
    "The pure domain does not parse certificates or verify signatures. The\n`PreverifiedSignatureEvidence` name is intentional: production adapters must\nsupply the proof only after trusted signature and policy verification. The domain\nthen rejects evidence whose approved release ID, version or content digest differs\nfrom the candidate, preventing reuse of one opaque approval for another artifact.\nHealth confirmation and failure signals are bound to the same exact release ID,\nversion and digest, so a stale signal cannot approve a newer artifact that reused\nan opaque release ID.\n",
    "health identity documentation",
)
text = replace_once(
    text,
    "Failed health confirmation restores the previous approved release. A first\ninstallation has no rollback target and fails closed rather than inventing one.\nA failed higher version cannot be replayed after rollback.\n",
    "Failed health confirmation restores the previous approved release. A first\ninstallation has no rollback target, removes the failed candidate from the active\nslot and enters an explicit `FAILED` state rather than remaining ambiguously\n`AWAITING_HEALTH`. A failed higher version cannot be replayed after rollback.\n",
    "first install failure documentation",
)
docs.write_text(text, encoding="utf-8")

policy = root / "scripts/check-step10-certification.py"
text = policy.read_text(encoding="utf-8")
text = replace_once(
    text,
    "    \"AwaitingHealth\",\n    \"fail_health_and_rollback\",\n    \"RollbackUnavailable\",\n",
    "    \"AwaitingHealth\",\n    \"UpdateState::Failed\",\n    \"RollbackOutcome\",\n    \"matches_identity\",\n    \"fail_health_and_rollback\",\n",
    "health lifecycle policy fragments",
)
policy.write_text(text, encoding="utf-8")

fixture = root / "tests/certification/fixtures/raw-signal-output/crates/certification-domain/src/lib.rs"
text = fixture.read_text(encoding="utf-8")
text = replace_once(
    text,
    "pub enum UpdateState {\n    AwaitingHealth,\n}",
    "pub enum UpdateState {\n    AwaitingHealth,\n    Failed,\n}\n\npub enum RollbackOutcome {\n    NoPreviousRelease,\n}",
    "negative fixture failed update state",
)
text = replace_once(
    text,
    "    RollbackUnavailable,\n    VerificationEvidenceMismatch,\n",
    "    VerificationEvidenceMismatch,\n",
    "negative fixture obsolete rollback error",
)
text = replace_once(
    text,
    "impl UpdateController {\n    pub fn fail_health_and_rollback(&self) -> Result<(), Error> {\n        Ok(())\n    }\n}",
    "impl UpdateController {\n    fn matches_identity(&self) -> bool {\n        true\n    }\n\n    pub fn fail_health_and_rollback(&self) -> Result<RollbackOutcome, Error> {\n        Ok(RollbackOutcome::NoPreviousRelease)\n    }\n}",
    "negative fixture exact health boundary",
)
fixture.write_text(text, encoding="utf-8")
