use crate::dirty_generation::{GenerationSealingMaterial, GenerationSealingMaterialPort, PreparedDirtyGeneration};
use crate::shipping_control_plane::{MachineHttpMethod, MachineHttpPort, ShippingControlPlaneError};
use crate::shipping_generation_save::{
    GenerationSuccessorCommitOutcome, GenerationSuccessorControlPort, GenerationUploadAuthorization,
    SignedGenerationUploadCapability,
};
use control_plane_contract::profile_generation_api::{
    BRIDGE_PROFILE_GENERATION_COMMIT_PATH_TEMPLATE,
    BRIDGE_PROFILE_GENERATION_UPLOAD_CAPABILITY_PATH_TEMPLATE,
    BridgeGenerationSuccessorCommitOutcomeDto, BridgeGenerationSuccessorCommitResponse,
    BridgeGenerationUploadCapabilityResponse, BridgeProfileGenerationSuccessorRequest,
};
use profile_platform_primitives::{CorrelationId, GenerationId, ProfileId, TenantId};
use session_domain::ProfileLease;
use std::fmt;
use std::sync::atomic::{AtomicU64, Ordering};

const MAX_UPLOAD_CAPABILITY_RESPONSE_BYTES: usize = 16_384;
const MAX_COMMIT_RESPONSE_BYTES: usize = 1_024;
const MAX_SIGNED_UPLOAD_URL_BYTES: usize = 8_192;
const MAX_SIGNED_UPLOAD_HEADERS: usize = 16;
const MAX_SIGNED_UPLOAD_HEADER_NAME_BYTES: usize = 128;
const MAX_SIGNED_UPLOAD_HEADER_VALUE_BYTES: usize = 1_024;
const MAX_UPLOAD_CAPABILITY_EXPIRES_SECONDS: u32 = 300;
static SAVE_REQUEST_SEQUENCE: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Eq, PartialEq)]
pub enum GenerationSuccessorControlError<S> {
    Sealing(S),
    ControlPlane(ShippingControlPlaneError),
}

impl<S: fmt::Display> fmt::Display for GenerationSuccessorControlError<S> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Sealing(error) => write!(formatter, "generation sealing material failed: {error}"),
            Self::ControlPlane(error) => write!(formatter, "generation successor control failed: {error}"),
        }
    }
}

impl<S: std::error::Error> std::error::Error for GenerationSuccessorControlError<S> {}

/// Typed machine client for the P3 successor endpoints.
///
/// It is intentionally not a coordinator. The live coordinator remains the sole lease/fence and
/// sealing-material owner; this adapter only serializes the canonical successor request and
/// delegates sealing material back to that existing owner.
pub struct ControlPlaneGenerationSuccessor<'a, T, S> {
    transport: T,
    sealer: &'a mut S,
}

impl<'a, T, S> ControlPlaneGenerationSuccessor<'a, T, S> {
    #[must_use]
    pub const fn new(transport: T, sealer: &'a mut S) -> Self {
        Self { transport, sealer }
    }
}

impl<T, S> GenerationSealingMaterialPort for ControlPlaneGenerationSuccessor<'_, T, S>
where
    S: GenerationSealingMaterialPort,
{
    type Error = GenerationSuccessorControlError<S::Error>;

    fn material_for(
        &mut self,
        tenant_id: &TenantId,
        profile_id: &ProfileId,
        base_generation_id: &GenerationId,
        generation_id: &GenerationId,
        plaintext_digest: [u8; 32],
    ) -> Result<GenerationSealingMaterial, Self::Error> {
        self.sealer
            .material_for(
                tenant_id,
                profile_id,
                base_generation_id,
                generation_id,
                plaintext_digest,
            )
            .map_err(GenerationSuccessorControlError::Sealing)
    }
}

impl<T, S> GenerationSuccessorControlPort for ControlPlaneGenerationSuccessor<'_, T, S>
where
    T: MachineHttpPort,
    S: GenerationSealingMaterialPort,
{
    fn upload_authorization(
        &mut self,
        tenant_id: &TenantId,
        profile_id: &ProfileId,
        base_generation_id: &GenerationId,
        prepared: &PreparedDirtyGeneration,
        lease: &ProfileLease,
    ) -> Result<GenerationUploadAuthorization, Self::Error> {
        let request = successor_request(base_generation_id, prepared, lease)?;
        let response = post_json(
            &mut self.transport,
            &successor_path(
                BRIDGE_PROFILE_GENERATION_UPLOAD_CAPABILITY_PATH_TEMPLATE,
                tenant_id,
                profile_id,
            ),
            &request,
        )?;
        if response.len() > MAX_UPLOAD_CAPABILITY_RESPONSE_BYTES {
            return Err(control_error(ShippingControlPlaneError::InvalidResponse));
        }
        let decoded = serde_json::from_slice::<BridgeGenerationUploadCapabilityResponse>(&response)
            .map_err(|_| control_error(ShippingControlPlaneError::InvalidResponse))?;
        decode_upload_authorization(decoded)
    }

    fn commit_successor(
        &mut self,
        tenant_id: &TenantId,
        profile_id: &ProfileId,
        base_generation_id: &GenerationId,
        prepared: &PreparedDirtyGeneration,
        lease: &ProfileLease,
    ) -> Result<GenerationSuccessorCommitOutcome, Self::Error> {
        let request = successor_request(base_generation_id, prepared, lease)?;
        let response = post_json(
            &mut self.transport,
            &successor_path(
                BRIDGE_PROFILE_GENERATION_COMMIT_PATH_TEMPLATE,
                tenant_id,
                profile_id,
            ),
            &request,
        )?;
        if response.len() > MAX_COMMIT_RESPONSE_BYTES {
            return Err(control_error(ShippingControlPlaneError::InvalidResponse));
        }
        let decoded = serde_json::from_slice::<BridgeGenerationSuccessorCommitResponse>(&response)
            .map_err(|_| control_error(ShippingControlPlaneError::InvalidResponse))?;
        Ok(match decoded.outcome {
            BridgeGenerationSuccessorCommitOutcomeDto::Activated => {
                GenerationSuccessorCommitOutcome::Activated
            }
            BridgeGenerationSuccessorCommitOutcomeDto::AlreadyActive => {
                GenerationSuccessorCommitOutcome::AlreadyActive
            }
        })
    }
}

fn successor_request<S>(
    base_generation_id: &GenerationId,
    prepared: &PreparedDirtyGeneration,
    lease: &ProfileLease,
) -> Result<BridgeProfileGenerationSuccessorRequest, GenerationSuccessorControlError<S>> {
    let container_bytes = u64::try_from(prepared.sealed().container().len())
        .map_err(|_| control_error(ShippingControlPlaneError::InvalidResponse))?;
    Ok(BridgeProfileGenerationSuccessorRequest::new(
        base_generation_id.as_str(),
        prepared.sealed().metadata().generation_id().as_str(),
        &prepared.object_key(),
        prepared.metadata_digest(),
        prepared.container_digest(),
        container_bytes,
        lease.session_id().as_str(),
        lease.fencing_token().as_str(),
        lease.epoch(),
    ))
}

fn post_json<T, S, B>(
    transport: &mut T,
    path: &str,
    body: &B,
) -> Result<Vec<u8>, GenerationSuccessorControlError<S>>
where
    T: MachineHttpPort,
    B: serde::Serialize,
{
    let correlation_id = next_correlation_id()?;
    let mut encoded = serde_json::to_vec(body)
        .map_err(|_| control_error(ShippingControlPlaneError::InvalidResponse))?;
    let response = transport
        .request(
            MachineHttpMethod::PostJson,
            path,
            &correlation_id,
            Some(&encoded),
        )
        .map_err(|_| control_error(ShippingControlPlaneError::Transport));
    encoded.fill(0);
    let response = response?;
    if !(200..300).contains(&response.status()) {
        return Err(control_error(ShippingControlPlaneError::HttpStatus));
    }
    Ok(response.body().to_vec())
}

fn decode_upload_authorization<S>(
    decoded: BridgeGenerationUploadCapabilityResponse,
) -> Result<GenerationUploadAuthorization, GenerationSuccessorControlError<S>> {
    match decoded.state.as_str() {
        "verified"
            if decoded.method.is_none()
                && decoded.url.is_none()
                && decoded.headers.is_empty()
                && decoded.expires_seconds.is_none() =>
        {
            Ok(GenerationUploadAuthorization::Verified)
        }
        "uploadRequired" => {
            let method = decoded
                .method
                .ok_or_else(|| control_error(ShippingControlPlaneError::InvalidResponse))?;
            let url = decoded
                .url
                .ok_or_else(|| control_error(ShippingControlPlaneError::InvalidResponse))?;
            let expires_seconds = decoded
                .expires_seconds
                .ok_or_else(|| control_error(ShippingControlPlaneError::InvalidResponse))?;
            if method != "PUT"
                || !valid_signed_upload_url(&url)
                || !(1..=MAX_UPLOAD_CAPABILITY_EXPIRES_SECONDS).contains(&expires_seconds)
                || decoded.headers.is_empty()
                || decoded.headers.len() > MAX_SIGNED_UPLOAD_HEADERS
            {
                return Err(control_error(ShippingControlPlaneError::InvalidResponse));
            }
            let mut headers = Vec::with_capacity(decoded.headers.len());
            for header in decoded.headers {
                if !valid_header_name(&header.name) || !valid_header_value(&header.value) {
                    return Err(control_error(ShippingControlPlaneError::InvalidResponse));
                }
                headers.push((header.name, header.value));
            }
            Ok(GenerationUploadAuthorization::UploadRequired(
                SignedGenerationUploadCapability::new(url, headers, expires_seconds),
            ))
        }
        _ => Err(control_error(ShippingControlPlaneError::InvalidResponse)),
    }
}

fn valid_signed_upload_url(value: &str) -> bool {
    let Some(rest) = value.strip_prefix("https://") else {
        return false;
    };
    !rest.is_empty()
        && value.len() <= MAX_SIGNED_UPLOAD_URL_BYTES
        && !value.contains('#')
        && !value
            .bytes()
            .any(|byte| byte.is_ascii_control() || byte.is_ascii_whitespace())
}

fn valid_header_name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_SIGNED_UPLOAD_HEADER_NAME_BYTES
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-'
        })
}

fn valid_header_value(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_SIGNED_UPLOAD_HEADER_VALUE_BYTES
        && !value
            .bytes()
            .any(|byte| byte == b'\r' || byte == b'\n' || byte == 0)
}

fn successor_path(template: &str, tenant_id: &TenantId, profile_id: &ProfileId) -> String {
    template
        .replace("{tenantId}", tenant_id.as_str())
        .replace("{profileId}", profile_id.as_str())
}

fn next_correlation_id<S>() -> Result<CorrelationId, GenerationSuccessorControlError<S>> {
    let sequence = SAVE_REQUEST_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    CorrelationId::parse(format!("corr_p3_generation_save_{sequence:020}"))
        .map_err(|_| control_error(ShippingControlPlaneError::Clock))
}

const fn control_error<S>(
    error: ShippingControlPlaneError,
) -> GenerationSuccessorControlError<S> {
    GenerationSuccessorControlError::ControlPlane(error)
}

#[cfg(test)]
mod tests {
    use super::{ControlPlaneGenerationSuccessor, GenerationSuccessorControlError};
    use crate::dirty_generation::{
        GenerationSealingMaterial, GenerationSealingMaterialPort, prepare_dirty_generation_candidate,
    };
    use crate::local_profile::{LocalGenerationRecord, MaterializationRoot};
    use crate::shipping_control_plane::{MachineHttpMethod, MachineHttpPort, MachineHttpResponse};
    use crate::shipping_generation_save::{
        GenerationSuccessorCommitOutcome, GenerationSuccessorControlPort,
        GenerationUploadAuthorization,
    };
    use bridge_domain::BridgePortError;
    use encrypted_generation_domain::{GenerationDek, KeyId, NoncePrefix};
    use profile_platform_primitives::{
        CorrelationId, DeviceId, FencingToken, GenerationId, ProfileId, SessionId, TenantId,
        UnixMillis,
    };
    use session_domain::ProfileLease;
    use std::collections::VecDeque;

    #[derive(Default)]
    struct Transport {
        responses: VecDeque<MachineHttpResponse>,
        paths: Vec<String>,
    }

    impl MachineHttpPort for Transport {
        type Error = BridgePortError;

        fn request(
            &mut self,
            method: MachineHttpMethod,
            path: &str,
            _correlation_id: &CorrelationId,
            body: Option<&[u8]>,
        ) -> Result<MachineHttpResponse, Self::Error> {
            if method != MachineHttpMethod::PostJson || body.is_none() {
                return Err(BridgePortError::InvalidResponse);
            }
            self.paths.push(path.to_owned());
            self.responses
                .pop_front()
                .ok_or(BridgePortError::Unavailable)
        }
    }

    #[derive(Default)]
    struct Sealer;

    impl GenerationSealingMaterialPort for Sealer {
        type Error = BridgePortError;

        fn material_for(
            &mut self,
            _tenant_id: &TenantId,
            _profile_id: &ProfileId,
            _base_generation_id: &GenerationId,
            _generation_id: &GenerationId,
            _plaintext_digest: [u8; 32],
        ) -> Result<GenerationSealingMaterial, Self::Error> {
            Ok(GenerationSealingMaterial::new(
                GenerationDek::new(
                    KeyId::parse("profile-generation-root-v1-7")
                        .map_err(|_| BridgePortError::InvalidResponse)?,
                    [7; 32],
                ),
                NoncePrefix::new([8; 16]),
                4096,
            ))
        }
    }

    struct Fixture {
        root_path: std::path::PathBuf,
        root: MaterializationRoot,
        tenant: TenantId,
        profile: ProfileId,
        base: GenerationId,
        prepared: crate::dirty_generation::PreparedDirtyGeneration,
        lease: ProfileLease,
    }

    impl Fixture {
        fn new() -> Result<Self, Box<dyn std::error::Error>> {
            let root_path = std::env::temp_dir().join(format!(
                "profile-bridge-successor-control-{}",
                std::process::id()
            ));
            let _ = std::fs::remove_dir_all(&root_path);
            let root = MaterializationRoot::open_or_create(root_path.clone())?;
            let tenant = TenantId::parse("tenant_successor_control_01")?;
            let profile = ProfileId::parse("profile_successor_control_01")?;
            let base = GenerationId::parse("generation_successor_control_base_01")?;
            let candidate = GenerationId::parse("generation_successor_control_next_01")?;
            let device = DeviceId::parse("device_successor_control_01")?;
            let workspace = root.create_generation(&tenant, &profile, &base)?;
            std::fs::write(workspace.path().join("prefs.js"), b"save")?;
            let mut record = LocalGenerationRecord::new(base.clone(), 4, UnixMillis::new(1));
            record.set_locked(true)?;
            record.begin_use(UnixMillis::new(2))?;
            record.graceful_close(UnixMillis::new(3))?;
            let mut sealer = Sealer;
            let prepared = prepare_dirty_generation_candidate(
                &record,
                &workspace,
                &root,
                &tenant,
                &profile,
                &candidate,
                &mut sealer,
            )?;
            let lease = ProfileLease::issue(
                tenant.clone(),
                profile.clone(),
                SessionId::parse("session_successor_control_01")?,
                device,
                4,
                FencingToken::parse("fence_successor_control_01")?,
            )?;
            Ok(Self {
                root_path,
                root,
                tenant,
                profile,
                base,
                prepared,
                lease,
            })
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = crate::test_support::remove_test_root(&self.root_path);
        }
    }

    #[test]
    fn verified_upload_response_and_commit_are_strictly_typed()
    -> Result<(), Box<dyn std::error::Error>> {
        let fixture = Fixture::new()?;
        let transport = Transport {
            responses: VecDeque::from([
                MachineHttpResponse::new(
                    200,
                    br#"{"state":"verified","method":null,"url":null,"headers":[],"expiresSeconds":null}"#
                        .to_vec(),
                ),
                MachineHttpResponse::new(200, br#"{"outcome":"ACTIVATED"}"#.to_vec()),
            ]),
            paths: Vec::new(),
        };
        let mut sealer = Sealer;
        let mut client = ControlPlaneGenerationSuccessor::new(transport, &mut sealer);
        assert_eq!(
            client.upload_authorization(
                &fixture.tenant,
                &fixture.profile,
                &fixture.base,
                &fixture.prepared,
                &fixture.lease,
            )?,
            GenerationUploadAuthorization::Verified
        );
        assert_eq!(
            client.commit_successor(
                &fixture.tenant,
                &fixture.profile,
                &fixture.base,
                &fixture.prepared,
                &fixture.lease,
            )?,
            GenerationSuccessorCommitOutcome::Activated
        );
        Ok(())
    }

    #[test]
    fn malformed_upload_capability_fails_closed() -> Result<(), Box<dyn std::error::Error>> {
        let fixture = Fixture::new()?;
        let transport = Transport {
            responses: VecDeque::from([MachineHttpResponse::new(
                200,
                br#"{"state":"uploadRequired","method":"GET","url":"http://bad.example/object","headers":[],"expiresSeconds":0}"#
                    .to_vec(),
            )]),
            paths: Vec::new(),
        };
        let mut sealer = Sealer;
        let mut client = ControlPlaneGenerationSuccessor::new(transport, &mut sealer);
        assert!(matches!(
            client.upload_authorization(
                &fixture.tenant,
                &fixture.profile,
                &fixture.base,
                &fixture.prepared,
                &fixture.lease,
            ),
            Err(GenerationSuccessorControlError::ControlPlane(_))
        ));
        Ok(())
    }
}
