use core::fmt;
use profile_platform_primitives::{ActorId, DeviceId, TenantId, UnixMillis};

pub const DEVICE_PROOF_NONCE_BYTES: usize = 32;
const P256_SPKI_DER_BYTES: usize = 91;
const P256_SPKI_PREFIX: [u8; 27] = [
    0x30, 0x59, 0x30, 0x13, 0x06, 0x07, 0x2a, 0x86, 0x48, 0xce, 0x3d, 0x02, 0x01, 0x06, 0x08, 0x2a,
    0x86, 0x48, 0xce, 0x3d, 0x03, 0x01, 0x07, 0x03, 0x42, 0x00, 0x04,
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DevicePublicKeyAlgorithm {
    EcdsaP256Sha256,
}

#[derive(Clone, Eq, PartialEq)]
pub struct DevicePublicKey {
    algorithm: DevicePublicKeyAlgorithm,
    spki_der: Vec<u8>,
}

impl DevicePublicKey {
    /// Accepts the canonical DER transport shape for an uncompressed P-256 SubjectPublicKeyInfo.
    ///
    /// This validates the exact AlgorithmIdentifier (`id-ecPublicKey` + `prime256v1`) and the
    /// uncompressed 65-byte EC point transport shape. The cryptographic adapter that imports the
    /// key MUST still reject points that are not valid members of the P-256 curve.
    pub fn p256_spki_der(spki_der: Vec<u8>) -> Result<Self, DevicePairingError> {
        if !is_canonical_p256_spki(&spki_der) {
            return Err(DevicePairingError::InvalidPublicKey);
        }
        Ok(Self {
            algorithm: DevicePublicKeyAlgorithm::EcdsaP256Sha256,
            spki_der,
        })
    }

    #[must_use]
    pub const fn algorithm(&self) -> DevicePublicKeyAlgorithm {
        self.algorithm
    }

    #[must_use]
    pub fn spki_der(&self) -> &[u8] {
        &self.spki_der
    }
}

impl fmt::Debug for DevicePublicKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("DevicePublicKey")
            .field("algorithm", &self.algorithm)
            .field("spki_der_bytes", &self.spki_der.len())
            .finish()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DevicePairingTransaction {
    tenant_id: TenantId,
    device_id: DeviceId,
    public_key: DevicePublicKey,
    issued_at: UnixMillis,
    expires_at: UnixMillis,
    authorized_actor_id: Option<ActorId>,
    consumed_at: Option<UnixMillis>,
}

impl DevicePairingTransaction {
    pub fn issue(
        tenant_id: TenantId,
        device_id: DeviceId,
        public_key: DevicePublicKey,
        issued_at: UnixMillis,
        expires_at: UnixMillis,
    ) -> Result<Self, DevicePairingError> {
        if expires_at <= issued_at {
            return Err(DevicePairingError::InvalidTimeline);
        }
        Ok(Self {
            tenant_id,
            device_id,
            public_key,
            issued_at,
            expires_at,
            authorized_actor_id: None,
            consumed_at: None,
        })
    }

    #[must_use]
    pub const fn tenant_id(&self) -> &TenantId {
        &self.tenant_id
    }

    #[must_use]
    pub const fn device_id(&self) -> &DeviceId {
        &self.device_id
    }

    #[must_use]
    pub const fn public_key(&self) -> &DevicePublicKey {
        &self.public_key
    }

    #[must_use]
    pub const fn issued_at(&self) -> UnixMillis {
        self.issued_at
    }

    #[must_use]
    pub const fn expires_at(&self) -> UnixMillis {
        self.expires_at
    }

    #[must_use]
    pub const fn authorized_actor_id(&self) -> Option<&ActorId> {
        self.authorized_actor_id.as_ref()
    }

    #[must_use]
    pub const fn consumed_at(&self) -> Option<UnixMillis> {
        self.consumed_at
    }

    pub fn authorize(
        &mut self,
        actor_id: ActorId,
        now: UnixMillis,
    ) -> Result<(), DevicePairingError> {
        self.require_live(now)?;
        match &self.authorized_actor_id {
            Some(existing) if existing == &actor_id => Ok(()),
            Some(_) => Err(DevicePairingError::BindingMismatch),
            None => {
                self.authorized_actor_id = Some(actor_id);
                Ok(())
            }
        }
    }

    pub fn complete(
        &mut self,
        actor_id: &ActorId,
        device_id: &DeviceId,
        public_key: &DevicePublicKey,
        current_user_auth_epoch: u64,
        now: UnixMillis,
    ) -> Result<RegisteredDeviceCredential, DevicePairingError> {
        self.require_live(now)?;
        if self.authorized_actor_id.as_ref() != Some(actor_id) {
            return Err(DevicePairingError::NotAuthorized);
        }
        if &self.device_id != device_id || &self.public_key != public_key {
            return Err(DevicePairingError::BindingMismatch);
        }
        let credential = RegisteredDeviceCredential::issue(
            self.tenant_id.clone(),
            actor_id.clone(),
            self.device_id.clone(),
            self.public_key.clone(),
            current_user_auth_epoch,
        )?;
        self.consumed_at = Some(now);
        Ok(credential)
    }

    fn require_live(&self, now: UnixMillis) -> Result<(), DevicePairingError> {
        if self.consumed_at.is_some() {
            return Err(DevicePairingError::ReplayRejected);
        }
        if now >= self.expires_at {
            return Err(DevicePairingError::Expired);
        }
        if now < self.issued_at {
            return Err(DevicePairingError::InvalidTimeline);
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RegisteredDeviceCredential {
    tenant_id: TenantId,
    actor_id: ActorId,
    device_id: DeviceId,
    public_key: DevicePublicKey,
    auth_epoch_at_registration: u64,
    enabled: bool,
}

impl RegisteredDeviceCredential {
    pub fn issue(
        tenant_id: TenantId,
        actor_id: ActorId,
        device_id: DeviceId,
        public_key: DevicePublicKey,
        auth_epoch_at_registration: u64,
    ) -> Result<Self, DevicePairingError> {
        if auth_epoch_at_registration == 0 {
            return Err(DevicePairingError::InvalidAuthEpoch);
        }
        Ok(Self {
            tenant_id,
            actor_id,
            device_id,
            public_key,
            auth_epoch_at_registration,
            enabled: true,
        })
    }

    #[must_use]
    pub const fn tenant_id(&self) -> &TenantId {
        &self.tenant_id
    }

    #[must_use]
    pub const fn actor_id(&self) -> &ActorId {
        &self.actor_id
    }

    #[must_use]
    pub const fn device_id(&self) -> &DeviceId {
        &self.device_id
    }

    #[must_use]
    pub const fn public_key(&self) -> &DevicePublicKey {
        &self.public_key
    }

    #[must_use]
    pub const fn auth_epoch_at_registration(&self) -> u64 {
        self.auth_epoch_at_registration
    }

    #[must_use]
    pub const fn enabled(&self) -> bool {
        self.enabled
    }

    pub fn disable(&mut self) {
        self.enabled = false;
    }

    pub fn require_authorized(
        &self,
        user_enabled: bool,
        current_user_auth_epoch: u64,
    ) -> Result<(), DeviceAuthorizationError> {
        if !user_enabled {
            return Err(DeviceAuthorizationError::UserDisabled);
        }
        if !self.enabled {
            return Err(DeviceAuthorizationError::DeviceDisabled);
        }
        if current_user_auth_epoch == 0
            || current_user_auth_epoch != self.auth_epoch_at_registration
        {
            return Err(DeviceAuthorizationError::StaleAuthEpoch);
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeviceProofChallenge {
    device_id: DeviceId,
    nonce: [u8; DEVICE_PROOF_NONCE_BYTES],
    issued_at: UnixMillis,
    expires_at: UnixMillis,
    consumed_at: Option<UnixMillis>,
}

impl DeviceProofChallenge {
    pub fn issue(
        device_id: DeviceId,
        nonce: [u8; DEVICE_PROOF_NONCE_BYTES],
        issued_at: UnixMillis,
        expires_at: UnixMillis,
    ) -> Result<Self, DeviceProofError> {
        if expires_at <= issued_at {
            return Err(DeviceProofError::InvalidTimeline);
        }
        Ok(Self {
            device_id,
            nonce,
            issued_at,
            expires_at,
            consumed_at: None,
        })
    }

    #[must_use]
    pub const fn device_id(&self) -> &DeviceId {
        &self.device_id
    }

    #[must_use]
    pub const fn nonce(&self) -> &[u8; DEVICE_PROOF_NONCE_BYTES] {
        &self.nonce
    }

    #[must_use]
    pub const fn expires_at(&self) -> UnixMillis {
        self.expires_at
    }

    #[must_use]
    pub const fn consumed_at(&self) -> Option<UnixMillis> {
        self.consumed_at
    }

    pub fn verify_and_consume(
        &mut self,
        device_id: &DeviceId,
        nonce: &[u8],
        signature_valid: bool,
        now: UnixMillis,
    ) -> Result<(), DeviceProofError> {
        if self.consumed_at.is_some() {
            return Err(DeviceProofError::ReplayRejected);
        }
        if now >= self.expires_at {
            return Err(DeviceProofError::Expired);
        }
        if now < self.issued_at {
            return Err(DeviceProofError::InvalidTimeline);
        }
        if &self.device_id != device_id || self.nonce.as_slice() != nonce {
            return Err(DeviceProofError::BindingMismatch);
        }
        if !signature_valid {
            return Err(DeviceProofError::SignatureRejected);
        }
        self.consumed_at = Some(now);
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DevicePairingError {
    InvalidPublicKey,
    InvalidTimeline,
    Expired,
    ReplayRejected,
    BindingMismatch,
    NotAuthorized,
    InvalidAuthEpoch,
}

impl fmt::Display for DevicePairingError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidPublicKey => "device public key is invalid",
            Self::InvalidTimeline => "device pairing timeline is invalid",
            Self::Expired => "device pairing transaction expired",
            Self::ReplayRejected => "device pairing transaction replay rejected",
            Self::BindingMismatch => "device pairing binding mismatch",
            Self::NotAuthorized => "device pairing transaction is not authorized",
            Self::InvalidAuthEpoch => "device pairing auth epoch is invalid",
        })
    }
}

impl std::error::Error for DevicePairingError {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeviceAuthorizationError {
    DeviceDisabled,
    UserDisabled,
    StaleAuthEpoch,
}

impl fmt::Display for DeviceAuthorizationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::DeviceDisabled => "device credential is disabled",
            Self::UserDisabled => "device credential owner is disabled",
            Self::StaleAuthEpoch => "device credential auth epoch is stale",
        })
    }
}

impl std::error::Error for DeviceAuthorizationError {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeviceProofError {
    InvalidTimeline,
    Expired,
    ReplayRejected,
    BindingMismatch,
    SignatureRejected,
}

impl fmt::Display for DeviceProofError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidTimeline => "device proof challenge timeline is invalid",
            Self::Expired => "device proof challenge expired",
            Self::ReplayRejected => "device proof challenge replay rejected",
            Self::BindingMismatch => "device proof challenge binding mismatch",
            Self::SignatureRejected => "device proof signature rejected",
        })
    }
}

impl std::error::Error for DeviceProofError {}

fn is_canonical_p256_spki(bytes: &[u8]) -> bool {
    bytes.len() == P256_SPKI_DER_BYTES && bytes.starts_with(&P256_SPKI_PREFIX)
}

#[cfg(test)]
mod tests {
    use super::{
        DeviceAuthorizationError, DevicePairingError, DevicePairingTransaction,
        DeviceProofChallenge, DeviceProofError, DevicePublicKey, DevicePublicKeyAlgorithm,
        P256_SPKI_DER_BYTES, P256_SPKI_PREFIX, RegisteredDeviceCredential,
    };
    use profile_platform_primitives::{ActorId, DeviceId, TenantId, UnixMillis};

    fn key(last: u8) -> Result<DevicePublicKey, DevicePairingError> {
        let mut spki = vec![0_u8; P256_SPKI_DER_BYTES];
        spki[..P256_SPKI_PREFIX.len()].copy_from_slice(&P256_SPKI_PREFIX);
        spki[P256_SPKI_DER_BYTES - 1] = last;
        DevicePublicKey::p256_spki_der(spki)
    }

    #[test]
    fn pairing_binds_exact_user_device_and_public_key_and_is_one_shot()
    -> Result<(), Box<dyn std::error::Error>> {
        let tenant_id = TenantId::parse("tenant_01JPAIRING")?;
        let actor_id = ActorId::parse("actor_01JPAIRING")?;
        let device_id = DeviceId::parse("device_01JPAIRING")?;
        let public_key = key(1)?;
        assert_eq!(
            public_key.algorithm(),
            DevicePublicKeyAlgorithm::EcdsaP256Sha256
        );

        let mut pairing = DevicePairingTransaction::issue(
            tenant_id.clone(),
            device_id.clone(),
            public_key.clone(),
            UnixMillis::new(100),
            UnixMillis::new(200),
        )?;
        pairing.authorize(actor_id.clone(), UnixMillis::new(110))?;
        pairing.authorize(actor_id.clone(), UnixMillis::new(111))?;

        let foreign_actor = ActorId::parse("actor_01JFOREIGN")?;
        assert_eq!(
            pairing.authorize(foreign_actor.clone(), UnixMillis::new(112)),
            Err(DevicePairingError::BindingMismatch)
        );
        assert_eq!(
            pairing.complete(&actor_id, &device_id, &key(2)?, 7, UnixMillis::new(120)),
            Err(DevicePairingError::BindingMismatch)
        );
        assert_eq!(
            pairing.complete(
                &foreign_actor,
                &device_id,
                &public_key,
                7,
                UnixMillis::new(120),
            ),
            Err(DevicePairingError::NotAuthorized)
        );

        let registration =
            pairing.complete(&actor_id, &device_id, &public_key, 7, UnixMillis::new(120))?;
        assert_eq!(registration.tenant_id(), &tenant_id);
        assert_eq!(registration.actor_id(), &actor_id);
        assert_eq!(registration.device_id(), &device_id);
        assert_eq!(registration.public_key(), &public_key);
        assert_eq!(registration.auth_epoch_at_registration(), 7);
        assert_eq!(
            pairing.complete(&actor_id, &device_id, &public_key, 7, UnixMillis::new(121)),
            Err(DevicePairingError::ReplayRejected)
        );
        Ok(())
    }

    #[test]
    fn pairing_expiry_and_invalid_timeline_fail_closed() -> Result<(), Box<dyn std::error::Error>> {
        let tenant_id = TenantId::parse("tenant_01JPAIRING")?;
        let actor_id = ActorId::parse("actor_01JPAIRING")?;
        let device_id = DeviceId::parse("device_01JPAIRING")?;
        assert_eq!(
            DevicePairingTransaction::issue(
                tenant_id.clone(),
                device_id.clone(),
                key(1)?,
                UnixMillis::new(100),
                UnixMillis::new(100),
            ),
            Err(DevicePairingError::InvalidTimeline)
        );
        let mut pairing = DevicePairingTransaction::issue(
            tenant_id,
            device_id,
            key(1)?,
            UnixMillis::new(100),
            UnixMillis::new(200),
        )?;
        assert_eq!(
            pairing.authorize(actor_id, UnixMillis::new(200)),
            Err(DevicePairingError::Expired)
        );
        Ok(())
    }

    #[test]
    fn proof_challenge_rejects_wrong_binding_signature_expiry_and_replay()
    -> Result<(), Box<dyn std::error::Error>> {
        let device_id = DeviceId::parse("device_01JPAIRING")?;
        let nonce = [7_u8; 32];
        let mut challenge = DeviceProofChallenge::issue(
            device_id.clone(),
            nonce,
            UnixMillis::new(100),
            UnixMillis::new(200),
        )?;
        let foreign_device = DeviceId::parse("device_01JFOREIGN")?;
        assert_eq!(
            challenge.verify_and_consume(&foreign_device, &nonce, true, UnixMillis::new(110)),
            Err(DeviceProofError::BindingMismatch)
        );
        assert_eq!(
            challenge.verify_and_consume(&device_id, &[8_u8; 32], true, UnixMillis::new(110)),
            Err(DeviceProofError::BindingMismatch)
        );
        assert_eq!(
            challenge.verify_and_consume(&device_id, &nonce, false, UnixMillis::new(110)),
            Err(DeviceProofError::SignatureRejected)
        );
        assert_eq!(challenge.consumed_at(), None);
        challenge.verify_and_consume(&device_id, &nonce, true, UnixMillis::new(120))?;
        assert_eq!(challenge.consumed_at(), Some(UnixMillis::new(120)));
        assert_eq!(
            challenge.verify_and_consume(&device_id, &nonce, true, UnixMillis::new(121)),
            Err(DeviceProofError::ReplayRejected)
        );

        let mut expired = DeviceProofChallenge::issue(
            device_id.clone(),
            [9_u8; 32],
            UnixMillis::new(100),
            UnixMillis::new(200),
        )?;
        assert_eq!(
            expired.verify_and_consume(&device_id, &[9_u8; 32], true, UnixMillis::new(200)),
            Err(DeviceProofError::Expired)
        );
        Ok(())
    }

    #[test]
    fn device_and_user_revocation_are_separate_and_auth_epoch_invalidates_all_devices()
    -> Result<(), Box<dyn std::error::Error>> {
        let tenant_id = TenantId::parse("tenant_01JPAIRING")?;
        let actor_id = ActorId::parse("actor_01JPAIRING")?;
        let device_a = DeviceId::parse("device_01JPAIRINGA")?;
        let device_b = DeviceId::parse("device_01JPAIRINGB")?;
        let public_key = key(1)?;

        let mut a = RegisteredDeviceCredential::issue(
            tenant_id.clone(),
            actor_id.clone(),
            device_a,
            public_key.clone(),
            5,
        )?;
        let b = RegisteredDeviceCredential::issue(tenant_id, actor_id, device_b, public_key, 5)?;
        a.require_authorized(true, 5)?;
        b.require_authorized(true, 5)?;

        a.disable();
        assert_eq!(
            a.require_authorized(true, 5),
            Err(DeviceAuthorizationError::DeviceDisabled)
        );
        b.require_authorized(true, 5)?;
        assert_eq!(
            b.require_authorized(false, 5),
            Err(DeviceAuthorizationError::UserDisabled)
        );
        assert_eq!(
            b.require_authorized(true, 6),
            Err(DeviceAuthorizationError::StaleAuthEpoch)
        );
        assert_eq!(
            RegisteredDeviceCredential::issue(
                b.tenant_id().clone(),
                b.actor_id().clone(),
                b.device_id().clone(),
                b.public_key().clone(),
                0,
            ),
            Err(DevicePairingError::InvalidAuthEpoch)
        );
        Ok(())
    }

    #[test]
    fn public_key_transport_requires_exact_p256_spki_profile() -> Result<(), DevicePairingError> {
        let valid = key(1)?;
        assert_eq!(valid.spki_der().len(), P256_SPKI_DER_BYTES);

        let mut wrong_curve = valid.spki_der().to_vec();
        wrong_curve[22] ^= 1;
        assert_eq!(
            DevicePublicKey::p256_spki_der(wrong_curve),
            Err(DevicePairingError::InvalidPublicKey)
        );

        let mut compressed_point = valid.spki_der().to_vec();
        compressed_point[P256_SPKI_PREFIX.len() - 1] = 0x02;
        assert_eq!(
            DevicePublicKey::p256_spki_der(compressed_point),
            Err(DevicePairingError::InvalidPublicKey)
        );
        assert_eq!(
            DevicePublicKey::p256_spki_der(valid.spki_der()[..90].to_vec()),
            Err(DevicePairingError::InvalidPublicKey)
        );
        Ok(())
    }
}
