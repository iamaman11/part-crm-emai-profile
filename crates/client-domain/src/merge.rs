use core::fmt;
use profile_platform_primitives::ClientId;

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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ClientMergeError {
    SelfMerge,
}

impl fmt::Display for ClientMergeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("client cannot be merged into itself")
    }
}

impl std::error::Error for ClientMergeError {}

#[cfg(test)]
mod tests {
    use super::{ClientMergeError, ClientMergeState};
    use profile_platform_primitives::ClientId;

    #[test]
    fn merge_boundary_rejects_self_target_without_implementing_workflow()
    -> Result<(), Box<dyn std::error::Error>> {
        let source = ClientId::parse("client_01JMERGE")?;
        assert_eq!(
            ClientMergeState::merged_into(&source, source.clone()),
            Err(ClientMergeError::SelfMerge)
        );
        let target = ClientId::parse("client_02JMERGE")?;
        let state = ClientMergeState::merged_into(&source, target.clone())?;
        assert_eq!(state.target(), Some(&target));
        Ok(())
    }
}
