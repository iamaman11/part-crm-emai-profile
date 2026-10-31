"""Read-only first-operation B7 evidence against the already-owned Camoufox context.

Executed inside Camouhost by the trusted local operator. Never create/navigate/
close a page. Keep all raw profile test data locally; do not post to GitHub.
"""
import json
import os
import secrets
from pathlib import Path

# Snapshot handles and URLs before touching Playwright. Blank or still-loading
# tabs cannot be silently promoted to proof that Firefox restored a session.
observed = tuple(context.pages)
urls_before = [page.url for page in observed]
report = {
    "session": session_id, "count_before": len(observed),
    "urls_before": urls_before, "pages": [], "cookies": None,
    "not_applicable": [], "errors": [],
}
for index, page in enumerate(observed):
    entry = {"index": index, "url": urls_before[index]}
    report["pages"].append(entry)
    for field, read in (
        ("title", page.title),
        ("html", page.content),
        ("origin_storage", lambda p=page: p.evaluate(
            """() => ({local: {...localStorage}, session: {...sessionStorage}})"""
        )),
    ):
        try:
            entry[field] = read()
        except Exception as error:
            # Firefox's isolated about:blank has no origin storage. Only that
            # precise storage/security combination is inapplicable. A real
            # HTTPS storage error or a closed page remains an actual failure.
            if (field == "origin_storage" and entry["url"] == "about:blank"
                    and "SecurityError" in repr(error)):
                entry[field] = {
                    "status": "NOT_APPLICABLE",
                    "reason": "about_blank_opaque_origin_security_error",
                }
                report["not_applicable"].append({
                    "page": index, "operation": field,
                    "reason": "about_blank_opaque_origin_security_error",
                    "error": repr(error),
                })
            else:
                report["errors"].append({
                    "page": index, "operation": field, "error": repr(error),
                })
    try:
        screenshot = f"b7-restore-{index}.png"
        page.screenshot(path=str(agent_artifacts_dir / screenshot))
        entry["screenshot"] = screenshot
    except Exception as error:
        report["errors"].append({
            "page": index, "operation": "screenshot", "error": repr(error),
        })
try:
    report["cookies"] = context.cookies()
except Exception as error:
    report["errors"].append({"operation": "cookies", "error": repr(error)})

pages_after = tuple(context.pages)
report["count_after"] = len(pages_after)
report["urls_after"] = [page.url for page in pages_after]
if report["count_after"] != report["count_before"]:
    report["errors"].append({
        "operation": "page_count_changed",
        "error": "the live context changed during observation",
    })
if report["urls_before"] != report["urls_after"]:
    report["restore_stage"] = "URLS_CHANGED_DURING_OBSERVATION"
elif not any(url != "about:blank" for url in urls_before):
    report["restore_stage"] = "BLANK_ONLY_RESTORE_NOT_PROVEN"
else:
    report["restore_stage"] = "URLS_OBSERVED_COMPLETENESS_NOT_PROVEN"

artifact = Path(agent_artifacts_dir) / f"b7-first-restore-{secrets.token_hex(8)}.json"
# Keep full results on disk, not truncated by the compact UI status.
with artifact.open("x", encoding="utf-8") as output:
    json.dump(report, output, ensure_ascii=False, default=str)
    output.flush()
    os.fsync(output.fileno())
result = {
    "artifact": str(artifact), "before": report["count_before"],
    "after": report["count_after"], "errors": report["errors"],
    "not_applicable": report["not_applicable"],
    "restore_stage": report["restore_stage"],
}
if report["errors"]:
    raise AssertionError(f"B7 first-open observation incomplete: errors={report['errors']!r}; see {artifact}")
