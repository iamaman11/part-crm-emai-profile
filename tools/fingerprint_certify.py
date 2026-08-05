#!/usr/bin/env python3
"""Run sanitized external fingerprint checks on a disposable profile clone."""

from __future__ import annotations

import argparse
import copy
import hashlib
import json
from pathlib import Path
from types import SimpleNamespace

from profile_browser import FINGERPRINT_PROBE, browser_environment


def certify(workspace: Path, profile_id: str) -> dict[str, object]:
    from camoufox import Camoufox
    from camouhost.checkers.browserleaks import BrowserLeaksChecker
    from camouhost.checkers.creepjs import CreepJsChecker
    from playwright._impl._errors import TargetClosedError

    metadata = json.loads((workspace / "profile.json").read_text(encoding="utf-8"))
    if metadata.get("profile_id") != profile_id:
        raise RuntimeError("profile ID does not match metadata")

    results: list[dict[str, object]] = []
    probe_stable = False
    try:
        with Camoufox(
            config=copy.deepcopy(metadata["fingerprint_config"]),
            i_know_what_im_doing=True,
            persistent_context=True,
            user_data_dir=str(workspace / "user_data"),
            headless=False,
            enable_cache=True,
            env=browser_environment(),
        ) as context:
            page = context.pages[0] if context.pages else context.new_page()
            probe = page.evaluate(FINGERPRINT_PROBE)
            probe_digest = hashlib.sha256(
                json.dumps(probe, sort_keys=True, separators=(",", ":")).encode()
            ).hexdigest()
            probe_stable = probe_digest == metadata.get("fingerprint_probe_sha256")
            checker_context = SimpleNamespace(timeout_ms=180_000)
            for checker in (BrowserLeaksChecker(), CreepJsChecker()):
                result = checker.run(page, checker_context)
                results.append(
                    {
                        "checker": result.checker,
                        "score": result.score.value,
                        "grade": result.score.grade,
                        "signals": [
                            {"key": signal.key, "severity": signal.severity}
                            for signal in result.signals
                        ],
                    }
                )
            context.close()
    except TargetClosedError:
        pass

    return {
        "success": True,
        "profile_id": profile_id,
        "fingerprint_probe_stable": probe_stable,
        "results": results,
        "disposable_clone": True,
        "synced": False,
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--workspace", type=Path, required=True)
    parser.add_argument("--profile-id", required=True)
    args = parser.parse_args()
    print(
        json.dumps(
            certify(args.workspace.resolve(), args.profile_id),
            separators=(",", ":"),
        ),
        flush=True,
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
