#!/usr/bin/env python3
"""Repository-local D1 fault/recovery proof orchestration.

This harness owns no migration or recovery policy. It composes the existing typed
opsctl owners with pinned local Wrangler proofs and emits one secret-free report.
Remote/provider D1 is intentionally unavailable here.
"""

from __future__ import annotations

import argparse
import json
import os
import shutil
import subprocess
import sys
from pathlib import Path
from typing import Any

SCHEMA_VERSION = 1
DEFAULT_WRANGLER_VERSION = "4.94.0"

# These are evidence anchors only. The Rust owners remain the semantic authority.
TYPED_OWNER_ANCHORS: dict[str, tuple[str, tuple[str, ...]]] = {
    "ledger_noncanonical": (
        "tools/opsctl/src/d1/tests.rs",
        ("fn classifies_exact_prefix_and_noncanonical_ledgers",),
    ),
    "supported_ahead_recovery_window": (
        "tools/opsctl/src/d1/tests.rs",
        (
            "fn known_ahead_schema_distinguishes_rollback_window",
            "fn verify_requires_exact_target",
        ),
    ),
    "expand_old_new_runtime_rollout": (
        "tools/opsctl/src/d1/tests.rs",
        ("fn expand_backfill_and_rollout_order_remain_typed",),
    ),
    "contract_rejection_admission": (
        "tools/opsctl/src/d1/tests.rs",
        ("fn contract_requires_typed_preconditions",),
    ),
    "contract_exact_successor": (
        "tools/opsctl/tests/d1_contract_transition.rs",
        ("fn actual_deployed_context_traverses_separate_sealed_contract_and_current_expand_with_data_preserved",),
    ),
    "transaction_observation_plan_target_drift": (
        "tools/opsctl/src/d1/transaction.rs",
        (
            "fn provider_observation_drift_changes_transaction_identity",
            "fn source_drift_changes_transaction_identity",
            "fn plan_drift_is_rejected_instead_of_silently_replanned",
            "fn target_drift_is_rejected",
        ),
    ),
    "authorization_freshness_and_scope_drift": (
        "tools/opsctl/src/d1/authorization.rs",
        (
            "fn transaction_id_drift_is_rejected",
            "fn provider_effect_widening_is_rejected",
            "fn phase_drift_is_rejected",
            "fn expired_authorization_is_rejected",
            "fn forged_freshness_deadline_is_rejected",
        ),
    ),
    "executor_checkout_identity_drift": (
        "tools/opsctl/src/d1/executor_admission.rs",
        (
            "fn source_checkout_drift_is_rejected",
            "fn tree_checkout_drift_is_rejected",
            "fn target_drift_is_rejected",
            "fn transaction_id_drift_is_rejected",
            "fn component_drift_is_rejected",
        ),
    ),
    "stale_or_unknown_target_fence": (
        "tools/opsctl/src/d1/execution_control.rs",
        (
            "fn newer_same_target_fence_rejects_stale_executor",
            "fn unknown_history_fails_closed_before_write",
        ),
    ),
    "runner_interruption_requires_recovery": (
        "tools/opsctl/src/d1/execution_control.rs",
        ("fn mutation_started_failure_requires_recovery_terminal",),
    ),
}


class HarnessError(RuntimeError):
    pass


class Harness:
    def __init__(self, root: Path, wrangler_version: str, report_path: Path) -> None:
        self.root = root.resolve()
        self.wrangler_version = wrangler_version
        self.report_path = report_path
        self.scratch = self.root / ".wrangler" / "tx5-fault-recovery-harness"
        self.artifacts = self.root / "artifacts" / "d1-evolution"
        self.scenarios: list[dict[str, Any]] = []
        self.env = os.environ.copy()
        self.env["WRANGLER_SEND_METRICS"] = "false"
        for key in (
            "CLOUDFLARE_API_TOKEN",
            "CLOUDFLARE_OBSERVE_API_TOKEN",
        ):
            if self.env.get(key):
                raise HarnessError(
                    f"{key} must not be exposed to the local TX-5 fault/recovery harness"
                )
            self.env.pop(key, None)

    def record(self, scenario_id: str, owner: str, kind: str, **details: Any) -> None:
        item: dict[str, Any] = {
            "scenario_id": scenario_id,
            "status": "PASS",
            "owner": owner,
            "proof_kind": kind,
            "provider_mutation": False,
            "production_mutation": False,
        }
        if details:
            item["details"] = details
        self.scenarios.append(item)

    def run(
        self,
        command: list[str],
        *,
        expect_success: bool = True,
        capture: bool = True,
    ) -> subprocess.CompletedProcess[str]:
        result = subprocess.run(
            command,
            cwd=self.root,
            env=self.env,
            text=True,
            stdout=subprocess.PIPE if capture else None,
            stderr=subprocess.PIPE if capture else None,
            check=False,
        )
        succeeded = result.returncode == 0
        if succeeded != expect_success:
            stdout = (result.stdout or "")[-4000:]
            stderr = (result.stderr or "")[-4000:]
            expectation = "success" if expect_success else "failure"
            raise HarnessError(
                f"command did not produce expected {expectation}: {' '.join(command)}\n"
                f"exit={result.returncode}\nstdout_tail={stdout}\nstderr_tail={stderr}"
            )
        return result

    def opsctl(self, *arguments: str) -> subprocess.CompletedProcess[str]:
        return self.run(
            [
                "cargo",
                "run",
                "--locked",
                "--quiet",
                "--manifest-path",
                "tools/opsctl/Cargo.toml",
                "--",
                "--root",
                ".",
                *arguments,
            ]
        )

    def wrangler(self, *arguments: str, expect_success: bool = True) -> subprocess.CompletedProcess[str]:
        return self.run(
            [
                "npx",
                "--yes",
                f"wrangler@{self.wrangler_version}",
                *arguments,
            ],
            expect_success=expect_success,
        )

    @staticmethod
    def load_json_text(text: str, label: str) -> Any:
        try:
            return json.loads(text)
        except json.JSONDecodeError as error:
            raise HarnessError(f"{label} is not valid JSON: {error}") from error

    @staticmethod
    def write_json(path: Path, value: Any) -> None:
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(
            json.dumps(value, indent=2, sort_keys=True) + "\n", encoding="utf-8"
        )

    def relative(self, path: Path) -> str:
        return path.resolve().relative_to(self.root).as_posix()

    def validate_typed_owners(self) -> None:
        for scenario_id, (relative_path, anchors) in TYPED_OWNER_ANCHORS.items():
            path = self.root / relative_path
            text = path.read_text(encoding="utf-8")
            missing = [anchor for anchor in anchors if anchor not in text]
            if missing:
                raise HarnessError(
                    f"typed owner anchors missing for {scenario_id} in {relative_path}: {missing}"
                )

        self.run(
            [
                "cargo",
                "test",
                "--locked",
                "--manifest-path",
                "tools/opsctl/Cargo.toml",
                "--lib",
            ]
        )
        self.run(["cargo", "test", "--locked", "--manifest-path", "tools/opsctl/Cargo.toml", "--test", "d1_contract_transition"])
        for scenario_id, (relative_path, anchors) in TYPED_OWNER_ANCHORS.items():
            self.record(
                scenario_id,
                relative_path,
                "typed-owner-test-suite",
                anchors=list(anchors),
            )

    def repository_projection(self) -> dict[str, Any]:
        value = self.load_json_text(self.opsctl("d1", "repository").stdout, "D1 repository projection")
        if not isinstance(value, dict) or value.get("schema_version") not in (None, 1):
            raise HarnessError("D1 repository projection has an unexpected shape")
        return value

    def histories(self, projection: dict[str, Any]) -> dict[str, list[str]]:
        components = projection.get("components")
        if not isinstance(components, list):
            raise HarnessError("D1 repository projection is missing components")
        catalog = next(
            (item for item in components if item.get("component_id") == "catalog"), None
        )
        if not isinstance(catalog, dict):
            raise HarnessError("D1 repository projection is missing catalog")
        sources = catalog.get("executable_migration_sources")
        if not isinstance(sources, list) or not sources:
            raise HarnessError("Catalog projection is missing executable_migration_sources")
        catalog_names = [item.get("migration_file") for item in sources]
        if not all(isinstance(name, str) and name for name in catalog_names):
            raise HarnessError("Catalog executable migration projection is malformed")

        resolver_root = self.root / "migrations" / "resolver-d1"
        resolver_names = sorted(path.name for path in resolver_root.glob("*.sql"))
        if not resolver_names:
            raise HarnessError("Resolver migration history is empty")
        return {"catalog": catalog_names, "resolver": resolver_names}

    def catalog_sources(self, projection: dict[str, Any]) -> list[tuple[str, Path]]:
        components = projection["components"]
        catalog = next(item for item in components if item.get("component_id") == "catalog")
        result: list[tuple[str, Path]] = []
        for item in catalog["executable_migration_sources"]:
            name = item["migration_file"]
            source_root = item["source_root"]
            if source_root not in {
                "migrations/d1",
                "migrations/d1-successor",
                "migrations/d1-successor-v2",
            }:
                raise HarnessError(f"unexpected Catalog source root: {source_root}")
            source = self.root / source_root / name
            if not source.is_file() or source.is_symlink():
                raise HarnessError(f"invalid Catalog migration source: {source}")
            result.append((name, source))
        return result

    def resolver_sources(self) -> list[tuple[str, Path]]:
        result = [
            (path.name, path)
            for path in sorted((self.root / "migrations" / "resolver-d1").glob("*.sql"))
        ]
        if not result:
            raise HarnessError("Resolver migration sources are empty")
        return result

    def prove_every_canonical_prefix(self, histories: dict[str, list[str]]) -> None:
        fixture = self.scratch / "prefix-ledger.json"
        for component, history in histories.items():
            for length in range(0, len(history) + 1):
                prefix = history[:length]
                rows = [
                    {"id": index + 1, "name": name}
                    for index, name in enumerate(prefix)
                ]
                self.write_json(fixture, {"rows": rows})
                status = self.load_json_text(
                    self.opsctl(
                        "d1",
                        "status",
                        "--component",
                        component,
                        "--ledger-json",
                        self.relative(fixture),
                    ).stdout,
                    f"{component} prefix status",
                )
                expected_state = "EXACT" if length == len(history) else "BEHIND_KNOWN_PREFIX"
                if status.get("ledger_state") != expected_state:
                    raise HarnessError(
                        f"{component} prefix {length} expected {expected_state}, got {status.get('ledger_state')}"
                    )
                if status.get("planned_migrations") != history[length:]:
                    raise HarnessError(
                        f"{component} prefix {length} planned suffix drifted from typed history"
                    )
            self.record(
                f"canonical_prefix_matrix_{component}",
                "opsctl d1 status",
                "credential-free-cli",
                prefix_count=len(history) + 1,
                current_revision=history[-1],
            )

    def prepare_case(
        self,
        case_name: str,
        component: str,
        sources: list[tuple[str, Path]],
    ) -> tuple[Path, Path, Path, str, list[str]]:
        case = self.scratch / case_name
        if case.exists():
            shutil.rmtree(case)
        migrations = case / "migrations"
        state = case / "state"
        migrations.mkdir(parents=True, exist_ok=True)
        state.mkdir(parents=True, exist_ok=True)
        for name, source in sources:
            shutil.copyfile(source, migrations / name)

        binding = "CATALOG_DB" if component == "catalog" else "RESOLVER_DB"
        database_id = (
            "00000000-0000-0000-0000-000000000051"
            if component == "catalog"
            else "00000000-0000-0000-0000-000000000052"
        )
        main = os.path.relpath(self.root / "tests" / "d1-runtime" / "worker.mjs", case)
        config = case / "wrangler.jsonc"
        self.write_json(
            config,
            {
                "name": f"tx5-{case_name}",
                "main": main.replace(os.sep, "/"),
                "compatibility_date": "2026-08-05",
                "d1_databases": [
                    {
                        "binding": binding,
                        "database_name": f"tx5-{case_name}",
                        "database_id": database_id,
                        "migrations_dir": "migrations",
                    }
                ],
            },
        )
        return case, config, state, binding, [name for name, _ in sources]

    def apply(self, binding: str, config: Path, state: Path, *, expect_success: bool = True) -> None:
        self.wrangler(
            "d1",
            "migrations",
            "apply",
            binding,
            "--local",
            "--config",
            self.relative(config),
            "--persist-to",
            self.relative(state),
            "--experimental-provision=false",
            "--experimental-auto-create=false",
            expect_success=expect_success,
        )

    def execute(self, binding: str, config: Path, state: Path, command: str) -> Any:
        result = self.wrangler(
            "d1",
            "execute",
            binding,
            "--local",
            "--config",
            self.relative(config),
            "--persist-to",
            self.relative(state),
            "--command",
            command,
            "--json",
            "--experimental-provision=false",
            "--experimental-auto-create=false",
        )
        return self.load_json_text(result.stdout, "Wrangler execute output")

    @staticmethod
    def ledger_names(value: Any) -> list[str]:
        if not isinstance(value, list) or len(value) != 1:
            raise HarnessError("Wrangler ledger output must contain one result")
        rows = value[0].get("results")
        if not isinstance(rows, list):
            raise HarnessError("Wrangler ledger output is missing results")
        names = [row.get("name") for row in rows]
        if not all(isinstance(name, str) for name in names):
            raise HarnessError("Wrangler ledger contains invalid migration names")
        return names

    def write_wrangler_json(self, path: Path, value: Any) -> None:
        self.write_json(path, value)

    def prove_bootstrap_and_replay(
        self,
        projection: dict[str, Any],
    ) -> None:
        component_sources = {
            "catalog": self.catalog_sources(projection),
            "resolver": self.resolver_sources(),
        }
        for component, sources in component_sources.items():
            _, config, state, binding, expected = self.prepare_case(
                f"{component}-bootstrap", component, sources
            )
            self.apply(binding, config, state)
            first = self.execute(
                binding, config, state, "SELECT id, name FROM d1_migrations ORDER BY id"
            )
            if self.ledger_names(first) != expected:
                raise HarnessError(f"{component} bootstrap ledger is not exact")
            self.apply(binding, config, state)
            replay = self.execute(
                binding, config, state, "SELECT id, name FROM d1_migrations ORDER BY id"
            )
            if self.ledger_names(replay) != expected:
                raise HarnessError(f"{component} replay changed the exact ledger")
            ledger_file = self.artifacts / f"tx5-{component}-bootstrap-ledger.json"
            self.write_wrangler_json(ledger_file, replay)
            status = self.load_json_text(
                self.opsctl(
                    "d1",
                    "status",
                    "--component",
                    component,
                    "--ledger-json",
                    self.relative(ledger_file),
                ).stdout,
                f"{component} bootstrap status",
            )
            if status.get("ledger_state") != "EXACT" or status.get("decision") != "SAFE":
                raise HarnessError(f"{component} bootstrap/replay is not typed SAFE/EXACT")
            self.record(
                f"clean_bootstrap_and_replay_{component}",
                "pinned local Wrangler + opsctl d1 status",
                "local-physical",
                migration_count=len(expected),
            )

    def prove_existing_catalog_like_reparse(self, projection: dict[str, Any]) -> None:
        sources = self.catalog_sources(projection)
        case, config, state, binding, prefix = self.prepare_case(
            "catalog-existing-like-reparse", "catalog", sources[:34]
        )
        self.apply(binding, config, state)
        original = self.root / "migrations/d1-successor/0032_pas2_payload_fingerprint_contract.sql"
        probe = case / "original-contract.sql"
        probe.write_text("PRAGMA case_sensitive_like=OFF;\n" + original.read_text(encoding="utf-8"), encoding="utf-8")
        failed = self.wrangler(
            "d1", "execute", binding, "--local", "--config", self.relative(config),
            "--persist-to", self.relative(state), "--file", self.relative(probe),
            "--experimental-provision=false", "--experimental-auto-create=false", expect_success=False,
        )
        if "non-deterministic functions prohibited in partial index WHERE clauses" not in failed.stdout + failed.stderr:
            raise HarnessError("original CONTRACT did not reproduce the actual D1 LIKE-index failure")
        before = self.execute(binding, config, state, "SELECT id, name FROM d1_migrations ORDER BY id")
        if self.ledger_names(before) != prefix:
            raise HarnessError("failed original CONTRACT changed the local D1 ledger")
        residue = self.execute(binding, config, state,
                               "SELECT count(*) AS residue FROM sqlite_master WHERE name='idempotency_records_pas2_contract_legacy'")
        if residue[0]["results"] != [{"residue": 0}]:
            raise HarnessError("failed original first ALTER left schema residue")
        for name, source in sources[34:]:
            # Test connection setup precedes the exact canonical SQL bytes. It is
            # fixture state, never part of the hosted executor/provider plan.
            (case / "migrations" / name).write_text(
                "PRAGMA case_sensitive_like=OFF;\n" + source.read_text(encoding="utf-8"), encoding="utf-8"
            )
            self.apply(binding, config, state)
        after = self.execute(binding, config, state, "SELECT id, name FROM d1_migrations ORDER BY id")
        if self.ledger_names(after) != [name for name, _ in sources]:
            raise HarnessError("corrected split CONTRACT/initialization did not reach exact CURRENT ledger")
        fk = self.execute(binding, config, state, "PRAGMA foreign_key_check")
        integrity = self.execute(binding, config, state, "PRAGMA quick_check")
        if fk[0]["results"] or integrity[0]["results"] != [{"quick_check": "ok"}]:
            raise HarnessError("corrected D1 connection reparse broke integrity or foreign keys")
        self.record("existing_catalog_like_reparse", "pinned local Wrangler + canonical SQL", "local-physical",
                    original_contract_rejected=True, separate_stages=[35, 36], integrity_verified=True)

    def prove_current_empty_baseline(self, projection: dict[str, Any]) -> None:
        case, config, state, binding, expected = self.prepare_case(
            "catalog-current-empty-baseline", "catalog", self.catalog_sources(projection)
        )
        baseline = case / "baseline.sql"
        self.run(["python", "scripts/cloudflare-d1-bootstrap.py", "build", "--output", str(baseline)])
        self.wrangler("d1", "execute", binding, "--local", "--config", self.relative(config),
                      "--persist-to", self.relative(state), "--file", self.relative(baseline),
                      "--experimental-provision=false", "--experimental-auto-create=false")
        ledger = self.execute(binding, config, state, "SELECT id,name FROM d1_migrations ORDER BY id")
        if self.ledger_names(ledger) != expected:
            raise HarnessError("CURRENT baseline did not create the exact canonical D1 ledger")
        before = self.execute(binding, config, state, "SELECT type,name,tbl_name,sql FROM sqlite_master ORDER BY type,name")
        self.wrangler("d1", "execute", binding, "--local", "--config", self.relative(config),
                      "--persist-to", self.relative(state), "--file", self.relative(baseline),
                      "--experimental-provision=false", "--experimental-auto-create=false", expect_success=False)
        after = self.execute(binding, config, state, "SELECT type,name,tbl_name,sql FROM sqlite_master ORDER BY type,name")
        if before[0]["results"] != after[0]["results"]:
            raise HarnessError("rejected CURRENT baseline replay changed D1 schema")
        if self.execute(binding, config, state, "PRAGMA foreign_key_check")[0]["results"]:
            raise HarnessError("CURRENT D1 baseline has invalid foreign keys")
        if self.execute(binding, config, state, "PRAGMA quick_check")[0]["results"] != [{"quick_check": "ok"}]:
            raise HarnessError("CURRENT D1 baseline failed integrity verification")
        _, reference_config, reference_state, reference_binding, _ = self.prepare_case(
            "catalog-baseline-reference", "catalog", self.catalog_sources(projection)
        )
        self.apply(reference_binding, reference_config, reference_state)
        metadata_query = ("SELECT type,name,tbl_name,sql FROM sqlite_master WHERE name NOT GLOB 'sqlite_*' "
                          "AND name NOT GLOB '_cf_*' AND name <> 'd1_migrations' ORDER BY type,name")
        reference = self.execute(reference_binding, reference_config, reference_state, metadata_query)
        baseline_schema = self.execute(binding, config, state, metadata_query)
        if reference[0]["results"] != baseline_schema[0]["results"]:
            raise HarnessError("CURRENT baseline differs from canonical migration schema on pinned D1 engine")
        self.apply(binding, config, state)  # Canonical migration replay must be a no-op after baseline.
        if self.ledger_names(self.execute(binding, config, state, "SELECT id,name FROM d1_migrations ORDER BY id")) != expected:
            raise HarnessError("ordinary migration replay changed the baseline ledger")
        self.record("current_empty_baseline", "pinned local Wrangler + CURRENT bootstrap", "local-physical",
                    migration_count=len(expected), replay_rejected=True, ordinary_migrations_noop=True,
                    canonical_migration_schema_equal=True)

    def prove_future_append(self, projection: dict[str, Any]) -> None:
        cases = [
            (
                "catalog",
                self.catalog_sources(projection),
                self.root / "tests" / "d1-evolution" / "post-epoch" / "catalog" / "0027_post_epoch_probe.sql",
                "0034_post_epoch_probe.sql",
            ),
            (
                "resolver",
                self.resolver_sources(),
                self.root / "tests" / "d1-evolution" / "post-epoch" / "resolver" / "0005_post_epoch_probe.sql",
                "0005_post_epoch_probe.sql",
            ),
        ]
        for component, sources, probe, appended_name in cases:
            case, config, state, binding, expected = self.prepare_case(
                f"{component}-future-append", component, sources
            )
            self.apply(binding, config, state)
            if not probe.is_file():
                raise HarnessError(f"future migration probe is missing: {probe}")
            shutil.copyfile(probe, case / "migrations" / appended_name)
            self.apply(binding, config, state)
            self.apply(binding, config, state)
            ledger = self.execute(
                binding, config, state, "SELECT id, name FROM d1_migrations ORDER BY id"
            )
            observed = self.ledger_names(ledger)
            if observed != expected + [appended_name]:
                raise HarnessError(f"{component} future append/replay ledger is not exact")
            self.record(
                f"future_migration_append_{component}",
                "pinned local Wrangler",
                "local-physical",
                appended_migration=appended_name,
            )

    def prove_partial_failure_and_continuation(self) -> None:
        sources = dict(self.resolver_sources())
        expected_names = [
            "0001_resolver_security_boundary.sql",
            "0002_oauth_refresh_fencing.sql",
            "0003_lookup_hmac_versions.sql",
            "0004_refresh_owner_hmac_version.sql",
        ]
        if list(sorted(sources)) != expected_names:
            raise HarnessError(
                f"Resolver canonical migration set drifted: {list(sorted(sources))}"
            )
        selected = [(name, sources[name]) for name in expected_names]
        case, config, state, binding, _ = self.prepare_case(
            "resolver-partial-failure", "resolver", selected
        )
        failing = case / "migrations" / "0002_oauth_refresh_fencing.sql"
        failing.write_text(
            "CREATE TABLE tx5_partial_failure_residue (id INTEGER PRIMARY KEY);\n"
            "INSERT INTO tx5_missing_table (id) VALUES (1);\n",
            encoding="utf-8",
        )
        self.apply(binding, config, state, expect_success=False)

        partial_ledger = self.execute(
            binding, config, state, "SELECT id, name FROM d1_migrations ORDER BY id"
        )
        residue = self.execute(
            binding,
            config,
            state,
            "SELECT COUNT(*) AS residue FROM sqlite_master WHERE name='tx5_partial_failure_residue'",
        )
        partial_file = self.artifacts / "tx5-partial-ledger.json"
        self.write_wrangler_json(partial_file, partial_ledger)
        status = self.load_json_text(
            self.opsctl(
                "d1",
                "status",
                "--component",
                "resolver",
                "--ledger-json",
                self.relative(partial_file),
            ).stdout,
            "partial status",
        )
        plan = self.load_json_text(
            self.opsctl(
                "d1",
                "plan",
                "--component",
                "resolver",
                "--ledger-json",
                self.relative(partial_file),
                "--release-manifest",
                "tests/d1-evolution/resolver-release-compatible.json",
                "--current-manifest",
                "tests/d1-evolution/resolver-release-compatible.json",
                "--known-good-manifest",
                "tests/d1-evolution/resolver-release-compatible.json",
            ).stdout,
            "partial plan",
        )
        residue_count = residue[0]["results"][0]["residue"]
        if (
            status.get("ledger_state") != "BEHIND_KNOWN_PREFIX"
            or status.get("planned_migrations", [None])[0] != "0002_oauth_refresh_fencing.sql"
            or plan.get("decision") != "MIGRATION_REQUIRED"
            or plan.get("allowed") is not False
            or "HISTORICAL_COMPATIBILITY_UNKNOWN" not in plan.get("reason_codes", [])
            or residue_count != 0
        ):
            raise HarnessError("partial failure did not preserve the accepted canonical/no-residue boundary")
        self.record(
            "partial_prefix_failure",
            "pinned local Wrangler + opsctl d1 status/plan",
            "local-fault-injection",
            ledger_state="BEHIND_KNOWN_PREFIX",
            residue=0,
        )

        shutil.copyfile(sources["0002_oauth_refresh_fencing.sql"], failing)
        self.apply(binding, config, state)
        recovered = self.execute(
            binding, config, state, "SELECT id, name FROM d1_migrations ORDER BY id"
        )
        recovered_file = self.artifacts / "tx5-recovered-ledger.json"
        self.write_wrangler_json(recovered_file, recovered)
        verify = self.load_json_text(
            self.opsctl(
                "d1",
                "verify",
                "--component",
                "resolver",
                "--ledger-json",
                self.relative(recovered_file),
                "--release-manifest",
                "tests/d1-evolution/resolver-release-compatible.json",
            ).stdout,
            "recovered verify",
        )
        if (
            verify.get("ledger_state") != "EXACT"
            or verify.get("decision") != "SAFE"
            or verify.get("allowed") is not True
        ):
            raise HarnessError("repaired partial migration did not reach typed SAFE/EXACT state")
        self.record(
            "repair_and_safe_continuation",
            "pinned local Wrangler + opsctl d1 verify",
            "local-recovery-proof",
            ledger_state="EXACT",
        )

    def write_report(self) -> None:
        scenario_ids = [item["scenario_id"] for item in self.scenarios]
        if len(scenario_ids) != len(set(scenario_ids)):
            raise HarnessError("scenario report contains duplicate scenario ids")
        report = {
            "schema_version": SCHEMA_VERSION,
            "status": "PASS",
            "mode": "LOCAL_TEST_ONLY",
            "provider_mutation": False,
            "production_mutation": False,
            "remote_provider_credentials_present": False,
            "scenario_count": len(self.scenarios),
            "scenarios": self.scenarios,
        }
        target = self.report_path
        if not target.is_absolute():
            target = self.root / target
        self.write_json(target, report)

    def execute_all(self) -> None:
        if self.scratch.exists():
            shutil.rmtree(self.scratch)
        self.scratch.mkdir(parents=True, exist_ok=True)
        self.artifacts.mkdir(parents=True, exist_ok=True)

        self.validate_typed_owners()
        projection = self.repository_projection()
        histories = self.histories(projection)
        self.prove_every_canonical_prefix(histories)
        self.prove_bootstrap_and_replay(projection)
        self.prove_existing_catalog_like_reparse(projection)
        self.prove_current_empty_baseline(projection)
        self.prove_future_append(projection)
        self.prove_partial_failure_and_continuation()
        self.write_report()


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", default=".", help="repository root")
    parser.add_argument(
        "--wrangler-version", default=DEFAULT_WRANGLER_VERSION, help="exact pinned Wrangler version"
    )
    parser.add_argument(
        "--report",
        default="artifacts/d1-evolution/fault-recovery-report.json",
        help="secret-free JSON report path",
    )
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    try:
        harness = Harness(Path(args.root), args.wrangler_version, Path(args.report))
        harness.execute_all()
    except (HarnessError, OSError, KeyError, IndexError, TypeError) as error:
        print(f"TX-5 fault/recovery harness failed: {error}", file=sys.stderr)
        return 1
    print("TX-5 fault/recovery harness passed.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
