#!/usr/bin/env python3
"""Build one fail-closed patched Camoufox candidate outside customer hosts."""
from __future__ import annotations
import argparse, hashlib, json, shutil, subprocess, sys
from pathlib import Path
ROOT = Path(__file__).resolve().parents[1]
LOCK = ROOT / "runtime/camouhost/camoufox-patch-lock.json"
VERIFY = ROOT / "scripts/check-camoufox-webgl-patch.py"
def run(*args: str, cwd: Path) -> None: subprocess.run(args, cwd=cwd, check=True)
def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for block in iter(lambda: handle.read(1024 * 1024), b""): digest.update(block)
    return digest.hexdigest()
def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--upstream-root", type=Path, required=True)
    parser.add_argument("--target", choices=("linux", "windows"), required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    lock = json.loads(LOCK.read_text(encoding="utf-8"))
    source = args.upstream_root.resolve(strict=True)
    actual = subprocess.check_output(("git","rev-parse","HEAD"), cwd=source, text=True).strip()
    if actual != lock["browser"]["release_commit"]:
        raise SystemExit("candidate source commit differs from pinned runtime patch lock")
    run(sys.executable, str(VERIFY), "--upstream-root", str(source), cwd=ROOT)
    run("patch", "--batch", "-p1", "-i", str(ROOT / lock["patch"]["path"]), cwd=source)
    run("make", "fetch", cwd=source)
    run("make", "setup-minimal", cwd=source)
    run("make", "mozbootstrap", cwd=source)
    run("python3", "multibuild.py", "--target", args.target, "--arch", "x86_64", cwd=source)
    candidates = sorted(source.glob(f"dist/camoufox-*-{args.target}.x86_64.zip"))
    if len(candidates) != 1 or candidates[0].is_symlink():
        raise SystemExit("candidate build did not produce exactly one regular browser archive")
    args.output.parent.mkdir(parents=True, exist_ok=True)
    if args.output.exists(): raise SystemExit("candidate output already exists")
    shutil.copyfile(candidates[0], args.output)
    print(json.dumps({"kind":"CAMOUFOX_PATCHED_CANDIDATE","target":args.target,"source_commit":actual,"patch_sha256":lock["patch"]["sha256"],"artifact_sha256":sha256(args.output)},sort_keys=True))
    return 0
if __name__ == "__main__": raise SystemExit(main())
