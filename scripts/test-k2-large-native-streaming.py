#!/usr/bin/env python3
"""Measured 256/512 MiB K2 file-stream roundtrip in the real native Rust test process.

This validates bounded file streaming and cold snapshot restore. It does NOT
exercise R2/D1, installed Windows browser state, or independent WORKING/LKG.
Never reinterpret this test as an installed B7 or cloud acceptance receipt.
"""

from __future__ import annotations

import ctypes
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import time

TEST = "generation_snapshot::tests::k2_full_256_and_512_mib_file_streaming_encrypt_cold_restore"
MIB = 1_048_576
MAX_PROCESS_PEAK_RSS = 256 * MIB
MAX_LOGICAL_PEAK_DISK = 2_304 * MIB  # 512 MiB native + ciphertext + BPGW + restored + margins


def compiled_library_test() -> Path:
    result = subprocess.run(
        [
            "cargo", "test", "--locked", "-p", "profile-bridge", "--lib",
            "--features", "synthetic-test-bin", "--no-run", "--message-format=json",
        ],
        check=True,
        capture_output=True,
        text=True,
    )
    candidates: list[Path] = []
    for line in result.stdout.splitlines():
        try:
            artifact = json.loads(line)
        except json.JSONDecodeError:
            continue
        target = artifact.get("target", {})
        executable = artifact.get("executable")
        if (
            artifact.get("reason") == "compiler-artifact"
            and target.get("name") == "profile_bridge"
            and "lib" in target.get("kind", [])
            and executable
        ):
            candidates.append(Path(executable))
    if len(candidates) != 1 or not candidates[0].is_file():
        raise RuntimeError(
            f"K2 requires exactly one compiled Profile Bridge library test, got {candidates}"
        )
    return candidates[0]


def peak_process_rss(pid: int) -> int | None:
    if sys.platform == "win32":
        class Counters(ctypes.Structure):
            _fields_ = [
                ("cb", ctypes.c_ulong),
                ("PageFaultCount", ctypes.c_ulong),
                ("PeakWorkingSetSize", ctypes.c_size_t),
                ("WorkingSetSize", ctypes.c_size_t),
                ("QuotaPeakPagedPoolUsage", ctypes.c_size_t),
                ("QuotaPagedPoolUsage", ctypes.c_size_t),
                ("QuotaPeakNonPagedPoolUsage", ctypes.c_size_t),
                ("QuotaNonPagedPoolUsage", ctypes.c_size_t),
                ("PagefileUsage", ctypes.c_size_t),
                ("PeakPagefileUsage", ctypes.c_size_t),
            ]
        kernel32 = ctypes.WinDLL("kernel32", use_last_error=True)
        psapi = ctypes.WinDLL("psapi", use_last_error=True)
        kernel32.OpenProcess.argtypes = [ctypes.c_ulong, ctypes.c_int, ctypes.c_ulong]
        kernel32.OpenProcess.restype = ctypes.c_void_p
        kernel32.CloseHandle.argtypes = [ctypes.c_void_p]
        psapi.GetProcessMemoryInfo.argtypes = [
            ctypes.c_void_p, ctypes.POINTER(Counters), ctypes.c_ulong
        ]
        psapi.GetProcessMemoryInfo.restype = ctypes.c_int
        handle = kernel32.OpenProcess(0x0410, 0, pid)  # QUERY_INFORMATION | VM_READ
        if not handle:
            return None
        try:
            counters = Counters()
            counters.cb = ctypes.sizeof(Counters)
            if not psapi.GetProcessMemoryInfo(
                handle, ctypes.byref(counters), ctypes.sizeof(Counters)
            ):
                return None
            return int(counters.PeakWorkingSetSize)
        finally:
            kernel32.CloseHandle(handle)
    if sys.platform.startswith("linux"):
        try:
            contents = Path(f"/proc/{pid}/status").read_text(encoding="utf-8")
        except FileNotFoundError:
            return None
        for line in contents.splitlines():
            if line.startswith("VmHWM:"):
                return int(line.split()[1]) * 1024
    return None


def fixture_logical_disk_bytes(pid: int) -> int:
    total = 0
    for root in Path(tempfile.gettempdir()).glob(
        f"profile-bridge-generation-snapshot-{pid}-*"
    ):
        for path in root.rglob("*"):
            if path.is_file() and not path.is_symlink():
                try:
                    total += path.stat().st_size
                except FileNotFoundError:
                    # Test can retire a staging file between listing and stat.
                    pass
    return total


def main() -> int:
    if sys.platform != "win32" and not sys.platform.startswith("linux"):
        raise RuntimeError("K2 memory proof requires Windows or Linux process accounting")
    test_binary = compiled_library_test()
    started = time.monotonic()
    with subprocess.Popen(
        [str(test_binary), "--exact", TEST, "--ignored", "--nocapture"],
        stdout=None,
        stderr=None,
    ) as child:
        peak_rss = 0
        peak_disk = 0
        memory_samples = 0
        while child.poll() is None:
            observed = peak_process_rss(child.pid)
            if observed is not None:
                memory_samples += 1
                peak_rss = max(peak_rss, observed)
            peak_disk = max(peak_disk, fixture_logical_disk_bytes(child.pid))
            time.sleep(0.1)
        status = child.wait()
    elapsed = time.monotonic() - started
    print(
        f"K2_MEASURED_FILE_STREAM exact_test={TEST} source=256/512MiB"
        f" peak_process_rss_bytes={peak_rss}"
        f" peak_fixture_logical_disk_bytes={peak_disk}"
        f" memory_samples={memory_samples} elapsed_seconds={elapsed:.1f}",
        flush=True,
    )
    if status != 0:
        raise RuntimeError(f"K2 exact native file-stream Rust test failed: exit={status}")
    if memory_samples < 2 or peak_disk < 512 * MIB:
        raise RuntimeError("K2 resource measurement missing representative samples")
    if peak_rss > MAX_PROCESS_PEAK_RSS:
        raise RuntimeError("K2 native file-stream test breached the 256 MiB peak RSS budget")
    if peak_disk > MAX_LOGICAL_PEAK_DISK:
        raise RuntimeError("K2 native file-stream test breached the 2.25 GiB temp disk budget")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
