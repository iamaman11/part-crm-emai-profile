# Governed write evidence fixture notes

Step 4 lifecycle and ACL mutations are exposed through typed Cloudflare adapter
repositories that submit the aggregate change, idempotency record, sanitized
audit event and outbox event in one D1 batch. Optimistic preconditions are
validated by command-table triggers so a stale version aborts the full batch.

Invitation acceptance uses a separate verified-identity adapter because the
invited principal does not have an active membership until that same atomic
batch commits. Historical profile/client assignments are never authorization
grants.
