use crate::{ClientError, ClientRecord, ClientStatus};
use core::fmt;
use profile_platform_primitives::{AggregateVersion, ClientId, TenantId};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ClientMergeState {
    NotMerged,
    MergedInto(ClientId),
}

impl ClientMergeState {
    pub fn merged_into(source: &ClientId, target: ClientId) -> Result<Self, ClientMergeError> {
        if source == &target {
            return Err(ClientMergeError::SelfMerge);
        }
        Ok(Self::MergedInto(target))
    }

    #[must_use]
    pub const fn is_merged(&self) -> bool {
        matches!(self, Self::MergedInto(_))
    }

    #[must_use]
    pub const fn target(&self) -> Option<&ClientId> {
        match self {
            Self::NotMerged => None,
            Self::MergedInto(target) => Some(target),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClientMergePlan {
    tenant_id: TenantId,
    source_client_id: ClientId,
    target_client_id: ClientId,
    source_expected_version: AggregateVersion,
    target_expected_version: AggregateVersion,
    source_next_version: AggregateVersion,
    merge_state: ClientMergeState,
}

impl ClientMergePlan {
    #[must_use]
    pub const fn tenant_id(&self) -> &TenantId {
        &self.tenant_id
    }

    #[must_use]
    pub const fn source_client_id(&self) -> &ClientId {
        &self.source_client_id
    }

    #[must_use]
    pub const fn target_client_id(&self) -> &ClientId {
        &self.target_client_id
    }

    #[must_use]
    pub const fn source_expected_version(&self) -> AggregateVersion {
        self.source_expected_version
    }

    #[must_use]
    pub const fn target_expected_version(&self) -> AggregateVersion {
        self.target_expected_version
    }

    #[must_use]
    pub const fn source_next_version(&self) -> AggregateVersion {
        self.source_next_version
    }

    #[must_use]
    pub const fn merge_state(&self) -> &ClientMergeState {
        &self.merge_state
    }
}

pub fn merge_clients(
    source: &mut ClientRecord,
    target: &ClientRecord,
    source_expected_version: AggregateVersion,
    target_expected_version: AggregateVersion,
) -> Result<ClientMergePlan, ClientMergeError> {
    if source.tenant_id() != target.tenant_id() {
        return Err(ClientMergeError::TenantMismatch);
    }
    if source.client_id() == target.client_id() {
        return Err(ClientMergeError::SelfMerge);
    }
    if source.version() != source_expected_version {
        return Err(ClientMergeError::SourceVersionConflict);
    }
    if target.version() != target_expected_version {
        return Err(ClientMergeError::TargetVersionConflict);
    }
    match source.status() {
        ClientStatus::Active => {}
        ClientStatus::Merged => return Err(ClientMergeError::SourceAlreadyMerged),
        ClientStatus::Archived => return Err(ClientMergeError::SourceNotActive),
    }
    match target.status() {
        ClientStatus::Active => {}
        ClientStatus::Merged => return Err(ClientMergeError::MergeCycle),
        ClientStatus::Archived => return Err(ClientMergeError::TargetNotActive),
    }

    let tenant_id = source.tenant_id().clone();
    let source_client_id = source.client_id().clone();
    let target_client_id = target.client_id().clone();
    let merge_state = ClientMergeState::merged_into(&source_client_id, target_client_id.clone())?;

    source.mark_merged().map_err(map_source_transition_error)?;

    Ok(ClientMergePlan {
        tenant_id,
        source_client_id,
        target_client_id,
        source_expected_version,
        target_expected_version,
        source_next_version: source.version(),
        merge_state,
    })
}

fn map_source_transition_error(error: ClientError) -> ClientMergeError {
    match error {
        ClientError::VersionOverflow => ClientMergeError::VersionOverflow,
        ClientError::InvalidStatusTransition => ClientMergeError::SourceNotActive,
        ClientError::InvalidDisplayName => ClientMergeError::InvalidSourceState,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ClientMergeError {
    TenantMismatch,
    SelfMerge,
    SourceVersionConflict,
    TargetVersionConflict,
    SourceNotActive,
    SourceAlreadyMerged,
    TargetNotActive,
    MergeCycle,
    VersionOverflow,
    InvalidSourceState,
}

impl fmt::Display for ClientMergeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::TenantMismatch => "source and target client tenants differ",
            Self::SelfMerge => "client cannot be merged into itself",
            Self::SourceVersionConflict => "source client merge version conflict",
            Self::TargetVersionConflict => "target client merge version conflict",
            Self::SourceNotActive => "source client is not active",
            Self::SourceAlreadyMerged => "source client is already merged",
            Self::TargetNotActive => "target client is not active",
            Self::MergeCycle => "merge target is already merged",
            Self::VersionOverflow => "source client merge version overflow",
            Self::InvalidSourceState => "source client merge state is invalid",
        })
    }
}

impl std::error::Error for ClientMergeError {}

#[cfg(test)]
mod tests {
    use super::{ClientMergeError, ClientMergeState, merge_clients};
    use crate::{ClientKind, ClientRecord, ClientStatus};
    use profile_platform_primitives::{AggregateVersion, ClientId, TenantId};

    fn client(
        tenant: &str,
        client_id: &str,
        version: u64,
        status: ClientStatus,
    ) -> Result<ClientRecord, Box<dyn std::error::Error>> {
        Ok(ClientRecord::restore(
            TenantId::parse(tenant)?,
            ClientId::parse(client_id)?,
            AggregateVersion::new(version)?,
            ClientKind::Person,
            client_id,
            status,
        )?)
    }

    #[test]
    fn merge_state_rejects_self_target() -> Result<(), Box<dyn std::error::Error>> {
        let source = ClientId::parse("client_01JMERGE")?;
        assert_eq!(
            ClientMergeState::merged_into(&source, source.clone()),
            Err(ClientMergeError::SelfMerge)
        );
        Ok(())
    }

    #[test]
    fn successful_merge_is_same_tenant_checked_version_and_one_way()
    -> Result<(), Box<dyn std::error::Error>> {
        let mut source = client(
            "tenant_01JMERGE",
            "client_01JMERGE",
            4,
            ClientStatus::Active,
        )?;
        let target = client(
            "tenant_01JMERGE",
            "client_02JMERGE",
            9,
            ClientStatus::Active,
        )?;

        let plan = merge_clients(
            &mut source,
            &target,
            AggregateVersion::new(4)?,
            AggregateVersion::new(9)?,
        )?;

        assert_eq!(plan.tenant_id(), source.tenant_id());
        assert_eq!(plan.source_client_id().as_str(), "client_01JMERGE");
        assert_eq!(plan.target_client_id().as_str(), "client_02JMERGE");
        assert_eq!(plan.source_expected_version().value(), 4);
        assert_eq!(plan.target_expected_version().value(), 9);
        assert_eq!(plan.source_next_version().value(), 5);
        assert_eq!(plan.merge_state().target(), Some(plan.target_client_id()));
        assert_eq!(source.status(), ClientStatus::Merged);
        assert_eq!(source.version().value(), 5);
        assert_eq!(target.status(), ClientStatus::Active);
        assert_eq!(target.version().value(), 9);
        assert_eq!(
            source.rename("cannot resurrect"),
            Err(crate::ClientError::InvalidStatusTransition)
        );
        Ok(())
    }

    #[test]
    fn tenant_or_self_merge_failure_never_mutates_source() -> Result<(), Box<dyn std::error::Error>>
    {
        let mut source = client(
            "tenant_01JMERGE",
            "client_01JMERGE",
            1,
            ClientStatus::Active,
        )?;
        let other_tenant = client(
            "tenant_02JMERGE",
            "client_02JMERGE",
            1,
            ClientStatus::Active,
        )?;
        assert_eq!(
            merge_clients(
                &mut source,
                &other_tenant,
                AggregateVersion::INITIAL,
                AggregateVersion::INITIAL,
            ),
            Err(ClientMergeError::TenantMismatch)
        );
        assert_eq!(source.status(), ClientStatus::Active);
        assert_eq!(source.version(), AggregateVersion::INITIAL);

        let same = source.clone();
        assert_eq!(
            merge_clients(
                &mut source,
                &same,
                AggregateVersion::INITIAL,
                AggregateVersion::INITIAL,
            ),
            Err(ClientMergeError::SelfMerge)
        );
        assert_eq!(source.status(), ClientStatus::Active);
        assert_eq!(source.version(), AggregateVersion::INITIAL);
        Ok(())
    }

    #[test]
    fn version_conflicts_fail_before_state_transition() -> Result<(), Box<dyn std::error::Error>> {
        let mut source = client(
            "tenant_01JMERGE",
            "client_01JMERGE",
            3,
            ClientStatus::Active,
        )?;
        let target = client(
            "tenant_01JMERGE",
            "client_02JMERGE",
            7,
            ClientStatus::Active,
        )?;
        assert_eq!(
            merge_clients(
                &mut source,
                &target,
                AggregateVersion::new(2)?,
                AggregateVersion::new(7)?,
            ),
            Err(ClientMergeError::SourceVersionConflict)
        );
        assert_eq!(source.status(), ClientStatus::Active);
        assert_eq!(source.version().value(), 3);

        assert_eq!(
            merge_clients(
                &mut source,
                &target,
                AggregateVersion::new(3)?,
                AggregateVersion::new(6)?,
            ),
            Err(ClientMergeError::TargetVersionConflict)
        );
        assert_eq!(source.status(), ClientStatus::Active);
        assert_eq!(source.version().value(), 3);
        Ok(())
    }

    #[test]
    fn inactive_or_already_merged_clients_fail_closed() -> Result<(), Box<dyn std::error::Error>> {
        let target = client(
            "tenant_01JMERGE",
            "client_02JMERGE",
            1,
            ClientStatus::Active,
        )?;
        let mut archived_source = client(
            "tenant_01JMERGE",
            "client_01JMERGE",
            1,
            ClientStatus::Archived,
        )?;
        assert_eq!(
            merge_clients(
                &mut archived_source,
                &target,
                AggregateVersion::INITIAL,
                AggregateVersion::INITIAL,
            ),
            Err(ClientMergeError::SourceNotActive)
        );

        let mut merged_source = client(
            "tenant_01JMERGE",
            "client_03JMERGE",
            2,
            ClientStatus::Merged,
        )?;
        assert_eq!(
            merge_clients(
                &mut merged_source,
                &target,
                AggregateVersion::new(2)?,
                AggregateVersion::INITIAL,
            ),
            Err(ClientMergeError::SourceAlreadyMerged)
        );

        let archived_target = client(
            "tenant_01JMERGE",
            "client_04JMERGE",
            1,
            ClientStatus::Archived,
        )?;
        let mut active_source = client(
            "tenant_01JMERGE",
            "client_05JMERGE",
            1,
            ClientStatus::Active,
        )?;
        assert_eq!(
            merge_clients(
                &mut active_source,
                &archived_target,
                AggregateVersion::INITIAL,
                AggregateVersion::INITIAL,
            ),
            Err(ClientMergeError::TargetNotActive)
        );

        let merged_target = client(
            "tenant_01JMERGE",
            "client_06JMERGE",
            2,
            ClientStatus::Merged,
        )?;
        assert_eq!(
            merge_clients(
                &mut active_source,
                &merged_target,
                AggregateVersion::INITIAL,
                AggregateVersion::new(2)?,
            ),
            Err(ClientMergeError::MergeCycle)
        );
        assert_eq!(active_source.status(), ClientStatus::Active);
        Ok(())
    }

    #[test]
    fn version_overflow_does_not_partially_merge_source() -> Result<(), Box<dyn std::error::Error>>
    {
        let mut source = client(
            "tenant_01JMERGE",
            "client_01JMERGE",
            u64::MAX,
            ClientStatus::Active,
        )?;
        let target = client(
            "tenant_01JMERGE",
            "client_02JMERGE",
            1,
            ClientStatus::Active,
        )?;
        assert_eq!(
            merge_clients(
                &mut source,
                &target,
                AggregateVersion::new(u64::MAX)?,
                AggregateVersion::INITIAL,
            ),
            Err(ClientMergeError::VersionOverflow)
        );
        assert_eq!(source.status(), ClientStatus::Active);
        assert_eq!(source.version().value(), u64::MAX);
        Ok(())
    }
}
