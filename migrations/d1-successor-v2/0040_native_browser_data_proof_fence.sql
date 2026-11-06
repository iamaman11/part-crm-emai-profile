-- #830 / G5-C09. Additive fence to existing D1 WORKING proof owner.
--
-- Legacy 0038 receipts prove only signed metadata/actor/device/runtime, not
-- that Firefox restored cookies, IndexedDB, localStorage or tabs. Retaining
-- them preserves audit history, but they cannot be treated as proven WORKING
-- or as a basis for deleting older encrypted R2 generations.
--
-- Version 1 is RESERVED for a separately reviewed end-to-end pre-Save
-- expectation + cold R2 browser-data readback with server-authenticated
-- actor/device/generation/fence/key and anti-replay. Until its full producer,
-- consumer and negative tests exist, even direct D1 v1 insertion is rejected.
-- This migration NEVER changes/deletes existing rows, files, R2 or keys.

ALTER TABLE profile_generation_working_proofs
    ADD COLUMN browser_data_proof_version INTEGER NOT NULL DEFAULT 0
    CHECK(browser_data_proof_version IN (0, 1));

ALTER TABLE profile_generation_working_proofs
    ADD COLUMN browser_data_proof_digest TEXT DEFAULT NULL
    CHECK(browser_data_proof_digest IS NULL OR (
        length(browser_data_proof_digest) = 64
        AND browser_data_proof_digest NOT GLOB '*[^0-9a-f]*'
    ));

CREATE TRIGGER profile_generation_native_data_proof_unimplemented
BEFORE INSERT ON profile_generation_working_proofs
FOR EACH ROW
WHEN NEW.browser_data_proof_version <> 0
  OR NEW.browser_data_proof_digest IS NOT NULL
BEGIN
    SELECT RAISE(ABORT, 'native_browser_data_proof_not_implemented');
END;
