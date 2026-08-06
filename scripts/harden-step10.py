#!/usr/bin/env python3
"""Apply bounded fail-closed hardening to Repository Step 10."""

from pathlib import Path


def replace_once(text: str, old: str, new: str, label: str) -> str:
    if new in text:
        return text
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"{label}: expected one match, found {count}")
    return text.replace(old, new, 1)


root = Path(__file__).resolve().parents[1]
lib = root / "crates/certification-domain/src/lib.rs"
text = lib.read_text(encoding="utf-8")

text = replace_once(
    text,
    "        let mut indexed = BTreeMap::new();\n        for rule in rules {\n            if indexed.insert(rule.name.clone(), rule).is_some() {\n                return Err(CertificationError::DuplicateSignal);\n            }\n        }\n        Ok(Self {\n            version,\n            rules: indexed,\n        })",
    "        let mut indexed = BTreeMap::new();\n        let mut required_rules = 0_u32;\n        for rule in rules {\n            if rule.requirement == SignalRequirement::Required {\n                required_rules = required_rules\n                    .checked_add(1)\n                    .ok_or(CertificationError::CounterOverflow)?;\n            }\n            if indexed.insert(rule.name.clone(), rule).is_some() {\n                return Err(CertificationError::DuplicateSignal);\n            }\n        }\n        if required_rules == 0 {\n            return Err(CertificationError::InvalidPolicy);\n        }\n        Ok(Self {\n            version,\n            rules: indexed,\n        })",
    "required signal policy",
)

text = replace_once(
    text,
    "#[derive(Clone, Debug, Eq, PartialEq)]\npub struct ObservationSet {",
    "#[derive(Clone, Eq, PartialEq)]\npub struct ObservationSet {",
    "raw observation Debug boundary",
)

text = replace_once(
    text,
    "            \"schema=certification-summary-v1\\npolicy_version={}\\nobservation_count={}\\nevaluated_signals={}\\ndrifted_signals={}\\nmissing_required_signals={}\\nprohibited_signals={}\\noutcome={}\\nmatrix_digest={}\\n\",\n            self.policy_version,\n            self.observation_count,\n            self.evaluated_signals,\n            self.drifted_signals,\n            self.missing_required_signals,\n            self.prohibited_signals,\n            self.outcome.as_str(),\n            self.matrix_digest.to_hex(),",
    "            \"schema=certification-summary-v1\\npolicy_version={}\\nobservation_count={}\\nevaluated_signals={}\\ndrifted_signals={}\\nmissing_required_signals={}\\nprohibited_signals={}\\noutcome={}\\n\",\n            self.policy_version,\n            self.observation_count,\n            self.evaluated_signals,\n            self.drifted_signals,\n            self.missing_required_signals,\n            self.prohibited_signals,\n            self.outcome.as_str(),",
    "metadata-only certification summary",
)

anchor = "        assert_eq!(\n            SignalRule::new(signal(\"raw.secret\")?, SignalRequirement::Prohibited, 1),\n            Err(CertificationError::InvalidTolerance)\n        );\n"
addition = anchor + "        assert_eq!(\n            CertificationPolicy::new(\n                1,\n                vec![SignalRule::new(\n                    signal(\"optional.signal\")?,\n                    SignalRequirement::Optional,\n                    0,\n                )?],\n            ),\n            Err(CertificationError::InvalidPolicy)\n        );\n"
text = replace_once(text, anchor, addition, "required signal regression")

anchor = "        assert!(!certification.contains(\"raw.secret\"));\n"
addition = anchor + "        assert!(!certification.contains(&report.matrix_digest().to_hex()));\n"
text = replace_once(text, anchor, addition, "matrix digest support privacy proof")

lib.write_text(text, encoding="utf-8")

docs = root / "docs/CERTIFICATION_MULTI_DEVICE.md"
text = docs.read_text(encoding="utf-8")
text = replace_once(
    text,
    "A policy has a non-zero version and a unique sorted set of signal rules. Every\nrule is exactly one of:\n",
    "A policy has a non-zero version, at least one required signal and a unique\nsorted set of signal rules. Every rule is exactly one of:\n",
    "required signal documentation",
)
text = replace_once(
    text,
    "The report exposes only policy version, observation count, aggregate result\ncounts, outcome and matrix digest. Raw signal names and values are not accepted\nby the metadata-only support renderer.\n",
    "The internal report exposes policy version, observation count, aggregate result\ncounts, outcome and matrix digest for controlled evidence comparison. The\nmetadata-only support renderer omits the matrix digest as well as raw signal names\nand values, preventing a value-derived identifier from becoming support telemetry.\n",
    "matrix digest privacy documentation",
)
text = replace_once(
    text,
    "Certification, device and update support summaries expose aggregate counts,\nversions, state and matrix digest only. They exclude:\n",
    "Certification, device and update support summaries expose aggregate counts,\nversions and state only. The certification matrix digest remains available to\ncontrolled internal evidence but is omitted from support output. They exclude:\n",
    "support output documentation",
)
docs.write_text(text, encoding="utf-8")

policy = root / "scripts/check-step10-certification.py"
text = policy.read_text(encoding="utf-8")
text = replace_once(
    text,
    "    \"CertificationOutcome::Prohibited\",\n",
    "    \"CertificationOutcome::Prohibited\",\n    \"required_rules\",\n",
    "required signal policy fragment",
)
text = replace_once(
    text,
    "    \"reqwest::\",\n)",
    "    \"reqwest::\",\n    \"#[derive(Clone, Debug, Eq, PartialEq)]\\npub struct ObservationSet\",\n    \"matrix_digest={}\",\n)",
    "privacy forbidden fragments",
)
policy.write_text(text, encoding="utf-8")
