-- PAS-2 Transaction B: the browser-supplied request digest protocol is retired.
-- Existing pre-production replay evidence is preserved byte-for-byte while ownership
-- changes to the server-owned PayloadFingerprint semantic.
ALTER TABLE idempotency_records
    RENAME COLUMN request_digest TO payload_fingerprint;
