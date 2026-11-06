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
import contextlib
import importlib.util
import pathlib
import sys
import tempfile


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
    assert parse_args(["--skip-worker-check"]).skip_worker_check
    assert not parse_args([]).skip_worker_check
    assert parse_args(["--exclude-ubo"]).exclude_ubo
    assert not parse_args([]).exclude_ubo
    assert parse_args(["--pinned-ubo-dir", "/fixture"]).pinned_ubo_dir is not None
    assert parse_args(["--shipping-camouhost", "/fixture"]).shipping_camouhost is not None
    assert parse_args([]).shipping_camouhost is None


def shipping_launch_options(
    camouhost_file: pathlib.Path,
    browser: pathlib.Path,
    profile_root: pathlib.Path,
) -> tuple[dict[str, object], object]:
    """Mirror the *installed* Camouhost's launch options in a fresh TEMP profile.

    This deliberately calls the installed adapter's existing builder instead of
    copying or second-guessing Firefox preferences, bootstrap, UBO policy or
    BrowserForge config. It neither calls the production IPC nor reads any CRM
    profile. The returned config is only needed for the two ordinary adapter
    browser-visible observations. Never write it to a persistent profile.
    """
    spec = importlib.util.spec_from_file_location("c02_pinned_native_adapter", camouhost_file)
    if spec is None or spec.loader is None:
        raise ValueError("invalid installed Camouhost module")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    from camoufox.fingerprints import generate_fingerprint
    from camoufox.utils import get_screen_cons, launch_options

    lock, _ = module.load_runtime_lock()
    executable = module.packaged_windows_browser(lock)
    if executable.resolve(strict=True) != browser.resolve(strict=True):
        raise ValueError("installed Camouhost and browser runtime identity differ")
    environment = module.browser_environment()
    with contextlib.redirect_stdout(sys.stderr):
        fingerprint = generate_fingerprint(
            screen=get_screen_cons("DISPLAY" in environment), os="windows",
        )
        launch = launch_options(
            **module.camoufox_browser_selector(lock),
            enable_cache=True,
            env=environment,
            exclude_addons=module.default_addon_exclusions(),
            ff_version=module.locked_firefox_major(lock),
            fingerprint=fingerprint,
            headless=False,
            i_know_what_im_doing=True,
            os="windows",
        )
    config = module.extract_camoufox_config(launch)
    return module.camoufox_kwargs(lock, profile_root.parent, config), (module, config)


def single_trial(
    browser: pathlib.Path, target: str, disable_workers: bool,
    disable_javascript: bool, block_webgl: bool,
    profile_root: pathlib.Path, check_worker_constructor: bool,
    exclude_ubo: bool,
    pinned_ubo_dir: pathlib.Path | None,
    shipping_camouhost: pathlib.Path | None = None,
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
    if exclude_ubo:
        # Avoid Camoufox's mutable default-addon download while testing the
        # shipping release path. Add an exact pre-extracted addon separately.
        from camoufox.addons import DefaultAddons
        kwargs["exclude_addons"] = [DefaultAddons.UBO]
    if pinned_ubo_dir is not None:
        kwargs["addons"] = [str(pinned_ubo_dir)]

    shipping_adapter = None
    try:
        if shipping_camouhost is not None:
            kwargs, shipping_adapter = shipping_launch_options(
                shipping_camouhost, browser, profile_root,
            )
        with Camoufox(**kwargs) as context:
            if shipping_adapter is None:
                page = context.new_page()
            else:
                module, config = shipping_adapter
                page = context.pages[0] if context.pages else context.new_page()
                context.set_offline(True)
                # Identical ordinary Camouhost pre-admission browser observations.
                module.stable_probe_digest(page)
                module.browser_visible_observation(page, config)
                module.admit_navigation(context, None)
                page = context.pages[0] if context.pages else context.new_page()
            page.on("crash", callback)
            # A constructor check is diagnostic, NOT a privileged Firefox
            # about:config/pref readback and not proof that workers are disabled.
            if check_worker_constructor:
                try:
                    observed_workers = "present" if page.evaluate(
                        "() => typeof Worker === 'function'"
                    ) else "absent"
                except Exception:
                    observed_workers = "unknown"
            # The opt-in no-precheck variant tests a possible Juggler
            # navigation/readiness race without adding sleeps or implicit
            # navigation to the real browser owner.
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
    parser.add_argument("--exclude-ubo", action="store_true",
                        help="match the current shipping Camouhost addon exclusions")
    parser.add_argument("--pinned-ubo-dir", type=pathlib.Path,
                        help="opt-in absolute pre-extracted verified UBO dir; no download")
    parser.add_argument("--skip-worker-check", action="store_true",
                        help="navigate without pre-navigation Playwright evaluate")
    parser.add_argument("--shipping-camouhost", type=pathlib.Path,
                        help="opt-in exact installed real.py; mirror its native launch/admission")
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
    if config.shipping_camouhost is not None:
        # A faithful full native adapter comparator cannot combine with
        # temporary prefs, extra addons or overrides not used by shipping.
        if (
            not config.shipping_camouhost.is_absolute()
            or not config.shipping_camouhost.is_file()
            or config.shipping_camouhost.name != "real.py"
            or not config.exclude_ubo
            or config.disable_workers or config.disable_javascript
            or config.block_webgl or config.pinned_ubo_dir is not None
        ):
            print("CAMOUFOX_MANAGED_PROBE=INVALID_SHIPPING_COMPARATOR", flush=True)
            return 2
    if config.pinned_ubo_dir is not None:
        # Test-only source directory is created by the operator from a
        # separately SHA-256 verified official XPI, not from a runtime URL.
        manifest = config.pinned_ubo_dir / "manifest.json"
        try:
            version = __import__("json").loads(
                manifest.read_text(encoding="utf-8")
            ).get("version")
        except (OSError, ValueError, AttributeError):
            version = None
        if (
            not config.exclude_ubo
            or not config.pinned_ubo_dir.is_absolute()
            or config.pinned_ubo_dir.is_symlink()
            or version not in {"1.75.0", "1.75.4"}
        ):
            print("CAMOUFOX_MANAGED_PROBE=INVALID_PINNED_UBO", flush=True)
            return 2

    variant = ("workers_off" if config.disable_workers else "workers_default")
    if config.disable_javascript:
        variant += "_javascript_off"
    if config.block_webgl:
        variant += "_webgl_blocked"
    if config.skip_worker_check:
        variant += "_no_precheck"
    if config.exclude_ubo:
        variant += "_ubo_excluded"
    if config.pinned_ubo_dir is not None:
        variant += "_pinned_ubo"
    if config.shipping_camouhost is not None:
        variant += "_installed_camouhost"
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
                    not config.skip_worker_check,
                    config.exclude_ubo,
                    config.pinned_ubo_dir,
                    config.shipping_camouhost,
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
