-- Fail closed against direct writes that bypass mailbox command journals.

CREATE TRIGGER mailbox_bindings_insert_governed
BEFORE INSERT ON mailbox_bindings
FOR EACH ROW
BEGIN
    SELECT RAISE(ABORT, 'mailbox_binding_insert_not_governed')
    WHERE NOT EXISTS (
        SELECT 1
        FROM mailbox_binding_create_commands AS command
        WHERE command.tenant_id = NEW.tenant_id
          AND command.binding_id = NEW.binding_id
          AND command.provider = NEW.provider
          AND command.secret_handle = NEW.secret_handle
          AND command.command_actor_id = NEW.created_by_actor_id
          AND command.command_actor_id = NEW.updated_by_actor_id
          AND command.executed_at_ms = NEW.created_at_ms
          AND command.executed_at_ms = NEW.updated_at_ms
    );
END;

CREATE TRIGGER mailbox_bindings_update_governed
BEFORE UPDATE ON mailbox_bindings
FOR EACH ROW
BEGIN
    SELECT RAISE(ABORT, 'mailbox_binding_identity_immutable')
    WHERE NEW.tenant_id <> OLD.tenant_id
       OR NEW.binding_id <> OLD.binding_id
       OR NEW.provider <> OLD.provider
       OR NEW.secret_handle <> OLD.secret_handle
       OR NEW.created_by_actor_id <> OLD.created_by_actor_id
       OR NEW.created_at_ms <> OLD.created_at_ms;

    SELECT RAISE(ABORT, 'mailbox_binding_update_not_governed')
    WHERE NOT EXISTS (
        SELECT 1
        FROM mailbox_binding_revoke_commands AS command
        WHERE command.tenant_id = OLD.tenant_id
          AND command.binding_id = OLD.binding_id
          AND command.expected_binding_version = OLD.version
          AND command.command_actor_id = NEW.updated_by_actor_id
          AND command.executed_at_ms = NEW.updated_at_ms
    );

    SELECT RAISE(ABORT, 'mailbox_binding_update_invalid_transition')
    WHERE OLD.status <> 'ACTIVE'
       OR NEW.status <> 'REVOKED'
       OR NEW.version <> OLD.version + 1;
END;

CREATE TRIGGER mailbox_bindings_delete_governed
BEFORE DELETE ON mailbox_bindings
FOR EACH ROW
BEGIN
    SELECT RAISE(ABORT, 'mailbox_binding_delete_not_governed');
END;

CREATE TRIGGER mailbox_jobs_insert_governed
BEFORE INSERT ON mailbox_jobs
FOR EACH ROW
BEGIN
    SELECT RAISE(ABORT, 'mailbox_job_insert_not_governed')
    WHERE NOT EXISTS (
        SELECT 1
        FROM mailbox_job_create_commands AS command
        WHERE command.tenant_id = NEW.tenant_id
          AND command.binding_id = NEW.binding_id
          AND command.job_id = NEW.job_id
          AND command.cursor IS NEW.cursor
          AND command.scheduled_at_ms = NEW.next_run_at_ms
          AND command.max_attempts = NEW.max_attempts
          AND command.command_actor_id = NEW.created_by_actor_id
          AND command.command_actor_id = NEW.updated_by_actor_id
          AND command.executed_at_ms = NEW.created_at_ms
          AND command.executed_at_ms = NEW.updated_at_ms
    );

    SELECT RAISE(ABORT, 'mailbox_job_insert_invalid_state')
    WHERE NEW.status <> 'PENDING'
       OR NEW.attempt <> 0
       OR NEW.bounded_item_count <> 0
       OR NEW.version <> 1;
END;

CREATE TRIGGER mailbox_jobs_update_governed
BEFORE UPDATE ON mailbox_jobs
FOR EACH ROW
BEGIN
    SELECT RAISE(ABORT, 'mailbox_job_identity_immutable')
    WHERE NEW.tenant_id <> OLD.tenant_id
       OR NEW.binding_id <> OLD.binding_id
       OR NEW.job_id <> OLD.job_id
       OR NEW.max_attempts <> OLD.max_attempts
       OR NEW.created_by_actor_id <> OLD.created_by_actor_id
       OR NEW.created_at_ms <> OLD.created_at_ms;

    SELECT RAISE(ABORT, 'mailbox_job_update_not_governed')
    WHERE NOT EXISTS (
        SELECT 1
        FROM mailbox_job_run_commands AS command
        WHERE command.tenant_id = OLD.tenant_id
          AND command.binding_id = OLD.binding_id
          AND command.job_id = OLD.job_id
          AND command.expected_job_version = OLD.version
          AND command.outcome_status = NEW.status
          AND command.provider_status = NEW.provider_status
          AND command.bounded_item_count = NEW.bounded_item_count
          AND command.command_actor_id = NEW.updated_by_actor_id
          AND command.executed_at_ms = NEW.updated_at_ms
          AND (
              (command.outcome_status = 'SUCCEEDED' AND command.next_cursor IS NEW.cursor)
              OR (command.outcome_status <> 'SUCCEEDED' AND NEW.cursor IS OLD.cursor)
          )
          AND (
              (command.outcome_status = 'RETRY_PENDING' AND command.retry_at_ms = NEW.next_run_at_ms)
              OR (command.outcome_status <> 'RETRY_PENDING' AND NEW.next_run_at_ms = OLD.next_run_at_ms)
          )
    );

    SELECT RAISE(ABORT, 'mailbox_job_update_invalid_transition')
    WHERE OLD.status NOT IN ('PENDING', 'RETRY_PENDING')
       OR NEW.status NOT IN ('SUCCEEDED', 'RETRY_PENDING', 'FAILED')
       OR NEW.attempt <> OLD.attempt + 1
       OR NEW.version <> OLD.version + 2;
END;

CREATE TRIGGER mailbox_jobs_delete_governed
BEFORE DELETE ON mailbox_jobs
FOR EACH ROW
BEGIN
    SELECT RAISE(ABORT, 'mailbox_job_delete_not_governed');
END;
