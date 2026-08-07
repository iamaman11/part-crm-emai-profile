use contracts::ProblemCode;
use profile_platform_primitives::ActorContext;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AuditResult {
    Succeeded,
    Rejected(ProblemCode),
    Failed(ProblemCode),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AuditRecord<'a> {
    actor: &'a ActorContext,
    action: &'static str,
    resource_id: &'a str,
    result: AuditResult,
}

impl<'a> AuditRecord<'a> {
    #[must_use]
    pub const fn new(
        actor: &'a ActorContext,
        action: &'static str,
        resource_id: &'a str,
        result: AuditResult,
    ) -> Self {
        Self {
            actor,
            action,
            resource_id,
            result,
        }
    }

    #[must_use]
    pub const fn actor(&self) -> &ActorContext {
        self.actor
    }

    #[must_use]
    pub const fn action(&self) -> &'static str {
        self.action
    }

    #[must_use]
    pub const fn resource_id(&self) -> &str {
        self.resource_id
    }

    #[must_use]
    pub const fn result(&self) -> AuditResult {
        self.result
    }
}

pub trait AuditPort {
    type Error;

    fn append(&mut self, record: AuditRecord<'_>) -> Result<(), Self::Error>;
}
