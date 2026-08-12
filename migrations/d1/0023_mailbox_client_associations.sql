-- Pre-2J Batch B: explicit mailbox-to-Client relationship authority.
-- Existing mailbox bindings are intentionally NOT backfilled. Absence of a state
-- row means never-associated/unassigned with relationship version 0.

CREATE TABLE mailbox_client_association_state (
    tenant_id TEXT NOT NULL,
    binding_id TEXT NOT NULL,
    client_id TEXT,
    version INTEGER NOT NULL CHECK(version >= 1),
    updated_by_actor_id TEXT NOT NULL,
    updated_at_ms INTEGER NOT NULL CHECK(updated_at_ms >= 0),
    PRIMARY KEY (tenant_id, binding_id),
    FOREIGN KEY (tenant_id, binding_id)
        REFERENCES mailbox_bindings(tenant_id, binding_id) ON DELETE RESTRICT,
    FOREIGN KEY (tenant_id, client_id)
        REFERENCES clients(tenant_id, client_id) ON DELETE RESTRICT,
    FOREIGN KEY (tenant_id, updated_by_actor_id)
        REFERENCES memberships(tenant_id, actor_id) ON DELETE RESTRICT
) STRICT;

CREATE INDEX mailbox_client_association_client_lookup
    ON mailbox_client_association_state(tenant_id, client_id, binding_id)
    WHERE client_id IS NOT NULL;

CREATE TABLE mailbox_client_association_history (
    tenant_id TEXT NOT NULL,
    binding_id TEXT NOT NULL,
    version INTEGER NOT NULL CHECK(version >= 1),
    operation TEXT NOT NULL CHECK(operation IN ('BIND', 'REBIND', 'UNBIND')),
    previous_client_id TEXT,
    next_client_id TEXT,
    changed_by_actor_id TEXT NOT NULL,
    changed_at_ms INTEGER NOT NULL CHECK(changed_at_ms >= 0),
    PRIMARY KEY (tenant_id, binding_id, version),
    FOREIGN KEY (tenant_id, binding_id)
        REFERENCES mailbox_bindings(tenant_id, binding_id) ON DELETE RESTRICT,
    FOREIGN KEY (tenant_id, previous_client_id)
        REFERENCES clients(tenant_id, client_id) ON DELETE RESTRICT,
    FOREIGN KEY (tenant_id, next_client_id)
        REFERENCES clients(tenant_id, client_id) ON DELETE RESTRICT,
    FOREIGN KEY (tenant_id, changed_by_actor_id)
        REFERENCES memberships(tenant_id, actor_id) ON DELETE RESTRICT
) STRICT;

CREATE INDEX mailbox_client_association_history_client_lookup
    ON mailbox_client_association_history(tenant_id, next_client_id, changed_at_ms, binding_id)
    WHERE next_client_id IS NOT NULL;

CREATE TABLE mailbox_client_association_commands (
    tenant_id TEXT NOT NULL,
    command_id TEXT NOT NULL,
    command_actor_id TEXT NOT NULL,
    binding_id TEXT NOT NULL,
    expected_relationship_version INTEGER NOT NULL CHECK(expected_relationship_version >= 0),
    next_relationship_version INTEGER NOT NULL CHECK(next_relationship_version >= 1),
    operation TEXT NOT NULL CHECK(operation IN ('BIND', 'REBIND', 'UNBIND')),
    previous_client_id TEXT,
    next_client_id TEXT,
    executed_at_ms INTEGER NOT NULL CHECK(executed_at_ms >= 0),
    PRIMARY KEY (tenant_id, command_id),
    FOREIGN KEY (tenant_id, command_actor_id)
        REFERENCES memberships(tenant_id, actor_id) ON DELETE RESTRICT,
    FOREIGN KEY (tenant_id, binding_id)
        REFERENCES mailbox_bindings(tenant_id, binding_id) ON DELETE RESTRICT,
    FOREIGN KEY (tenant_id, previous_client_id)
        REFERENCES clients(tenant_id, client_id) ON DELETE RESTRICT,
    FOREIGN KEY (tenant_id, next_client_id)
        REFERENCES clients(tenant_id, client_id) ON DELETE RESTRICT
) STRICT;

CREATE TRIGGER mailbox_client_association_command_validate
BEFORE INSERT ON mailbox_client_association_commands
FOR EACH ROW
BEGIN
    SELECT RAISE(ABORT, 'mailbox_client_association_owner_required')
    WHERE NOT EXISTS (
        SELECT 1 FROM memberships
        WHERE tenant_id = NEW.tenant_id
          AND actor_id = NEW.command_actor_id
          AND role = 'TENANT_OWNER'
          AND status = 'ACTIVE'
    );

    SELECT RAISE(ABORT, 'mailbox_client_association_binding_not_executable')
    WHERE NOT EXISTS (
        SELECT 1 FROM mailbox_bindings
        WHERE tenant_id = NEW.tenant_id
          AND binding_id = NEW.binding_id
          AND status = 'ACTIVE'
          AND execution_status = 'ACTIVE'
    );

    SELECT RAISE(ABORT, 'mailbox_client_association_version_mismatch')
    WHERE COALESCE((
        SELECT version FROM mailbox_client_association_state
        WHERE tenant_id = NEW.tenant_id AND binding_id = NEW.binding_id
    ), 0) <> NEW.expected_relationship_version;

    SELECT RAISE(ABORT, 'mailbox_client_association_next_version_invalid')
    WHERE NEW.expected_relationship_version >= 9223372036854775807
       OR NEW.next_relationship_version <> NEW.expected_relationship_version + 1;

    SELECT RAISE(ABORT, 'mailbox_client_association_previous_mismatch')
    WHERE COALESCE((
        SELECT client_id FROM mailbox_client_association_state
        WHERE tenant_id = NEW.tenant_id AND binding_id = NEW.binding_id
    ), '') <> COALESCE(NEW.previous_client_id, '');

    SELECT RAISE(ABORT, 'mailbox_client_association_target_not_active')
    WHERE NEW.next_client_id IS NOT NULL
      AND NOT EXISTS (
        SELECT 1 FROM clients
        WHERE tenant_id = NEW.tenant_id
          AND client_id = NEW.next_client_id
          AND status = 'ACTIVE'
    );

    SELECT RAISE(ABORT, 'mailbox_client_association_invalid_transition')
    WHERE (NEW.operation = 'BIND' AND NOT (
              NEW.previous_client_id IS NULL AND NEW.next_client_id IS NOT NULL
          ))
       OR (NEW.operation = 'REBIND' AND NOT (
              NEW.previous_client_id IS NOT NULL
              AND NEW.next_client_id IS NOT NULL
              AND NEW.previous_client_id <> NEW.next_client_id
          ))
       OR (NEW.operation = 'UNBIND' AND NOT (
              NEW.previous_client_id IS NOT NULL AND NEW.next_client_id IS NULL
          ));

    SELECT RAISE(ABORT, 'mailbox_client_association_time_regression')
    WHERE EXISTS (
        SELECT 1 FROM mailbox_client_association_state
        WHERE tenant_id = NEW.tenant_id
          AND binding_id = NEW.binding_id
          AND NEW.executed_at_ms < updated_at_ms
    );
END;

CREATE TRIGGER mailbox_client_association_command_apply
AFTER INSERT ON mailbox_client_association_commands
FOR EACH ROW
BEGIN
    INSERT INTO mailbox_client_association_state (
        tenant_id, binding_id, client_id, version, updated_by_actor_id, updated_at_ms
    ) VALUES (
        NEW.tenant_id, NEW.binding_id, NEW.next_client_id,
        NEW.next_relationship_version, NEW.command_actor_id, NEW.executed_at_ms
    )
    ON CONFLICT (tenant_id, binding_id) DO UPDATE SET
        client_id = excluded.client_id,
        version = excluded.version,
        updated_by_actor_id = excluded.updated_by_actor_id,
        updated_at_ms = excluded.updated_at_ms;

    INSERT INTO mailbox_client_association_history (
        tenant_id, binding_id, version, operation,
        previous_client_id, next_client_id, changed_by_actor_id, changed_at_ms
    ) VALUES (
        NEW.tenant_id, NEW.binding_id, NEW.next_relationship_version, NEW.operation,
        NEW.previous_client_id, NEW.next_client_id, NEW.command_actor_id, NEW.executed_at_ms
    );
END;

CREATE TRIGGER mailbox_client_association_state_insert_governed
BEFORE INSERT ON mailbox_client_association_state
FOR EACH ROW
WHEN NOT EXISTS (
    SELECT 1 FROM mailbox_client_association_commands AS command
    WHERE command.tenant_id = NEW.tenant_id
      AND command.binding_id = NEW.binding_id
      AND command.next_relationship_version = NEW.version
      AND command.next_client_id IS NEW.client_id
      AND command.command_actor_id = NEW.updated_by_actor_id
      AND command.executed_at_ms = NEW.updated_at_ms
)
BEGIN
    SELECT RAISE(ABORT, 'mailbox_client_association_state_not_governed');
END;

CREATE TRIGGER mailbox_client_association_state_update_governed
BEFORE UPDATE ON mailbox_client_association_state
FOR EACH ROW
BEGIN
    SELECT RAISE(ABORT, 'mailbox_client_association_state_identity_immutable')
    WHERE NEW.tenant_id <> OLD.tenant_id OR NEW.binding_id <> OLD.binding_id;

    SELECT RAISE(ABORT, 'mailbox_client_association_state_not_governed')
    WHERE NOT EXISTS (
        SELECT 1 FROM mailbox_client_association_commands AS command
        WHERE command.tenant_id = OLD.tenant_id
          AND command.binding_id = OLD.binding_id
          AND command.expected_relationship_version = OLD.version
          AND command.next_relationship_version = NEW.version
          AND command.previous_client_id IS OLD.client_id
          AND command.next_client_id IS NEW.client_id
          AND command.command_actor_id = NEW.updated_by_actor_id
          AND command.executed_at_ms = NEW.updated_at_ms
    );
END;

CREATE TRIGGER mailbox_client_association_state_delete_forbidden
BEFORE DELETE ON mailbox_client_association_state
FOR EACH ROW
BEGIN
    SELECT RAISE(ABORT, 'mailbox_client_association_state_delete_forbidden');
END;

CREATE TRIGGER mailbox_client_association_history_insert_governed
BEFORE INSERT ON mailbox_client_association_history
FOR EACH ROW
WHEN NOT EXISTS (
    SELECT 1 FROM mailbox_client_association_commands AS command
    WHERE command.tenant_id = NEW.tenant_id
      AND command.binding_id = NEW.binding_id
      AND command.next_relationship_version = NEW.version
      AND command.operation = NEW.operation
      AND command.previous_client_id IS NEW.previous_client_id
      AND command.next_client_id IS NEW.next_client_id
      AND command.command_actor_id = NEW.changed_by_actor_id
      AND command.executed_at_ms = NEW.changed_at_ms
)
BEGIN
    SELECT RAISE(ABORT, 'mailbox_client_association_history_not_governed');
END;

CREATE TRIGGER mailbox_client_association_history_update_forbidden
BEFORE UPDATE ON mailbox_client_association_history
FOR EACH ROW
BEGIN
    SELECT RAISE(ABORT, 'mailbox_client_association_history_immutable');
END;

CREATE TRIGGER mailbox_client_association_history_delete_forbidden
BEFORE DELETE ON mailbox_client_association_history
FOR EACH ROW
BEGIN
    SELECT RAISE(ABORT, 'mailbox_client_association_history_immutable');
END;

CREATE TRIGGER mailbox_client_association_commands_update_forbidden
BEFORE UPDATE ON mailbox_client_association_commands
FOR EACH ROW
BEGIN
    SELECT RAISE(ABORT, 'mailbox_client_association_command_immutable');
END;

CREATE TRIGGER mailbox_client_association_commands_delete_forbidden
BEFORE DELETE ON mailbox_client_association_commands
FOR EACH ROW
BEGIN
    SELECT RAISE(ABORT, 'mailbox_client_association_command_immutable');
END;
