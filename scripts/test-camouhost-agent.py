#!/usr/bin/env python3
"""Source-side regression for the owned Playwright agent transport, no browser needed."""
from __future__ import annotations

import importlib.util
import json
import os
import secrets
import subprocess
import sys
import tempfile
import threading
import time
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]


def runtime_module():
    spec = importlib.util.spec_from_file_location(
        "owned_agent_test_runtime", ROOT / "runtime" / "camouhost" / "real.py"
    )
    assert spec and spec.loader
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


class Page:
    def __init__(self) -> None:
        self.url = "about:blank"

    def goto(self, url: str) -> None:
        self.url = url


class Context:
    def __init__(self) -> None:
        self.pages = [Page()]


def committed_request(requests: Path, script: str) -> str:
    token = secrets.token_hex(16)
    staged = requests / f".{token}.part"
    staged.write_text(script, encoding="utf-8")
    os.replace(staged, requests / f"{token}.py")
    return token


def prove_agent() -> None:
    runtime = runtime_module()
    with tempfile.TemporaryDirectory(prefix="owned-playwright-agent-") as temp:
        base = Path(temp).resolve()
        fake = Context()
        # An ordinary session has no inbox. A non-absolute/symlink target is rejected.
        try:
            runtime.LocalPlaywrightAgent("relative", "session_test")
        except runtime.RuntimeContractError:
            pass
        else:
            raise AssertionError("unqualified local agent directory admitted")
        agent = runtime.LocalPlaywrightAgent(str(base), "session_test")
        assert json.loads(agent.descriptor.read_text(encoding="utf-8"))["run"] == str(agent.run)
        try:
            partial = agent.requests / ".incomplete.part"
            partial.write_text("raise AssertionError('uncommitted')", encoding="utf-8")
            assert agent.run_one(fake) is False, "partial writes must never execute"
            private_content = "https://example.test/full-url?raw=1&content=ok"
            token = committed_request(
                agent.requests,
                "import json\n"
                f"context.pages[0].goto({private_content!r})\n"
                "print(context.pages[0].url)\n"
                "result = {'url': context.pages[0].url, 'page_count': len(context.pages)}\n",
            )
            assert agent.run_one(fake)
            result = json.loads((agent.results / f"{token}.json").read_text(encoding="utf-8"))
            assert result == {"status": "ok", "result": {"url": private_content, "page_count": 1}}
            assert private_content in (agent.results / f"{token}.stdout.txt").read_text(encoding="utf-8")
            assert not agent.run_one(fake), "completed script must execute exactly once"

            failing = committed_request(
                agent.requests,
                "print('before error')\nraise ValueError('full error with test-only URL')\n",
            )
            assert agent.run_one(fake)
            result = json.loads((agent.results / f"{failing}.json").read_text(encoding="utf-8"))
            assert result["status"] == "error"
            assert "test-only URL" in (agent.results / f"{failing}.stderr.txt").read_text(encoding="utf-8")
            assert fake.pages[0].url == private_content, "agent failure destroyed live context"
            assert agent.run_one(fake) is False

            # CLI submits a request to the SAME live session; owner thread alone
            # runs the script. No second browser, remote debugger, or IPC controller.
            sample = base / "sample.py"
            sample.write_text("result = {'same_context': context.pages[0].url}\n", encoding="utf-8")
            command = [
                sys.executable, str(ROOT / "scripts" / "camouhost-agent.py"),
                "--directory", str(base), "--pid", str(os.getpid()),
                "--script", str(sample), "--timeout", "15",
            ]
            submitted: list[subprocess.CompletedProcess[str]] = []

            def caller() -> None:
                submitted.append(subprocess.run(command, capture_output=True, text=True, timeout=20))

            child = threading.Thread(target=caller, daemon=True)
            child.start()
            deadline = time.monotonic() + 15
            while child.is_alive() and time.monotonic() < deadline:
                agent.run_one(fake)
                time.sleep(0.01)
            child.join(timeout=2)
            assert len(submitted) == 1, "local client did not return from active context"
            assert submitted[0].returncode == 0, submitted[0].stderr
            assert '"same_context": "https://example.test' in submitted[0].stdout
        finally:
            agent.close()
        assert not agent.descriptor.exists()
        assert (agent.results / f"{token}.json").exists(), "test artifacts must outlive browser"


def main() -> int:
    prove_agent()
    print("Owned Playwright agent: opt-in, atomic jobs, full data, single context and replay guard PASS.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
