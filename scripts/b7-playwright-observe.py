"""Read-only first-operation B7 evidence against the already-owned Camoufox context.

Executed inside Camouhost by the trusted local operator. Never create/navigate/
close a page. Keep all raw profile test data locally; do not post to GitHub.
"""
import json
import os
import secrets
from pathlib import Path

observed = tuple(context.pages)  # BEFORE any agent action or page creation.
report = {"session": session_id, "count_before": len(observed), "pages": [],
          "cookies": None, "errors": []}
for index, page in enumerate(observed):
    entry = {"index": index, "url": page.url}
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
            report["errors"].append({"page": index, "operation": field, "error": repr(error)})
    try:
        screenshot = f"b7-restore-{index}.png"
        page.screenshot(path=str(agent_artifacts_dir / screenshot))
        entry["screenshot"] = screenshot
    except Exception as error:
        report["errors"].append({"page": index, "operation": "screenshot", "error": repr(error)})
try:
    report["cookies"] = context.cookies()
except Exception as error:
    report["errors"].append({"operation": "cookies", "error": repr(error)})
report["count_after"] = len(context.pages)
if report["count_after"] != report["count_before"]:
    report["errors"].append({"operation": "page_count_changed", "error": "the live context changed during observation"})
artifact = Path(agent_artifacts_dir) / f"b7-first-restore-{secrets.token_hex(8)}.json"
# Keep full results on disk, not truncated by the compact UI status.
with artifact.open("x", encoding="utf-8") as output:
    json.dump(report, output, ensure_ascii=False, default=str)
    output.flush()
    os.fsync(output.fileno())
result = {"artifact": str(artifact), "before": report["count_before"],
          "after": report["count_after"], "errors": report["errors"]}
if report["errors"]:
    raise AssertionError(f"B7 first-open observation incomplete: {len(report['errors'])} errors; see {artifact}")
