use crate::profile_coordinator::CoordinatorProjection;
use profile_platform_primitives::{OutboxEventId, ProfileId, TenantScope, UnixMillis};
use serde::Deserialize;
use worker::d1::D1Database;
use worker::{Error, Result, query};

const INSERT_PROJECTION_COMMAND: &str = r#"
INSERT OR IGNORE INTO profile_coordinator_projection_commands (
    tenant_id,
    profile_id,
    coordinator_sequence,
    coordinator_version,
    outbox_event_id,
    outcome,
    projection_json,
    projected_at_ms
) VALUES (?, ?, ?, ?, ?, ?, ?, ?)
"#;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CoordinatorProjectionOutcome {
    Snapshot,
    LaunchIntentIssued,
    LeaseClaimed,
    HeartbeatAccepted,
    Released,
    DrainStarted,
    TimedOut,
    LaunchIntentExpired,
    Recovered,
    NoChange,
}

impl CoordinatorProjectionOutcome {
    #[must_use]
    pub const fn database_value(self) -> &'static str {
        match self {
            Self::Snapshot => "snapshot",
            Self::LaunchIntentIssued => "launch_intent_issued",
            Self::LeaseClaimed => "lease_claimed",
            Self::HeartbeatAccepted => "heartbeat_accepted",
            Self::Released => "released",
            Self::DrainStarted => "drain_started",
            Self::TimedOut => "timed_out",
            Self::LaunchIntentExpired => "launch_intent_expired",
            Self::Recovered => "recovered",
            Self::NoChange => "no_change",
        }
    }
}

pub struct CoordinatorProjectionMutation<'a> {
    pub profile_id: &'a ProfileId,
    pub projection: &'a CoordinatorProjection,
    pub outcome: CoordinatorProjectionOutcome,
    pub outbox_event_id: &'a OutboxEventId,
    pub projected_at: UnixMillis,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CoordinatorProjectionWrite {
    Applied,
    Replayed,
}

pub struct D1ProfileCoordinatorRepository {
    database: D1Database,
}

impl D1ProfileCoordinatorRepository {
    #[must_use]
    pub const fn new(database: D1Database) -> Self {
        Self { database }
    }

    pub async fn project(
        &self,
        scope: &TenantScope,
        mutation: CoordinatorProjectionMutation<'_>,
    ) -> Result<CoordinatorProjectionWrite> {
        let tenant_id = scope.tenant_id().as_str();
        if mutation.projection.tenant_id != tenant_id
            || mutation.projection.profile_id != mutation.profile_id.as_str()
        {
            return Err(Error::RustError(
                "coordinator projection identity mismatch".to_owned(),
            ));
        }

        let sequence = sqlite_integer_value(mutation.projection.sequence)?;
        let version = sqlite_integer_value(mutation.projection.version)?;
        let projected_at = sqlite_integer_value(mutation.projected_at.value())?;
        let projection_json = serde_json::to_string(mutation.projection)
            .map_err(|error| Error::RustError(error.to_string()))?;
        let outcome = mutation.outcome.database_value();

        let existing = self
            .projection_command(scope, mutation.profile_id, mutation.projection.sequence)
            .await?;
        if let Some(existing) = existing {
            return verify_replay(existing, outcome, &projection_json);
        }

        query!(
            &self.database,
            INSERT_PROJECTION_COMMAND,
            tenant_id,
            mutation.profile_id.as_str(),
            sequence,
            version,
            mutation.outbox_event_id.as_str(),
            outcome,
            projection_json.as_str(),
            projected_at
        )?
        .run()
        .await?;

        let stored = self
            .projection_command(scope, mutation.profile_id, mutation.projection.sequence)
            .await?
            .ok_or_else(|| {
                Error::RustError("coordinator projection command was not persisted".to_owned())
            })?;
        match verify_replay(stored, outcome, &projection_json)? {
            CoordinatorProjectionWrite::Replayed => Ok(CoordinatorProjectionWrite::Applied),
            CoordinatorProjectionWrite::Applied => Ok(CoordinatorProjectionWrite::Applied),
        }
    }

    pub async fn projected_sequence(
        &self,
        scope: &TenantScope,
        profile_id: &ProfileId,
    ) -> Result<Option<u64>> {
        let value = query!(
            &self.database,
            r#"
            SELECT coordinator_sequence
            FROM profile_coordinator_projections
            WHERE tenant_id = ? AND profile_id = ?
            "#,
            scope.tenant_id().as_str(),
            profile_id.as_str()
        )?
        .first::<i64>(Some("coordinator_sequence"))
        .await?;
        value.map(sqlite_unsigned).transpose()
    }

    async fn projection_command(
        &self,
        scope: &TenantScope,
        profile_id: &ProfileId,
        sequence: u64,
    ) -> Result<Option<ProjectionCommandRow>> {
        query!(
            &self.database,
            r#"
            SELECT outcome, projection_json
            FROM profile_coordinator_projection_commands
            WHERE tenant_id = ?
              AND profile_id = ?
              AND coordinator_sequence = ?
            "#,
            scope.tenant_id().as_str(),
            profile_id.as_str(),
            sqlite_integer_value(sequence)?
        )?
        .first::<ProjectionCommandRow>(None)
        .await
    }
}

#[derive(Deserialize)]
struct ProjectionCommandRow {
    outcome: String,
    projection_json: String,
}

fn verify_replay(
    existing: ProjectionCommandRow,
    outcome: &str,
    projection_json: &str,
) -> Result<CoordinatorProjectionWrite> {
    if existing.outcome == outcome && existing.projection_json == projection_json {
        return Ok(CoordinatorProjectionWrite::Replayed);
    }
    Err(Error::RustError(
        "coordinator projection sequence conflict".to_owned(),
    ))
}

fn sqlite_integer_value(value: u64) -> Result<i64> {
    i64::try_from(value).map_err(|_| Error::RustError("value exceeds SQLite INTEGER".to_owned()))
}

fn sqlite_unsigned(value: i64) -> Result<u64> {
    u64::try_from(value).map_err(|_| Error::RustError("negative SQLite INTEGER".to_owned()))
}

#[cfg(test)]
mod tests {
    use super::{CoordinatorProjectionOutcome, sqlite_integer_value, sqlite_unsigned};

    #[test]
    fn outcome_values_are_stable() {
        assert_eq!(
            CoordinatorProjectionOutcome::LeaseClaimed.database_value(),
            "lease_claimed"
        );
        assert_eq!(
            CoordinatorProjectionOutcome::TimedOut.database_value(),
            "timed_out"
        );
    }

    #[test]
    fn sqlite_conversion_rejects_overflow_and_negative_values() {
        assert!(sqlite_integer_value(u64::MAX).is_err());
        assert!(sqlite_unsigned(-1).is_err());
        assert_eq!(sqlite_unsigned(7).expect("positive value"), 7);
    }
}
