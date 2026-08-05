# Security Policy

## Current Status

This repository is in research and executable-foundation development. No release
is currently supported for production use and no production security SLA is
offered.

Browser profiles, cookies, login databases, mailbox/proxy credentials and key
material are credential-equivalent data. Do not attach them to public issues,
pull requests, logs or screenshots.

## Reporting A Vulnerability

Do not create a public issue containing exploit details, credentials, personal
data or profile payload. Use GitHub private vulnerability reporting when enabled,
or contact the repository owner through a private established channel.

A report should include only the minimum safe information:

- affected commit/version and component;
- impact and required preconditions;
- safe reproduction using synthetic data;
- whether any real credential or personal data may have been exposed;
- proposed mitigation if known.

Never send raw cookies, tokens, private keys, mailbox content or a real profile
archive as proof.

## Credential Incidents

A credential found in source or history is considered compromised. Required
response is revoke/rotate, usage review, impact inventory and safe incident
record. Deleting or rewriting a Git line alone is not remediation.

## Security Gates

Production promotion is blocked until the repository has evidence for:

- accepted key hierarchy and clean-environment recovery;
- stale-writer rejection and device revoke;
- archive corruption/path traversal rejection;
- tenant/IDOR negative tests;
- signed Bridge/runtime update and rollback;
- privacy, retention, support access and incident runbooks.

See `docs/THREAT_MODEL.md`, `docs/DATA_CLASSIFICATION.md` and
`docs/TEST_EVIDENCE_INDEX.md`.
