#!/usr/bin/env python3
"""Canonical D3 compatibility entrypoint after Architecture Re-baseline v3 AR-2.

The accepted pre-AR-2 promotion implementation is preserved byte-for-byte in
`_mailbox_secret_resolver_promotion_core.py`. This module keeps the accepted D3
verifier surface available through the canonical path, while adding one narrow
AR-2 interlock: the superseded legacy production lane is unavailable until PC-1
is separately authorized after AR-17 under the AR-11 release-set model.
"""

from __future__ import annotations

import sys
from collections.abc import Sequence

import _mailbox_secret_resolver_promotion_core as core

AR2_LEGACY_PRODUCTION_DISABLED = True
AR2_PRODUCTION_AUTHORITY = "PC-1_AFTER_AR-17_USING_AR-11_RELEASE_SET"
ENVIRONMENT_GATED_COMMANDS = frozenset({"github-preflight", "prepare", "attest"})

# Preserve the accepted D3 verifier surface at the canonical module path. These
# are aliases to the byte-for-byte accepted implementation, not replacement
# implementations. Permanent D3 checks intentionally verify that these safety
# primitives and their policy messages remain reachable from this entrypoint.
require_mode_0600 = core.require_mode_0600
render_resolver_config = core.render_resolver_config
render_control_config = core.render_control_config
validate_release_identities = core.validate_release_identities
validate_staging_evidence_artifact = core.validate_staging_evidence_artifact
validate_deployment_closures = core.validate_deployment_closures

ACCEPTED_D3_POLICY_MESSAGES = (
    "cross-environment-identical secret documents are forbidden",
    "caller-auth secret must match both Workers",
    "Production same-bits artifacts match immutable passed staging evidence",
)


def requested_environment(argv: Sequence[str]) -> str | None:
    if len(argv) < 2 or argv[1] not in ENVIRONMENT_GATED_COMMANDS:
        return None
    try:
        index = argv.index("--environment", 2)
    except ValueError:
        return None
    value_index = index + 1
    return argv[value_index] if value_index < len(argv) else None


def enforce_ar2_environment_gate(argv: Sequence[str]) -> None:
    environment = requested_environment(argv)
    if environment == "production":
        raise core.PromotionError(
            "legacy D3 production promotion is disabled by Architecture Re-baseline v3 AR-2; "
            "production mutation remains forbidden through AR-17 and future production promotion "
            f"requires {AR2_PRODUCTION_AUTHORITY}"
        )


def self_test_gate() -> None:
    staging = ["promotion", "github-preflight", "--environment", "staging"]
    enforce_ar2_environment_gate(staging)
    production = ["promotion", "github-preflight", "--environment", "production"]
    try:
        enforce_ar2_environment_gate(production)
    except core.PromotionError:
        return
    raise core.PromotionError("AR-2 legacy production negative fixture unexpectedly passed")


def main() -> int:
    if len(sys.argv) >= 2 and sys.argv[1] == "self-test":
        self_test_gate()
    enforce_ar2_environment_gate(sys.argv)
    return core.main()


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except core.PromotionError as error:
        raise SystemExit(f"mailbox resolver promotion rejected: {error}") from error
