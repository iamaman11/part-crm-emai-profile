use crate::generation_reopen::GenerationReopenControlPort;
use crate::shipping_control_plane::{
    ControlPlaneCoordinator, MachineHttpMethod, MachineHttpPort, MachineHttpResponse,
    ShippingControlPlaneError,
};
use application_ports::ProfileCoordinatorPort;
use control_plane_contract::coordinator_api::{
    CoordinatorCommandDto, CoordinatorCommandRequestDto, CoordinatorOutcomeDto,
    CoordinatorProjectionDto, CoordinatorResponseDto, CoordinatorStatusDto,
};
use control_plane_contract::generation_reopen_api::BridgeGenerationOpeningMaterialResponse;
use encrypted_generation_domain::{
    GenerationDek, GenerationIdentity, GenerationMetadata, KeyId, NoncePrefix,
    inspect_generation_metadata_prelude, seal_generation,
};
use profile_platform_primitives::{
    ActorContext, ActorId, CorrelationId, DeviceId, GenerationId, LaunchIntentId, ProfileId,
    TenantId, TenantScope,
};

const FENCING_TOKEN: &str = "fence_opening_material_01";

struct OpeningMaterialTransport {
    tenant_id: TenantId,
    profile_id: ProfileId,
    device_id: DeviceId,
    opening_response: Vec<u8>,
    opening_requests: u64,
}

impl MachineHttpPort for OpeningMaterialTransport {
    type Error = ();

    fn request(
        &mut self,
        method: MachineHttpMethod,
        path: &str,
        _correlation_id: &CorrelationId,
        body: Option<&[u8]>,
    ) -> Result<MachineHttpResponse, Self::Error> {
        if method == MachineHttpMethod::Get {
            return coordinator_response(CoordinatorResponseDto {
                outcome: CoordinatorOutcomeDto::Snapshot,
                version: 11,
                sequence: 13,
                replayed: false,
                fencing_token: None,
                epoch: None,
                projection: CoordinatorProjectionDto {
                    tenant_id: self.tenant_id.as_str().to_owned(),
                    profile_id: self.profile_id.as_str().to_owned(),
                    status: CoordinatorStatusDto::Idle,
                    version: 11,
                    sequence: 13,
                    next_epoch: 7,
                    active_session_id: None,
                    active_device_id: None,
                    active_epoch: None,
                    idle_expires_at_ms: None,
                    hard_expires_at_ms: None,
                    drain_deadline_ms: None,
                    pending_launch_intent_id: None,
                    pending_intent_expires_at_ms: None,
                },
            });
        }

        if path.ends_with("/generation-reopen/opening-material") {
            self.opening_requests = self.opening_requests.saturating_add(1);
            return Ok(MachineHttpResponse::new(
                200,
                self.opening_response.clone(),
            ));
        }

        let request = serde_json::from_slice::<CoordinatorCommandRequestDto>(body.ok_or(())?)
            .map_err(|_| ())?;
        let CoordinatorCommandDto::Claim {
            device_id,
            session_id,
            ..
        } = request.command
        else {
            return Err(());
        };
        if device_id != self.device_id.as_str() {
            return Err(());
        }
        coordinator_response(CoordinatorResponseDto {
            outcome: CoordinatorOutcomeDto::LeaseClaimed,
            version: 12,
            sequence: request.sequence,
            replayed: false,
            fencing_token: Some(FENCING_TOKEN.to_owned()),
            epoch: Some(7),
            projection: CoordinatorProjectionDto {
                tenant_id: self.tenant_id.as_str().to_owned(),
                profile_id: self.profile_id.as_str().to_owned(),
                status: CoordinatorStatusDto::Active,
                version: 12,
                sequence: request.sequence,
                next_epoch: 8,
                active_session_id: Some(session_id),
                active_device_id: Some(device_id),
                active_epoch: Some(7),
                idle_expires_at_ms: Some(30_000),
                hard_expires_at_ms: Some(900_000),
                drain_deadline_ms: None,
                pending_launch_intent_id: None,
                pending_intent_expires_at_ms: None,
            },
        })
    }
}

fn coordinator_response(value: CoordinatorResponseDto) -> Result<MachineHttpResponse, ()> {
    serde_json::to_vec(&value)
        .map(|body| MachineHttpResponse::new(200, body))
        .map_err(|_| ())
}

struct Fixture {
    coordinator: ControlPlaneCoordinator<OpeningMaterialTransport>,
    tenant_id: TenantId,
    profile_id: ProfileId,
    metadata_prelude: Vec<u8>,
    key_id: KeyId,
}

impl Fixture {
    fn new(opening_response: Vec<u8>) -> Result<Self, Box<dyn std::error::Error>> {
        let tenant_id = TenantId::parse("tenant_opening_material_01")?;
        let profile_id = ProfileId::parse("profile_opening_material_01")?;
        let device_id = DeviceId::parse("device_opening_material_01")?;
        let actor = ActorContext::new(
            TenantScope::new(tenant_id.clone()),
            ActorId::parse("actor_opening_material_01")?,
            CorrelationId::parse("corr_opening_material_01")?,
        );
        let key_id = KeyId::parse("profile-generation-root-v1-7")?;
        let metadata_prelude = metadata_prelude(&tenant_id, &profile_id, &key_id)?;
        let mut coordinator = ControlPlaneCoordinator::new(OpeningMaterialTransport {
            tenant_id: tenant_id.clone(),
            profile_id: profile_id.clone(),
            device_id: device_id.clone(),
            opening_response,
            opening_requests: 0,
        });
        coordinator.claim_launch_intent(
            &actor,
            &profile_id,
            &device_id,
            &LaunchIntentId::parse("launch_opening_material_01")?,
        )?;
        Ok(Self {
            coordinator,
            tenant_id,
            profile_id,
            metadata_prelude,
            key_id,
        })
    }

    fn opening_material(&mut self) -> Result<GenerationDek, ShippingControlPlaneError> {
        self.coordinator.opening_material(
            &self.tenant_id,
            &self.profile_id,
            &self.metadata_prelude,
        )
    }
}

fn metadata_prelude(
    tenant_id: &TenantId,
    profile_id: &ProfileId,
    key_id: &KeyId,
) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let generation_id = GenerationId::parse("generation_opening_material_01")?;
    let base_generation_id = GenerationId::parse("generation_opening_material_base_01")?;
    let plaintext = b"opening-material-regression";
    let metadata = GenerationMetadata::for_plaintext(
        GenerationIdentity::new(
            tenant_id.clone(),
            profile_id.clone(),
            generation_id,
            Some(base_generation_id),
        ),
        key_id.clone(),
        NoncePrefix::new([0x71; 16]),
        4096,
        plaintext,
    )?;
    let sealed = seal_generation(
        &metadata,
        &GenerationDek::new(key_id.clone(), [0x37; 32]),
        plaintext,
    )?;
    let container = sealed.container();
    let inspected = inspect_generation_metadata_prelude(container)?;
    Ok(container[..inspected.prelude_bytes()].to_vec())
}

fn response(key_id: &str, dek_hex: String) -> Result<Vec<u8>, serde_json::Error> {
    serde_json::to_vec(&BridgeGenerationOpeningMaterialResponse::new(key_id, dek_hex))
}

#[test]
fn opening_material_accepts_only_exact_authenticated_key_material()
-> Result<(), Box<dyn std::error::Error>> {
    let mut fixture = Fixture::new(response(
        "profile-generation-root-v1-7",
        "ab".repeat(32),
    )?)?;
    let material = fixture.opening_material()?;
    assert_eq!(material.key_id(), &fixture.key_id);
    assert_eq!(fixture.coordinator.runtime_timing()?.idle_expires_at_ms(), 30_000);
    Ok(())
}

#[test]
fn opening_material_rejects_wrong_key_malformed_dek_and_oversized_response()
-> Result<(), Box<dyn std::error::Error>> {
    let malformed = [
        response("profile-generation-root-v1-8", "ab".repeat(32))?,
        response("profile-generation-root-v1-7", "AB".repeat(32))?,
        response("profile-generation-root-v1-7", "ab".repeat(31))?,
        b"{}".to_vec(),
        vec![b'x'; 2_048],
    ];
    for body in malformed {
        let mut fixture = Fixture::new(body)?;
        assert!(matches!(
            fixture.opening_material(),
            Err(ShippingControlPlaneError::InvalidResponse)
        ));
    }
    Ok(())
}
