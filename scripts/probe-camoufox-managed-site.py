#!/usr/bin/env python3
"""Opt-in disposable Camoufox/Playwright compatibility probe; not shipping code.

Run only on a disposable authorized Windows test host with the pinned,
installed Camoufox/Python runtime. Uses one new TemporaryDirectory per trial,
never opens a saved CRM profile, never authenticates to a site, and never reads
or prints cookies, DOM, page errors, site content, account names or arguments.

Example:
  <pinned-python> scripts/probe-camoufox-managed-site.py \
      --browser <absolute-path-to-pinned-camoufox.exe> --target vk --repeats 3
Control:
  ... --target example --repeats 3
Opt-in isolation only (not a proposed production workaround):
  ... --target vk --disable-workers --repeats 3
"""
from __future__ import annotations

import argparse
import pathlib
import sys
import tempfile
from typing import Callable

URLS = {
    "vk": "https://id.vk.ru/",
    "example": "https://example.com/",
}
TRIALS_MAX = 3


def classify_events(stages: list[str], error_class: str | None) -> str:
    """Never report PASS merely because DOMContentLoaded fired before crash."""
    if "content_crashed" in stages:
        return "TAB_CRASH"
    if error_class == "TargetClosedError":
        return "TARGET_CLOSED"
    if error_class is not None:
        return "ERROR"
    if "finished_no_crash" in stages and "dom_loaded" in stages:
        return "PASS"
    return "INCOMPLETE"


def assert_self_test() -> None:
    assert classify_events(["dom_loaded", "content_crashed"], "TargetClosedError") == "TAB_CRASH"
    assert classify_events(["dom_loaded"], "TargetClosedError") == "TARGET_CLOSED"
    assert classify_events(["dom_loaded"], None) == "INCOMPLETE"
    assert classify_events(["dom_loaded", "finished_no_crash"], None) == "PASS"
    assert classify_events(["finished_no_crash"], None) == "INCOMPLETE"
    assert classify_events(["dom_loaded", "content_crashed", "finished_no_crash"], None) == "TAB_CRASH"
    assert set(URLS) == {"vk", "example"}


def single_trial(
    browser: pathlib.Path, target: str, disable_workers: bool,
    disable_javascript: bool, block_webgl: bool,
    profile_root: pathlib.Path,
) -> tuple[str, str]:
    # Import is delayed so --self-test can run offline on any CI OS.
    from camoufox.sync_api import Camoufox

    stages: list[str] = []
    error_class = None
    observed_workers = "unknown"

    def stage(name: str) -> None:
        stages.append(name)

    def callback(_: object) -> None:
        stage("content_crashed")

    kwargs: dict[str, object] = {
        "executable_path": browser,
        "ff_version": 158,
        "headless": False,
        "persistent_context": True,
        "user_data_dir": str(profile_root),
    }
    prefs: dict[str, bool] = {}
    if disable_workers:
        prefs["dom.workers.enabled"] = False
    if disable_javascript:
        prefs["javascript.enabled"] = False
    if prefs:
        kwargs["firefox_user_prefs"] = prefs
    if block_webgl:
        kwargs["block_webgl"] = True

    try:
        with Camoufox(**kwargs) as context:
            page = context.new_page()
            page.on("crash", callback)
            # A constructor check is diagnostic, NOT a privileged Firefox
            # about:config/pref readback and not proof that workers are disabled.
            try:
                observed_workers = "present" if page.evaluate(
                    "() => typeof Worker === 'function'"
                ) else "absent"
            except Exception:
                observed_workers = "unknown"
            page.goto(URLS[target], wait_until="domcontentloaded", timeout=25000)
            stage("dom_loaded")
            page.wait_for_timeout(3500)
            stage("finished_no_crash")
    except BaseException as error:
        # Never print exception messages: they can contain URLs, DOM text,
        # private paths, cookies and session details.
        error_class = type(error).__name__

    return classify_events(stages, error_class), observed_workers


def parse_args(args: list[str]) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--self-test", action="store_true", help="offline classification tests")
    parser.add_argument("--browser", type=pathlib.Path)
    parser.add_argument("--target", choices=sorted(URLS), default="vk")
    parser.add_argument("--repeats", type=int, choices=range(1, TRIALS_MAX + 1), default=1)
    parser.add_argument("--disable-workers", action="store_true")
    parser.add_argument("--disable-javascript", action="store_true")
    parser.add_argument("--block-webgl", action="store_true")
    return parser.parse_args(args)


def main(args: list[str] | None = None) -> int:
    config = parse_args(sys.argv[1:] if args is None else args)
    assert_self_test()
    if config.self_test:
        print("CAMOUFOX_MANAGED_PROBE_SELF_TEST=PASS", flush=True)
        return 0
    if config.browser is None or not config.browser.is_absolute() or not config.browser.is_file():
        print("CAMOUFOX_MANAGED_PROBE=INVALID_PINNED_BROWSER", flush=True)
        return 2
    if config.browser.name.lower() != "camoufox.exe" or sys.platform != "win32":
        print("CAMOUFOX_MANAGED_PROBE=WINDOWS_PINNED_CAMOUFOX_REQUIRED", flush=True)
        return 2

    variant = ("workers_off" if config.disable_workers else "workers_default")
    if config.disable_javascript:
        variant += "_javascript_off"
    if config.block_webgl:
        variant += "_webgl_blocked"
    failed = False
    for trial in range(1, config.repeats + 1):
        # Temp root is a new private profile for every attempt; the context
        # manager exits before cleanup. If Windows still holds a file, fail
        # visibly rather than deleting or killing an unrelated browser.
        try:
            with tempfile.TemporaryDirectory(prefix="crm-managed-vendor-probe-") as temp:
                result, worker_ctor = single_trial(
                    config.browser, config.target,
                    config.disable_workers, config.disable_javascript,
                    config.block_webgl, pathlib.Path(temp) / "user_data",
                )
        except OSError:
            print("CAMOUFOX_MANAGED_PROBE_CLEANUP=INCOMPLETE", flush=True)
            return 3
        print(
            f"CAMOUFOX_MANAGED_PROBE target={config.target} variant={variant} "
            f"trial={trial} result={result} worker_constructor={worker_ctor}",
            flush=True,
        )
        failed |= result != "PASS"
    return 1 if failed else 0


if __name__ == "__main__":
    raise SystemExit(main())
