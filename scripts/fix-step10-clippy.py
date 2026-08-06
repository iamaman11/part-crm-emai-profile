#!/usr/bin/env python3
"""Apply exact Clippy corrections found by the Step 10 bootstrap."""

from pathlib import Path

path = Path(__file__).resolve().parents[1] / "crates/certification-domain/src/lib.rs"
text = path.read_text(encoding="utf-8")

old = "    #[must_use]\n    pub fn rules(&self) -> impl Iterator<Item = &SignalRule> {"
new = "    pub fn rules(&self) -> impl Iterator<Item = &SignalRule> {"
if text.count(old) != 1:
    raise SystemExit(f"rules must_use: expected one match, found {text.count(old)}")
text = text.replace(old, new, 1)

old = "#[derive(Clone, Copy, Debug, Eq, PartialEq)]\npub enum UpdateState {\n    Idle,"
new = "#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]\npub enum UpdateState {\n    #[default]\n    Idle,"
if text.count(old) != 1:
    raise SystemExit(f"UpdateState derive: expected one match, found {text.count(old)}")
text = text.replace(old, new, 1)

old = "\nimpl Default for UpdateState {\n    fn default() -> Self {\n        Self::Idle\n    }\n}\n"
if text.count(old) != 1:
    raise SystemExit(f"UpdateState manual default: expected one match, found {text.count(old)}")
text = text.replace(old, "\n", 1)

path.write_text(text, encoding="utf-8")
