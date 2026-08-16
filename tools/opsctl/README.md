# opsctl — AR-6 read-only foundation

`opsctl` is the project-specific Rust operational interface introduced by Architecture Re-baseline v3 AR-6.

AR-6 intentionally exposes only:

```text
opsctl doctor
opsctl status
opsctl inventory
```

`status` and `inventory` return the existing canonical JSON authorities directly. `doctor` runs the existing canonical architecture and Python-estate validators in read-only `--check` mode and returns a versioned JSON result.

This package is a standalone Cargo workspace so introducing the operator tool does not widen the production runtime dependency graph. It has no third-party Rust dependencies and no Cloudflare/provider/database client.

Mutation commands are forbidden in AR-6. GitHub Actions/Environments remain orchestration and credential boundaries; Wrangler/provider APIs remain the actual mutation mechanisms when a later owning slice explicitly authorizes them.
