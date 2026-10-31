#!/usr/bin/env python3
"""Drive the Playwright BrowserContext already owned by an installed Camouhost.

Local, explicit test-mode command. There is no CDP listener, Firefox profile attach,
new browser writer, privileged cloud control call, or profile/content restriction.

Usage:
  python scripts/camouhost-agent.py --directory C:\\bridge-agent --list
  python scripts/camouhost-agent.py --directory C:\\bridge-agent --script test.py
  python scripts/camouhost-agent.py --directory C:\\bridge-agent --pid 12345 --script test.py
  python scripts/camouhost-agent.py --directory C:\\bridge-agent --pid 12345 --timeout 0 --script test.py

A test-mode Bridge must be launched with PROFILE_BRIDGE_AGENT_DIRECTORY pointing
to the pre-existing directory. Run it as the SAME interactive Windows account.
"""
from __future__ import annotations

import argparse
import json
import math
import os
import secrets
import sys
import time
from pathlib import Path



def safe_agent_directory(path: Path) -> bool:
    """Reject symlinks/junctions/reparse points, not just Python symlinks."""
    try:
        metadata = path.lstat()
    except OSError:
        return False
    return (
        path.is_absolute()
        and not path.is_symlink()
        and not (os.name == "nt" and getattr(metadata, "st_file_attributes", 0) & 0x400)
        and path.is_dir()
    )


def inbox_owner_matches_process(requests: Path) -> bool:
    """Windows: ensure a submitted file inherits the owner's effective SID.

    An elevated token can have Administrators as its default file owner even
    when the same human owns the unelevated Camouhost inbox. Such a request
    cannot be safely renamed by Camouhost. Refuse before creating it rather
    than silently waiting for an impossible claim.
    """
    if os.name != "nt":
        return True
    import ctypes
    from ctypes import wintypes

    advapi = ctypes.WinDLL("advapi32", use_last_error=True)
    kernel = ctypes.WinDLL("kernel32", use_last_error=True)
    get_process = kernel.GetCurrentProcess
    get_process.restype = wintypes.HANDLE
    open_token = advapi.OpenProcessToken
    open_token.argtypes = [wintypes.HANDLE, wintypes.DWORD, ctypes.POINTER(wintypes.HANDLE)]
    open_token.restype = wintypes.BOOL
    get_info = advapi.GetTokenInformation
    get_info.argtypes = [
        wintypes.HANDLE, ctypes.c_int, ctypes.c_void_p,
        wintypes.DWORD, ctypes.POINTER(wintypes.DWORD),
    ]
    get_info.restype = wintypes.BOOL
    get_owner = advapi.GetNamedSecurityInfoW
    get_owner.argtypes = [
        wintypes.LPWSTR, wintypes.DWORD, wintypes.DWORD,
        ctypes.POINTER(ctypes.c_void_p), ctypes.c_void_p,
        ctypes.c_void_p, ctypes.c_void_p, ctypes.POINTER(ctypes.c_void_p),
    ]
    get_owner.restype = wintypes.DWORD
    equal_sid = advapi.EqualSid
    equal_sid.argtypes = [ctypes.c_void_p, ctypes.c_void_p]
    equal_sid.restype = wintypes.BOOL
    local_free = kernel.LocalFree
    local_free.argtypes = [ctypes.c_void_p]
    local_free.restype = ctypes.c_void_p
    close_handle = kernel.CloseHandle
    close_handle.argtypes = [wintypes.HANDLE]
    close_handle.restype = wintypes.BOOL

    token = wintypes.HANDLE()
    if not open_token(get_process(), 0x0008, ctypes.byref(token)):  # TOKEN_QUERY
        raise OSError(ctypes.get_last_error(), "Windows token-owner query denied")
    security = ctypes.c_void_p()
    try:
        size = wintypes.DWORD()
        get_info(token, 4, None, 0, ctypes.byref(size))  # TokenOwner
        if size.value < ctypes.sizeof(ctypes.c_void_p) or size.value > 65536:
            raise OSError("Windows token-owner response is invalid")
        owner_buffer = ctypes.create_string_buffer(size.value)
        if not get_info(token, 4, owner_buffer, size.value, ctypes.byref(size)):
            raise OSError(ctypes.get_last_error(), "Windows token owner unavailable")
        token_owner_sid = ctypes.cast(owner_buffer, ctypes.POINTER(ctypes.c_void_p)).contents.value
        inbox_sid = ctypes.c_void_p()
        error = get_owner(
            str(requests), 1, 1, ctypes.byref(inbox_sid), None, None, None,
            ctypes.byref(security),
        )  # SE_FILE_OBJECT + OWNER_SECURITY_INFORMATION
        if error:
            raise OSError(error, "Windows inbox owner unavailable")
        if not token_owner_sid or not inbox_sid.value:
            raise OSError("Windows inbox owner is missing")
        return bool(equal_sid(ctypes.c_void_p(token_owner_sid), inbox_sid))
    finally:
        if security.value:
            local_free(security)
        close_handle(token)


def process_alive(pid: object) -> bool:
    """Check actual process liveness before presenting a descriptor as a live session."""
    if not isinstance(pid, int) or isinstance(pid, bool) or pid <= 0:
        return False
    if os.name == "nt":
        import ctypes
        from ctypes import wintypes

        kernel32 = ctypes.WinDLL("kernel32", use_last_error=True)
        open_process = kernel32.OpenProcess
        open_process.argtypes = [wintypes.DWORD, wintypes.BOOL, wintypes.DWORD]
        open_process.restype = wintypes.HANDLE
        wait = kernel32.WaitForSingleObject
        wait.argtypes = [wintypes.HANDLE, wintypes.DWORD]
        wait.restype = wintypes.DWORD
        close = kernel32.CloseHandle
        close.argtypes = [wintypes.HANDLE]
        close.restype = wintypes.BOOL
        handle = open_process(0x00100000, False, pid)  # SYNCHRONIZE only.
        if not handle:
            return False  # Access denied/unknown must not become "active".
        try:
            return wait(handle, 0) == 0x00000102  # WAIT_TIMEOUT = still running.
        finally:
            close(handle)
    try:
        os.kill(pid, 0)
    except (OSError, OverflowError):
        return False
    return True


def durable_directory(path: Path) -> None:
    """On Unix, sync the parent entry after the atomic inbox publication."""
    if os.name == "posix":
        descriptor = os.open(path, os.O_RDONLY)
        try:
            os.fsync(descriptor)
        finally:
            os.close(descriptor)


def sessions(directory: Path) -> list[dict[str, object]]:
    active: list[dict[str, object]] = []
    for entry in sorted(directory.glob("active-*.json")):
        if entry.is_symlink() or not entry.is_file():
            continue
        try:
            details = json.loads(entry.read_text(encoding="utf-8"))
            run = Path(details["run"])
            if not safe_agent_directory(run):
                continue
            if run.parent != directory:
                continue
            if entry.name != f"active-{details.get('pid')}.json" or not process_alive(details.get("pid")):
                continue
            if not safe_agent_directory(run / "requests") or not safe_agent_directory(run / "results"):
                continue
            if entry.is_symlink() or not entry.is_file():
                continue
            active.append(details)
        except (OSError, ValueError, TypeError, KeyError):
            continue
    return active


def run() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--directory", type=Path, required=True)
    parser.add_argument("--pid", type=int)
    parser.add_argument("--list", action="store_true")
    parser.add_argument("--script", type=Path)
    parser.add_argument("--timeout", type=float, default=180.0,
                        help="Wait for completion in seconds; 0 means no time limit")
    arguments = parser.parse_args()
    if not safe_agent_directory(arguments.directory):
        parser.error("agent directory must be absolute and cannot be a symlink")
    directory = arguments.directory.resolve(strict=True)
    if not directory.is_dir():
        parser.error("agent directory must be an existing regular directory")
    available = sessions(directory)
    if arguments.list:
        for item in available:
            print(json.dumps(item, ensure_ascii=False))
        return 0
    if arguments.script is None or not math.isfinite(arguments.timeout) or arguments.timeout < 0:
        parser.error("--script is required and --timeout must be a finite nonnegative number")
    candidates = [x for x in available
                  if arguments.pid is None or x.get("pid") == arguments.pid]
    if len(candidates) != 1:
        parser.error(f"expected one live Camouhost selection; found {len(candidates)}; use --pid")
    run_dir = Path(candidates[0]["run"])
    requests = run_dir / "requests"
    results = run_dir / "results"
    if not safe_agent_directory(requests) or not safe_agent_directory(results):
        print("PLAYWRIGHT_AGENT_UNSAFE_INBOX: invalid directory or reparse point", file=sys.stderr)
        return 126
    try:
        owner_matches = inbox_owner_matches_process(requests)
    except OSError as error:
        print(
            f"PLAYWRIGHT_AGENT_SECURITY_CONTEXT_UNKNOWN: phase=prepublish "
            f"errno={error.errno} winerror={getattr(error, 'winerror', None)}; "
            "no request published", file=sys.stderr,
        )
        return 126
    if not owner_matches:
        print(
            "PLAYWRIGHT_AGENT_SECURITY_CONTEXT_MISMATCH: phase=prepublish; "
            "elevated and ordinary-user Windows tokens cannot share this "
            "inbox safely. Run the CLI unelevated with the same token as "
            "Camouhost; do not manually change request ownership. "
            "No request was published.", file=sys.stderr,
        )
        return 126
    script = arguments.script.read_bytes()  # No artificial source/content cap for test profiles.
    job_id = secrets.token_hex(16)
    staging = requests / f".{job_id}.part"
    committed = requests / f"{job_id}.py"
    with staging.open("xb") as output:
        output.write(script)
        output.flush()
        os.fsync(output.fileno())
    os.replace(staging, committed)
    durable_directory(requests)
    print(f"PLAYWRIGHT_AGENT_JOB={job_id}", flush=True)
    status = results / f"{job_id}.json"
    descriptor = directory / f"active-{candidates[0]['pid']}.json"
    deadline = None if arguments.timeout == 0 else time.monotonic() + arguments.timeout
    while not status.is_file():
        if not descriptor.is_file() or not process_alive(candidates[0].get("pid")):
            print(
                "PLAYWRIGHT_AGENT_UNCONFIRMED: session ended without a result; "
                "do not replay this job automatically",
                file=sys.stderr,
            )
            return 125
        if deadline is not None and time.monotonic() >= deadline:
            claimed = requests / f"{job_id}.running"
            stage = "RUNNING" if claimed.exists() else "QUEUED"
            print(
                f"PLAYWRIGHT_AGENT_PENDING_{stage}: no confirmed result; "
                "do not replay automatically",
                file=sys.stderr,
            )
            return 124
        time.sleep(0.1)
    try:
        details = json.loads(status.read_text(encoding="utf-8"))
    except (OSError, ValueError):
        print("PLAYWRIGHT_AGENT_UNCONFIRMED: unreadable result; do not retry automatically", file=sys.stderr)
        return 125
    for extension, target in (("stdout.txt", sys.stdout), ("stderr.txt", sys.stderr)):
        log = results / f"{job_id}.{extension}"
        if log.is_file():
            target.write(log.read_text(encoding="utf-8"))
    print(json.dumps(details, ensure_ascii=False, default=str), flush=True)
    return 0 if details.get("status") == "ok" else 1


if __name__ == "__main__":
    raise SystemExit(run())
