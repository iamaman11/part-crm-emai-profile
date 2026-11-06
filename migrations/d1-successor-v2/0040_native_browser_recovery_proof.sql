-- C09: disposable-staging native browser recovery proof, migrate-before-code.
-- Legacy schema0038 metadata-only receipts remain immutable with NULL witnesses
-- and cannot grant protected LKG status or authorise any R2 DELETE.
-- This does NOT store cookie values, URLs, IndexedDB content or encryption keys.
ALTER TABLE profile_generation_working_proofs
    ADD COLUMN native_snapshot_sha256 TEXT
    CHECK(native_snapshot_sha256 IS NULL OR (
        length(native_snapshot_sha256) = 64
        AND native_snapshot_sha256 NOT GLOB '*[^0-9a-f]*'
    ));
ALTER TABLE profile_generation_working_proofs
    ADD COLUMN native_browser_witness_sha256 TEXT
    CHECK(native_browser_witness_sha256 IS NULL OR (
        length(native_browser_witness_sha256) = 64
        AND native_browser_witness_sha256 NOT GLOB '*[^0-9a-f]*'
    ));

-- A direct D1 caller cannot use an old metadata-only WORKING row to retire
-- even a disposable history object. The original schema0039 authorization
-- and live-grant/job/fence validations remain mandatory as well.
CREATE TRIGGER profile_generation_native_retirement_proof_required
BEFORE INSERT ON profile_generation_retirement_intents
FOR EACH ROW WHEN NOT EXISTS (
    SELECT 1 FROM profile_generation_working_proofs AS proof
    WHERE proof.tenant_id = NEW.tenant_id
      AND proof.profile_id = NEW.profile_id
      AND proof.generation_id = NEW.retained_working_generation_id
      AND proof.native_snapshot_sha256 IS NOT NULL
      AND proof.native_browser_witness_sha256 IS NOT NULL
)
BEGIN
    SELECT RAISE(ABORT, 'retirement_requires_native_cold_recovery_proof');
END;
