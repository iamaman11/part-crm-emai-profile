use std::fmt::{Display, Formatter};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Environment {
    Rehearsal,
    Staging,
    Production,
}

impl Environment {
    pub fn parse(value: &str) -> Result<Self, PromotionModelError> {
        match value {
            "rehearsal" => Ok(Self::Rehearsal),
            "staging" => Ok(Self::Staging),
            "production" => Ok(Self::Production),
            other => Err(PromotionModelError::new(format!(
                "unsupported environment: {other}"
            ))),
        }
    }

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Rehearsal => "rehearsal",
            Self::Staging => "staging",
            Self::Production => "production",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromotionDecision {
    NoChange,
    Plan,
    Blocked,
}

impl PromotionDecision {
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::NoChange => "NO_CHANGE",
            Self::Plan => "PLAN",
            Self::Blocked => "BLOCKED",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PromotionRequest {
    pub environment: Environment,
    pub current_release_set_id: String,
    pub target_release_set_id: String,
    pub target_profile_id: String,
}

impl PromotionRequest {
    pub fn new(
        environment: Environment,
        current_release_set_id: impl Into<String>,
        target_release_set_id: impl Into<String>,
        target_profile_id: impl Into<String>,
    ) -> Result<Self, PromotionModelError> {
        let current_release_set_id = current_release_set_id.into();
        let target_release_set_id = target_release_set_id.into();
        let target_profile_id = target_profile_id.into();
        require_non_empty("current_release_set_id", &current_release_set_id)?;
        require_non_empty("target_release_set_id", &target_release_set_id)?;
        require_non_empty("target_profile_id", &target_profile_id)?;
        Ok(Self {
            environment,
            current_release_set_id,
            target_release_set_id,
            target_profile_id,
        })
    }

    #[must_use]
    pub fn decision(&self) -> PromotionDecision {
        if self.current_release_set_id == self.target_release_set_id {
            PromotionDecision::NoChange
        } else {
            PromotionDecision::Plan
        }
    }

    #[must_use]
    pub fn semantic_identity_input(&self) -> String {
        format!(
            "environment={}\ncurrent_release_set={}\ntarget_release_set={}\ntarget_profile={}",
            self.environment.name(),
            self.current_release_set_id,
            self.target_release_set_id,
            self.target_profile_id
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PromotionFence {
    pub expected_current_release_set_id: String,
}

impl PromotionFence {
    pub fn new(expected_current_release_set_id: impl Into<String>) -> Result<Self, PromotionModelError> {
        let expected_current_release_set_id = expected_current_release_set_id.into();
        require_non_empty(
            "expected_current_release_set_id",
            &expected_current_release_set_id,
        )?;
        Ok(Self {
            expected_current_release_set_id,
        })
    }

    pub fn verify(&self, observed_current_release_set_id: &str) -> Result<(), PromotionModelError> {
        if observed_current_release_set_id == self.expected_current_release_set_id {
            Ok(())
        } else {
            Err(PromotionModelError::new("PROMOTION_STALE"))
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PromotionModelError {
    message: String,
}

impl PromotionModelError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl Display for PromotionModelError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for PromotionModelError {}

fn require_non_empty(field: &str, value: &str) -> Result<(), PromotionModelError> {
    if value.trim().is_empty() {
        Err(PromotionModelError::new(format!(
            "{field} must not be empty"
        )))
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{Environment, PromotionDecision, PromotionFence, PromotionRequest};

    #[test]
    fn converged_transition_is_first_class_no_change() -> Result<(), Box<dyn std::error::Error>> {
        let request = PromotionRequest::new(
            Environment::Staging,
            "release-a",
            "release-a",
            "staging-core-v1",
        )?;
        assert_eq!(request.decision(), PromotionDecision::NoChange);
        Ok(())
    }

    #[test]
    fn changed_release_produces_plan() -> Result<(), Box<dyn std::error::Error>> {
        let request = PromotionRequest::new(
            Environment::Rehearsal,
            "release-a",
            "release-b",
            "rehearsal-core-v1",
        )?;
        assert_eq!(request.decision(), PromotionDecision::Plan);
        Ok(())
    }

    #[test]
    fn stale_promotion_fence_is_rejected() -> Result<(), Box<dyn std::error::Error>> {
        let fence = PromotionFence::new("release-a")?;
        assert!(fence.verify("release-c").is_err());
        Ok(())
    }

    #[test]
    fn unknown_environment_is_rejected() {
        assert!(Environment::parse("prod").is_err());
    }
}
