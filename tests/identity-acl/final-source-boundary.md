# Final Step 4 source boundary

The accepted Step 4 source head must contain only permanent workflows and must
pass the exact four-job Quality Gate. Lifecycle and ACL writes are confined to
typed governed D1 adapters; version conflicts abort the aggregate,
idempotency, audit and outbox envelope together. Tenant-scoped reads conceal
missing, foreign and unauthorized resources using the same neutral result
shape.
