-- #849: remove the unconditional six-minute Delete delay.
-- A signed R2 PUT cannot be revoked; keep its exact append-only reservation
-- fence until expires_at_ms. However the previous requested_at_ms + 360000
-- minimum punished even empty DRAFT profiles with no issued PUT capability.
--
-- The earlier migration 0041 established that signed URLs are recorded BEFORE
-- disclosure, and new reservations require a writable DRAFT/READY version.
-- DELETE atomically transitions to DELETING, preventing fresh reservations.
-- No new table, worker, queue, browser controller or second deletion path.
-- Existing exact R2 object receipts and owner/version checks remain intact.

DROP TRIGGER profile_delete_object_receipt_admit;
CREATE TRIGGER profile_delete_object_receipt_admit
BEFORE INSERT ON profile_delete_object_receipts
FOR EACH ROW
BEGIN
    -- The append-only reservation ledger holds every issued signed PUT.
    -- If no capability was issued, there is nothing to wait for.
    SELECT RAISE(ABORT, 'profile_delete_presigned_put_not_expired')
    WHERE EXISTS (
        SELECT 1 FROM profile_generation_upload_reservations AS reservation
        WHERE reservation.tenant_id = NEW.tenant_id
          AND reservation.profile_id = NEW.profile_id
          AND reservation.expires_at_ms > NEW.deleted_at_ms
    );
    SELECT RAISE(ABORT, 'profile_delete_exact_object_not_frozen')
    WHERE NOT EXISTS (
        SELECT 1 FROM profile_delete_objects AS object
        JOIN browser_profiles AS profile
          ON profile.tenant_id = object.tenant_id
         AND profile.profile_id = object.profile_id
        WHERE object.tenant_id = NEW.tenant_id
          AND object.profile_id = NEW.profile_id
          AND object.generation_id = NEW.generation_id
          AND object.object_key = NEW.object_key
          AND object.metadata_digest = NEW.metadata_digest
          AND object.container_digest = NEW.container_digest
          AND profile.status = 'DELETING'
          AND NEW.deleted_at_ms >= profile.updated_at_ms
    );
END;

DROP TRIGGER profile_delete_complete_admit;
CREATE TRIGGER profile_delete_complete_admit
BEFORE INSERT ON profile_delete_completion_commands
FOR EACH ROW
BEGIN
    SELECT RAISE(ABORT, 'profile_delete_complete_owner_required')
    WHERE NOT EXISTS (
        SELECT 1 FROM memberships
        WHERE tenant_id = NEW.tenant_id
          AND actor_id = NEW.command_actor_id
          AND role = 'TENANT_OWNER'
          AND status = 'ACTIVE'
    );
    SELECT RAISE(ABORT, 'profile_delete_complete_state_mismatch')
    WHERE NOT EXISTS (
        SELECT 1 FROM browser_profiles AS profile
        JOIN profile_delete_commands AS command
          ON command.tenant_id = profile.tenant_id
         AND command.profile_id = profile.profile_id
        WHERE profile.tenant_id = NEW.tenant_id
          AND profile.profile_id = NEW.profile_id
          AND profile.status = 'DELETING'
          AND profile.version = NEW.expected_profile_version
          AND NEW.completed_at_ms >= profile.updated_at_ms
    );
    -- The append-only reservation ledger holds every issued signed PUT.
    -- If no capability was issued, there is nothing to wait for.
    SELECT RAISE(ABORT, 'profile_delete_presigned_put_not_expired')
    WHERE EXISTS (
        SELECT 1 FROM profile_generation_upload_reservations AS reservation
        WHERE reservation.tenant_id = NEW.tenant_id
          AND reservation.profile_id = NEW.profile_id
          AND reservation.expires_at_ms > NEW.completed_at_ms
    );
    SELECT RAISE(ABORT, 'profile_delete_objects_remain')
    WHERE EXISTS (
        SELECT 1 FROM profile_delete_objects AS object
        WHERE object.tenant_id = NEW.tenant_id
          AND object.profile_id = NEW.profile_id
          AND NOT EXISTS (
              SELECT 1 FROM profile_delete_object_receipts AS receipt
              WHERE receipt.tenant_id = object.tenant_id
                AND receipt.profile_id = object.profile_id
                AND receipt.generation_id = object.generation_id
                AND receipt.object_key = object.object_key
                AND receipt.metadata_digest = object.metadata_digest
                AND receipt.container_digest = object.container_digest
          )
    );
END;
