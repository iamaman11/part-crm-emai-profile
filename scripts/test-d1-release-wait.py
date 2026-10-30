#!/usr/bin/env python3
"""Negative tests for the exact D1 release-wait shell in the workflow.

No real GitHub calls, Cloudflare credentials, provider writes or migrations.
"""
from __future__ import annotations

import os
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
WORKFLOW = ROOT / ".github/workflows/d1-isolated-target-observation.yml"
BEGIN = "# BEGIN D1_RELEASE_WAIT"
END = "# END D1_RELEASE_WAIT"
SHA = "c199c54dff64cbbf41c87301da992d79aada072d"
RELEASE = "release-set-v3-sha256-" + "a" * 64


def wait_source() -> str:
    source = WORKFLOW.read_text(encoding="utf-8")
    assert source.count(BEGIN) == 1 and source.count(END) == 1
    content = source.split(BEGIN, 1)[1].split("\n", 1)[1].split(END, 1)[0]
    assert "max_attempts=96" in content
    assert "timeout 45s gh api --paginate" in content
    assert "D1_RELEASE_DISCOVERY_FAILED" in content
    assert "D1_RELEASE_AMBIGUOUS" in content
    assert "D1_RELEASE_NOT_PUBLISHED" in content
    assert 'target_release=' in content and 'releases[0]' in content
    assert "sleep 15" in content
    assert "timeout-minutes: 45" in source
    assert "--connect-timeout 10 --max-time 90 --retry 5" in source
    assert "python scripts/test-d1-release-wait.py" in source
    # Speed up mock-only tests without changing production source.
    return content.replace("max_attempts=96", "max_attempts=3").replace("sleep 15", "sleep 0")


def case(content: str, scenario: str, *, expected_code: int, marker: str, calls: int) -> None:
    with tempfile.TemporaryDirectory(prefix="d1-release-wait-") as tmp:
        root = Path(tmp)
        bin_dir = root / "bin"
        bin_dir.mkdir()
        stub = bin_dir / "gh"
        stub.write_text(
            "#!/usr/bin/env python3\n"
            "import os, sys\n"
            "from pathlib import Path\n"
            "state = Path(os.environ['FAKE_GH_STATE'])\n"
            "count = int(state.read_text()) if state.exists() else 0\n"
            "state.write_text(str(count + 1))\n"
            "scenario = os.environ['FAKE_GH_SCENARIO']\n"
            "if scenario == 'api_error': sys.exit(14)\n"
            "if scenario == 'delayed' and count == 0: sys.exit(0)\n"
            "if scenario == 'never': sys.exit(0)\n"
            "print(os.environ['FAKE_GH_RELEASE'])\n"
            "if scenario == 'ambiguous': print('release-set-v3-sha256-' + 'b' * 64)\n",
            encoding="utf-8",
        )
        stub.chmod(0o755)
        env = os.environ.copy()
        env.update({
            "PATH": str(bin_dir) + os.pathsep + os.environ["PATH"],
            "FAKE_GH_SCENARIO": scenario,
            "FAKE_GH_RELEASE": RELEASE,
            "FAKE_GH_STATE": str(root / "calls"),
            "GITHUB_REPOSITORY": "iamaman11/part-crm-emai-profile",
            "SOURCE_SHA": SHA,
        })
        cmd = "set -euo pipefail\nroot=" + str(root) + "\n" + content
        run = subprocess.run(["bash", "-c", cmd], capture_output=True, text=True, env=env, timeout=15)
        assert run.returncode == expected_code, (scenario, run.returncode, run.stdout, run.stderr)
        assert marker in run.stdout + run.stderr, (scenario, run.stdout, run.stderr)
        assert int((root / "calls").read_text()) == calls, scenario
        if expected_code == 0:
            assert not (root / "release-tags-attempt.txt").exists()


def main() -> int:
    source = wait_source()
    case(source, "delayed", expected_code=0, marker="D1_RELEASE_READY", calls=2)
    case(source, "never", expected_code=1, marker="D1_RELEASE_NOT_PUBLISHED", calls=3)
    case(source, "ambiguous", expected_code=1, marker="D1_RELEASE_AMBIGUOUS", calls=1)
    case(source, "api_error", expected_code=1, marker="D1_RELEASE_DISCOVERY_FAILED", calls=1)
    print("d1_release_wait delayed/missing/ambiguous/api-error negative matrix PASS", flush=True)
    return 0


if __name__ == "__main__":
    sys.exit(main())
