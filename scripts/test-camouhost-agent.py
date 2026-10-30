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
from unittest.mock import patch
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

    def title(self) -> str:
        return "Existing tab"

    def content(self) -> str:
        return "<html>fake restored</html>"

    def evaluate(self, _script: str) -> dict[str, object]:
        return {"local": {"marker": "saved"}, "session": {}}

    def screenshot(self, *, path: str) -> None:
        Path(path).write_bytes(b"PNG-test-evidence")


class Context:
    def __init__(self) -> None:
        self.pages = [Page()]

    def cookies(self) -> list[dict[str, str]]:
        return [{"name": "test_cookie", "value": "full-value"}]


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

            # The listing and selector must never resurrect a stale Camouhost
            # descriptor after an unclean exit. No manual deletion is needed.
            stale = base / "active--1.json"
            stale.write_text(json.dumps({"pid": -1, "run": str(agent.run)}), encoding="utf-8")
            listing = subprocess.run(
                [sys.executable, str(ROOT / "scripts" / "camouhost-agent.py"),
                 "--directory", str(base), "--list"],
                capture_output=True, text=True, timeout=10,
            )
            assert listing.returncode == 0 and '"pid": -1' not in listing.stdout
            assert '"pid": ' + str(os.getpid()) in listing.stdout
            assert stale.is_file(), "stale evidence was silently deleted"

            # A queued script which never reaches the owner is PENDING, not a
            # successful browser operation. It remains intact for examination.
            waiting = subprocess.run(
                [*command[:6], "--script", str(sample), "--timeout", "0.2"],
                capture_output=True, text=True, timeout=10,
            )
            assert waiting.returncode == 124, waiting.stderr
            assert "PENDING_QUEUED" in waiting.stderr
            queued = waiting.stdout.split("PLAYWRIGHT_AGENT_JOB=")[1].splitlines()[0]
            assert (agent.requests / f"{queued}.py").exists()

            # An already-claimed but response-less job must NOT be replayed,
            # even if it took an action in the browser before Camouhost died.
            claimed = committed_request(agent.requests, "raise AssertionError('do not replay')")
            os.replace(agent.requests / f"{claimed}.py", agent.requests / f"{claimed}.running")
            running = agent.requests / f"{claimed}.running"
            assert running.exists()
            assert agent.run_one(fake)  # prior timed-out queued request can now complete
            assert running.exists() and not (agent.results / f"{claimed}.json").exists()

            # Invalid timeout values cannot mean "wait forever" by accident.
            for invalid in ("nan", "inf", "-1"):
                rejected = subprocess.run(
                    [*command[:6], "--script", str(sample), "--timeout", invalid],
                    capture_output=True, text=True, timeout=10,
                )
                assert rejected.returncode == 2, rejected.stderr

            # Read-only B7 capture: exactly the original page set, URLs, DOM
            # and cookies; no navigation, new page, browser close or Save.
            snapshot = committed_request(
                agent.requests,
                (ROOT / "scripts" / "b7-playwright-observe.py").read_text(encoding="utf-8"),
            )
            for _ in range(3):
                if (agent.results / f"{snapshot}.json").is_file():
                    break
                assert agent.run_one(fake)
            snapshot_status = json.loads(
                (agent.results / f"{snapshot}.json").read_text(encoding="utf-8")
            )
            assert snapshot_status["status"] == "ok", snapshot_status
            capture = json.loads(
                Path(snapshot_status["result"]["artifact"]).read_text(encoding="utf-8")
            )
            assert capture["count_before"] == capture["count_after"] == 1
            assert capture["pages"][0]["url"] == private_content
            assert capture["pages"][0]["html"] == "<html>fake restored</html>"
            assert capture["cookies"] == [{"name": "test_cookie", "value": "full-value"}]
            assert not capture["errors"]
            assert fake.pages[0].url == private_content, "read-only B7 script navigated"

            # A broken observation must preserve raw errors and must NOT
            # become a positive restored-content claim or delete its artifact.
            original = fake.pages[0].screenshot
            fake.pages[0].screenshot = lambda **_args: (_ for _ in ()).throw(OSError("snapshot disk full"))
            try:
                broken = committed_request(
                    agent.requests,
                    (ROOT / "scripts" / "b7-playwright-observe.py").read_text(encoding="utf-8"),
                )
                assert agent.run_one(fake)
                failed_status = json.loads(
                    (agent.results / f"{broken}.json").read_text(encoding="utf-8")
                )
                assert failed_status["status"] == "error"
                assert "snapshot disk full" in (
                    agent.results / f"{broken}.stderr.txt"
                ).read_text(encoding="utf-8")
                assert len(list(agent.results.glob("b7-first-restore-*.json"))) == 2
            finally:
                fake.pages[0].screenshot = original

            # A disk-full/fsync failure AFTER Python has executed cannot
            # invent successful completion. The claimed script and its raw
            # local stdout remain for diagnosis; automatic replay is forbidden.
            durability_job = committed_request(
                agent.requests,
                "context.pages[0].goto('about:durability-failure')\nresult = 'executed'\n",
            )
            with patch.object(runtime, "durable_agent_directory", lambda _path: None), \
                    patch.object(runtime.os, "fsync", side_effect=OSError("simulated disk full")):
                try:
                    agent.run_one(fake)
                except OSError as error:
                    assert "disk full" in str(error)
                else:
                    raise AssertionError("fsync failure silently confirmed a result")
            assert fake.pages[0].url == "about:durability-failure"
            assert not (agent.results / f"{durability_job}.json").exists()
            assert (agent.requests / f"{durability_job}.running").exists()
            assert not agent.run_one(fake), "uncertain already-claimed request was replayed"

            # If the live locator disappears while a submitted job is still
            # awaiting dispatch, the CLI returns UNCONFIRMED, not Saved/OK.
            prior = set(agent.requests.glob("*.py"))
            pending = subprocess.Popen(
                [*command[:6], "--script", str(sample), "--timeout", "5"],
                stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True,
            )
            cutoff = time.monotonic() + 4
            while time.monotonic() < cutoff and set(agent.requests.glob("*.py")) == prior:
                time.sleep(0.01)
            assert set(agent.requests.glob("*.py")) != prior, "client never submitted pending job"
            agent.close()
            outgoing, errors = pending.communicate(timeout=8)
            assert pending.returncode == 125 and "UNCONFIRMED" in errors, (outgoing, errors)
        finally:
            agent.close()
        assert not agent.descriptor.exists()
        assert (agent.results / f"{token}.json").exists(), "test artifacts must outlive browser"


def prove_owner_thread_polling() -> None:
    """Real pipe on Windows/POSIX: one reader, partial lines, buffered burst and EOF."""
    source = ROOT / "runtime" / "camouhost" / "real.py"
    probe = (
        "import importlib.util, sys, time\n"
        f"spec = importlib.util.spec_from_file_location('agent_pipe_probe', {str(source)!r})\n"
        "module = importlib.util.module_from_spec(spec)\n"
        "spec.loader.exec_module(module)\n"
        "pump = module.BridgeInputPoller()\n"
        "lines = []\n"
        "deadline = time.monotonic() + 8\n"
        "while len(lines) < 3 and time.monotonic() < deadline:\n"
        "    line = pump.read()\n"
        "    if line is None: continue\n"
        "    lines.append(line)\n"
        "print(repr(lines), flush=True)\n"
    )
    completed = subprocess.run(
        [sys.executable, "-u", "-c", probe],
        input=b"hello|a\nclose|b\n", capture_output=True, timeout=15,
    )
    assert completed.returncode == 0, completed.stderr
    assert "['hello|a\\n', 'close|b\\n', '']" in completed.stdout.decode("utf-8"), completed.stdout


def main() -> int:
    prove_agent()
    prove_owner_thread_polling()
    print("Owned Playwright agent: owner-thread pipe, atomic jobs, full data, single context and replay guard PASS.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
