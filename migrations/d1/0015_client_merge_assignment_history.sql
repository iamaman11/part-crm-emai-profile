-- Phase 2C: deterministic client merge plus one-way governed assignment history.
-- Assignment remains business/history linkage only and is never an authorization source.

CREATE TABLE client_merges (
    tenant_id TEXT NOT NULL,
    source_client_id TEXT NOT NULL,
    target_client_id TEXT NOT NULL,
    source_version_before INTEGER NOT NULL CHECK(source_version_before >= 1),
    source_version_after INTEGER NOT NULL CHECK(source_version_after = source_version_before + 1),
    target_version_observed INTEGER NOT NULL CHECK(target_version_observed >= 1),
    merged_by_actor_id TEXT NOT NULL,
    merged_at_ms INTEGER NOT NULL CHECK(merged_at_ms >= 0),
    reason TEXT NOT NULL CHECK(length(trim(reason)) BETWEEN 1 AND 500),
    PRIMARY KEY (tenant_id, source_client_id),
    CHECK(source_client_id <> target_client_id),
    FOREIGN KEY (tenant_id, source_client_id)
        REFERENCES clients(tenant_id, client_id) ON DELETE RESTRICT,
    FOREIGN KEY (tenant_id, target_client_id)
        REFERENCES clients(tenant_id, client_id) ON DELETE RESTRICT,
    FOREIGN KEY (tenant_id, merged_by_actor_id)
        REFERENCES memberships(tenant_id, actor_id) ON DELETE RESTRICT
) STRICT;

CREATE INDEX client_merge_target_history
    ON client_merges(tenant_id, target_client_id, merged_at_ms, source_client_id);

CREATE TABLE client_merge_commands (
    tenant_id TEXT NOT NULL,
    command_id TEXT NOT NULL,
    command_actor_id TEXT NOT NULL,
    source_client_id TEXT NOT NULL,
    target_client_id TEXT NOT NULL,
    expected_source_version INTEGER NOT NULL CHECK(expected_source_version >= 1),
    expected_target_version INTEGER NOT NULL CHECK(expected_target_version >= 1),
    reason TEXT NOT NULL CHECK(length(trim(reason)) BETWEEN 1 AND 500),
    executed_at_ms INTEGER NOT NULL CHECK(executed_at_ms >= 0),
    PRIMARY KEY (tenant_id, command_id),
    CHECK(source_client_id <> target_client_id),
    FOREIGN KEY (tenant_id, command_actor_id)
        REFERENCES memberships(tenant_id, actor_id) ON DELETE RESTRICT,
    FOREIGN KEY (tenant_id, source_client_id)
        REFERENCES clients(tenant_id, client_id) ON DELETE RESTRICT,
    FOREIGN KEY (tenant_id, target_client_id)
        REFERENCES clients(tenant_id, client_id) ON DELETE RESTRICT
) STRICT;

CREATE TRIGGER client_merge_command_validate
BEFORE INSERT ON client_merge_commands
FOR EACH ROW
BEGIN
    SELECT RAISE(ABORT, 'client_merge_owner_required')
    WHERE NOT EXISTS (
        SELECT 1 FROM memberships
        WHERE tenant_id = NEW.tenant_id
          AND actor_id = NEW.command_actor_id
          AND role = 'TENANT_OWNER'
          AND status = 'ACTIVE'
    );
    SELECT RAISE(ABORT, 'client_merge_source_version_or_state_mismatch')
    WHERE NOT EXISTS (
        SELECT 1 FROM clients
        WHERE tenant_id = NEW.tenant_id
          AND client_id = NEW.source_client_id
          AND version = NEW.expected_source_version
          AND status = 'ACTIVE'
    );
    SELECT RAISE(ABORT, 'client_merge_target_version_or_state_mismatch')
    WHERE NOT EXISTS (
        SELECT 1 FROM clients
        WHERE tenant_id = NEW.tenant_id
          AND client_id = NEW.target_client_id
          AND version = NEW.expected_target_version
          AND status = 'ACTIVE'
    );
    SELECT RAISE(ABORT, 'client_merge_time_regression')
    WHERE EXISTS (
        SELECT 1 FROM clients
        WHERE tenant_id = NEW.tenant_id
          AND client_id IN (NEW.source_client_id, NEW.target_client_id)
          AND updated_at_ms > NEW.executed_at_ms
    );
    SELECT RAISE(ABORT, 'client_merge_contact_time_regression')
    WHERE EXISTS (
        SELECT 1 FROM client_contact_points
        WHERE tenant_id = NEW.tenant_id
          AND client_id = NEW.source_client_id
          AND status = 'ACTIVE'
          AND updated_at_ms > NEW.executed_at_ms
    );
    SELECT RAISE(ABORT, 'client_merge_active_assignment_requires_reassignment')
    WHERE EXISTS (
        SELECT 1 FROM profile_client_assignments
        WHERE tenant_id = NEW.tenant_id
          AND client_id = NEW.source_client_id
          AND closed_at_ms IS NULL
    );
END;

CREATE TRIGGER client_merge_status_requires_command
BEFORE UPDATE OF status ON clients
FOR EACH ROW
WHEN OLD.status <> 'MERGED' AND NEW.status = 'MERGED'
BEGIN
    SELECT RAISE(ABORT, 'client_merge_not_governed')
    WHERE NOT EXISTS (
        SELECT 1 FROM client_merge_commands
        WHERE tenant_id = OLD.tenant_id
          AND source_client_id = OLD.client_id
          AND expected_source_version = OLD.version
          AND command_actor_id = NEW.updated_by_actor_id
          AND executed_at_ms = NEW.updated_at_ms
    );
END;

CREATE TRIGGER client_merged_source_cannot_resurrect
BEFORE UPDATE OF status ON clients
FOR EACH ROW
WHEN OLD.status = 'MERGED' AND NEW.status <> 'MERGED'
BEGIN
    SELECT RAISE(ABORT, 'client_merged_source_immutable');
END;

CREATE TRIGGER client_merge_record_requires_command
BEFORE INSERT ON client_merges
FOR EACH ROW
BEGIN
    SELECT RAISE(ABORT, 'client_merge_record_not_governed')
    WHERE NOT EXISTS (
        SELECT 1 FROM client_merge_commands
        WHERE tenant_id = NEW.tenant_id
          AND source_client_id = NEW.source_client_id
          AND target_client_id = NEW.target_client_id
          AND expected_source_version = NEW.source_version_before
          AND expected_target_version = NEW.target_version_observed
          AND command_actor_id = NEW.merged_by_actor_id
          AND executed_at_ms = NEW.merged_at_ms
          AND trim(reason) = trim(NEW.reason)
    );
    SELECT RAISE(ABORT, 'client_merge_record_source_mismatch')
    WHERE NOT EXISTS (
        SELECT 1 FROM clients
        WHERE tenant_id = NEW.tenant_id
          AND client_id = NEW.source_client_id
          AND status = 'MERGED'
          AND version = NEW.source_version_after
    );
    SELECT RAISE(ABORT, 'client_merge_record_target_mismatch')
    WHERE NOT EXISTS (
        SELECT 1 FROM clients
        WHERE tenant_id = NEW.tenant_id
          AND client_id = NEW.target_client_id
          AND status = 'ACTIVE'
          AND version = NEW.target_version_observed
    );
END;

CREATE TRIGGER client_merge_record_immutable_update
BEFORE UPDATE ON client_merges
FOR EACH ROW
BEGIN
    SELECT RAISE(ABORT, 'client_merge_record_immutable');
END;

CREATE TRIGGER client_merge_record_immutable_delete
BEFORE DELETE ON client_merges
FOR EACH ROW
BEGIN
    SELECT RAISE(ABORT, 'client_merge_record_delete_forbidden');
END;

CREATE TRIGGER client_merge_command_apply
AFTER INSERT ON client_merge_commands
FOR EACH ROW
BEGIN
    UPDATE clients
    SET status = 'MERGED',
        version = version + 1,
        updated_by_actor_id = NEW.command_actor_id,
        updated_at_ms = NEW.executed_at_ms
    WHERE tenant_id = NEW.tenant_id
      AND client_id = NEW.source_client_id;

    UPDATE client_contact_points
    SET status = 'ARCHIVED',
        updated_by_actor_id = NEW.command_actor_id,
        updated_at_ms = NEW.executed_at_ms
    WHERE tenant_id = NEW.tenant_id
      AND client_id = NEW.source_client_id
      AND status = 'ACTIVE';

    DELETE FROM client_grants
    WHERE tenant_id = NEW.tenant_id
      AND client_id = NEW.source_client_id;

    INSERT INTO client_merges (
        tenant_id, source_client_id, target_client_id,
        source_version_before, source_version_after, target_version_observed,
        merged_by_actor_id, merged_at_ms, reason
    ) VALUES (
        NEW.tenant_id, NEW.source_client_id, NEW.target_client_id,
        NEW.expected_source_version, NEW.expected_source_version + 1, NEW.expected_target_version,
        NEW.command_actor_id, NEW.executed_at_ms, trim(NEW.reason)
    );
END;

-- Phase 2C hardens the already-accepted assignment command path rather than creating
-- another persistence model. A reassignment must differ from the currently active client.
CREATE TRIGGER profile_assignment_phase2c_validate
BEFORE INSERT ON profile_assignment_commands
FOR EACH ROW
BEGIN
    SELECT RAISE(ABORT, 'profile_assignment_same_client')
    WHERE EXISTS (
        SELECT 1 FROM profile_client_assignments
        WHERE tenant_id = NEW.tenant_id
          AND profile_id = NEW.profile_id
          AND client_id = NEW.client_id
          AND closed_at_ms IS NULL
    );
    SELECT RAISE(ABORT, 'profile_assignment_time_regression')
    WHERE EXISTS (
        SELECT 1 FROM profile_client_assignments
        WHERE tenant_id = NEW.tenant_id
          AND profile_id = NEW.profile_id
          AND closed_at_ms IS NULL
          AND assigned_at_ms > NEW.executed_at_ms
    );
END;

CREATE TRIGGER profile_assignment_history_insert_guard
BEFORE INSERT ON profile_client_assignments
FOR EACH ROW
BEGIN
    SELECT RAISE(ABORT, 'profile_assignment_not_governed')
    WHERE NOT EXISTS (
        SELECT 1 FROM profile_assignment_commands
        WHERE tenant_id = NEW.tenant_id
          AND assignment_id = NEW.assignment_id
          AND profile_id = NEW.profile_id
          AND client_id = NEW.client_id
          AND command_actor_id = NEW.assigned_by_actor_id
          AND executed_at_ms = NEW.assigned_at_ms
          AND trim(reason) = trim(NEW.reason)
    );
END;

CREATE TRIGGER profile_assignment_history_update_guard
BEFORE UPDATE ON profile_client_assignments
FOR EACH ROW
BEGIN
    SELECT RAISE(ABORT, 'profile_assignment_identity_immutable')
    WHERE NEW.tenant_id <> OLD.tenant_id
       OR NEW.assignment_id <> OLD.assignment_id
       OR NEW.profile_id <> OLD.profile_id
       OR NEW.client_id <> OLD.client_id
       OR NEW.assigned_by_actor_id <> OLD.assigned_by_actor_id
       OR NEW.assigned_at_ms <> OLD.assigned_at_ms
       OR NEW.reason <> OLD.reason;
    SELECT RAISE(ABORT, 'profile_assignment_closed_history_immutable')
    WHERE OLD.closed_at_ms IS NOT NULL
      AND NEW.closed_at_ms IS NOT OLD.closed_at_ms;
    SELECT RAISE(ABORT, 'profile_assignment_invalid_close_time')
    WHERE NEW.closed_at_ms IS NOT NULL
      AND NEW.closed_at_ms < OLD.assigned_at_ms;
    SELECT RAISE(ABORT, 'profile_assignment_close_not_governed')
    WHERE OLD.closed_at_ms IS NULL
      AND NEW.closed_at_ms IS NOT NULL
      AND NOT EXISTS (
        SELECT 1 FROM profile_assignment_commands
        WHERE tenant_id = OLD.tenant_id
          AND profile_id = OLD.profile_id
          AND executed_at_ms = NEW.closed_at_ms
    );
END;

CREATE TRIGGER profile_assignment_history_delete_guard
BEFORE DELETE ON profile_client_assignments
FOR EACH ROW
BEGIN
    SELECT RAISE(ABORT, 'profile_assignment_delete_forbidden');
END;
