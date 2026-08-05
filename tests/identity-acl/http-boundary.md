# Versioned HTTP boundary expectations

All Step 4 owner/member routes live under `/api/v1`. The Worker verifies the
external identity before resolving an active tenant membership. Owner-only
mutations are delegated to governed D1 command repositories; client and profile
reads delegate visibility to tenant-scoped projections that grant owner access
or require an explicit grant for members.

Missing, foreign, unauthenticated, suspended, revoked and insufficiently granted
resources use the same neutral problem shape where disclosure is forbidden.
The accepted v1 baseline remains immutable; these routes and schemas are
additive.
