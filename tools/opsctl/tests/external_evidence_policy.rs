use opsctl::canonical::{canonical_pretty_json, parse_strict_json_with_limits};
use opsctl_core::external_evidence::{
    ExternalCheckOutcome, ExternalEnvironment, ExternalEvidenceCheck, ExternalEvidenceRecordV1,
    ExternalEvidenceStatus, ExternalGate, ExternalReadinessSummaryV1, mandatory_requirements,
    validate_external_evidence,
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::borrow::Cow;
use std::collections::BTreeSet;
use std::fmt::{Display, Formatter};
use std::fs;
use std::net::{Ipv4Addr, Ipv6Addr};
use std::path::{Path, PathBuf};
use std::str::FromStr;

const MAX_RECORD_BYTES: usize = 64 * 1024;
const MAX_RECORD_DEPTH: usize = 16;

#[derive(Debug)]
struct AdapterError(String);

impl AdapterError {
    fn new(message: impl Into<String>) -> Self {
        Self(message.into())
    }
}

impl Display for AdapterError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for AdapterError {}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ScopeDto {
    environment: String,
    subject_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CheckDto {
    code: String,
    outcome: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ReviewDto {
    github_login: String,
    review_reference: String,
    reviewed_at: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RecordDto {
    schema_version: u64,
    evidence_id: String,
    gate: String,
    status: String,
    observed_at: String,
    scope: ScopeDto,
    checks: Vec<CheckDto>,
    references: Vec<String>,
    artifact_digests_sha256: Vec<String>,
    limitations: Vec<String>,
    review: Option<ReviewDto>,
    supersedes: Option<String>,
}

fn repository_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn normalize_repository_text(value: &str) -> Cow<'_, str> {
    if value.contains('\r') {
        Cow::Owned(value.replace("\r\n", "\n").replace('\r', "\n"))
    } else {
        Cow::Borrowed(value)
    }
}

fn validate_tree(root: &Path) -> Result<ExternalReadinessSummaryV1, AdapterError> {
    let records_dir = root.join("evidence/external/records");
    if !records_dir.is_dir() {
        return Err(AdapterError::new(format!(
            "missing external evidence records directory: {}",
            records_dir.display()
        )));
    }
    let mut paths = fs::read_dir(&records_dir)
        .map_err(|error| {
            AdapterError::new(format!("cannot read {}: {error}", records_dir.display()))
        })?
        .map(|entry| entry.map(|value| value.path()))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| AdapterError::new(format!("cannot enumerate records: {error}")))?;
    paths.sort();

    let mut records = Vec::new();
    for path in paths {
        let Some(name) = path.file_name().and_then(|value| value.to_str()) else {
            return Err(AdapterError::new(
                "external evidence path is not valid UTF-8",
            ));
        };
        if matches!(name, "README.md" | ".gitkeep") {
            continue;
        }
        if path.extension().and_then(|value| value.to_str()) != Some("json") {
            return Err(AdapterError::new(format!(
                "unexpected external evidence file: {}",
                path.display()
            )));
        }
        records.push(parse_record(&path)?);
    }
    validate_external_evidence(&records).map_err(|error| AdapterError::new(error.to_string()))
}

fn parse_record(path: &Path) -> Result<ExternalEvidenceRecordV1, AdapterError> {
    let bytes = fs::read(path)
        .map_err(|error| AdapterError::new(format!("cannot read {}: {error}", path.display())))?;
    if bytes.len() > MAX_RECORD_BYTES {
        return Err(AdapterError::new(format!(
            "{} exceeds {MAX_RECORD_BYTES} bytes",
            path.display()
        )));
    }
    let raw = std::str::from_utf8(&bytes)
        .map_err(|error| AdapterError::new(format!("{} is not UTF-8: {error}", path.display())))?;
    reject_sensitive_text(raw, path)?;
    let value = parse_strict_json_with_limits(raw, MAX_RECORD_BYTES, MAX_RECORD_DEPTH)
        .map_err(|error| AdapterError::new(format!("{}: {error}", path.display())))?;
    let canonical = canonical_pretty_json(&value)
        .map_err(|error| AdapterError::new(format!("{}: {error}", path.display())))?;
    if normalize_repository_text(raw).as_ref() != canonical.as_str() {
        return Err(AdapterError::new(format!(
            "{} is not canonical pretty JSON",
            path.display()
        )));
    }
    let dto: RecordDto = serde_json::from_value(value)
        .map_err(|error| AdapterError::new(format!("{} schema: {error}", path.display())))?;
    if dto.schema_version != 1 {
        return Err(AdapterError::new(format!(
            "{} unsupported schema_version",
            path.display()
        )));
    }
    let evidence_date = validate_evidence_id(&dto.evidence_id)?;
    let expected_name = format!("{}.json", dto.evidence_id);
    if path.file_name().and_then(|value| value.to_str()) != Some(expected_name.as_str()) {
        return Err(AdapterError::new(format!(
            "{} evidence_id does not match filename",
            path.display()
        )));
    }
    let gate = ExternalGate::parse(&dto.gate)
        .ok_or_else(|| AdapterError::new(format!("unsupported external gate {}", dto.gate)))?;
    let status = ExternalEvidenceStatus::parse(&dto.status)
        .ok_or_else(|| AdapterError::new(format!("unsupported evidence status {}", dto.status)))?;
    let environment = ExternalEnvironment::parse(&dto.scope.environment).ok_or_else(|| {
        AdapterError::new(format!(
            "unsupported evidence environment {}",
            dto.scope.environment
        ))
    })?;
    validate_subject_id(&dto.scope.subject_id)?;
    let (observed_at_sort_key, observed_date) = parse_timestamp(&dto.observed_at)?;

    validate_bounded_unique_strings(&dto.references, 1, 10, "references")?;
    for reference in &dto.references {
        validate_reference(reference, false)?;
    }
    validate_bounded_unique_strings(
        &dto.artifact_digests_sha256,
        0,
        10,
        "artifact_digests_sha256",
    )?;
    for digest in &dto.artifact_digests_sha256 {
        if !is_lower_hex(digest, 64) {
            return Err(AdapterError::new(
                "invalid external evidence SHA-256 digest",
            ));
        }
    }
    validate_bounded_unique_strings(&dto.limitations, 0, 20, "limitations")?;
    for limitation in &dto.limitations {
        if !is_token(limitation) {
            return Err(AdapterError::new(
                "external evidence limitation must be a bounded token",
            ));
        }
    }

    let checks = dto
        .checks
        .into_iter()
        .map(|check| {
            let outcome = ExternalCheckOutcome::parse(&check.outcome).ok_or_else(|| {
                AdapterError::new(format!("unsupported check outcome {}", check.outcome))
            })?;
            if !is_token(&check.code) {
                return Err(AdapterError::new(
                    "external evidence check code is malformed",
                ));
            }
            Ok(ExternalEvidenceCheck {
                code: check.code,
                outcome,
            })
        })
        .collect::<Result<Vec<_>, AdapterError>>()?;

    let reviewed_at_sort_key = if let Some(review) = &dto.review {
        if !is_github_login(&review.github_login) {
            return Err(AdapterError::new(
                "external evidence reviewer login is malformed",
            ));
        }
        validate_reference(&review.review_reference, true)?;
        Some(parse_timestamp(&review.reviewed_at)?.0)
    } else {
        None
    };
    if let Some(supersedes) = &dto.supersedes {
        validate_evidence_id(supersedes)?;
        if supersedes == &dto.evidence_id {
            return Err(AdapterError::new(
                "external evidence cannot supersede itself",
            ));
        }
    }

    Ok(ExternalEvidenceRecordV1 {
        evidence_id: dto.evidence_id,
        evidence_date,
        gate,
        status,
        observed_at_sort_key,
        observed_date,
        environment,
        checks,
        artifact_digest_count: dto.artifact_digests_sha256.len(),
        has_review: dto.review.is_some(),
        reviewed_at_sort_key,
        supersedes: dto.supersedes,
    })
}

fn validate_projection(
    root: &Path,
    summary: &ExternalReadinessSummaryV1,
) -> Result<(), AdapterError> {
    let expected = render_summary(summary)?;
    let summary_path = root.join("docs/external-evidence-summary.json");
    let actual = fs::read_to_string(&summary_path).map_err(|error| {
        AdapterError::new(format!("cannot read {}: {error}", summary_path.display()))
    })?;
    if normalize_repository_text(&actual).as_ref() != expected.as_str() {
        return Err(AdapterError::new(format!(
            "{} differs from typed Rust readiness projection",
            summary_path.display()
        )));
    }
    validate_status(root, summary.eligible_for_production_review)
}

fn validate_status(root: &Path, eligible: bool) -> Result<(), AdapterError> {
    let path = root.join("docs/status.json");
    let raw = fs::read_to_string(&path)
        .map_err(|error| AdapterError::new(format!("cannot read {}: {error}", path.display())))?;
    let value = parse_strict_json_with_limits(&raw, 256 * 1024, 32)
        .map_err(|error| AdapterError::new(format!("{}: {error}", path.display())))?;
    let production_ready = value
        .get("production_ready")
        .and_then(Value::as_bool)
        .ok_or_else(|| AdapterError::new("docs/status.json production_ready must be boolean"))?;
    if production_ready && !eligible {
        return Err(AdapterError::new(
            "production_ready cannot be true while external evidence is incomplete",
        ));
    }
    Ok(())
}

fn render_summary(summary: &ExternalReadinessSummaryV1) -> Result<String, AdapterError> {
    let mut active = summary.active_records.clone();
    active.sort_by(|left, right| {
        left.gate
            .as_str()
            .cmp(right.gate.as_str())
            .then(left.environment.as_str().cmp(right.environment.as_str()))
            .then(left.evidence_id.cmp(&right.evidence_id))
    });
    let active_records = active
        .iter()
        .map(|record| {
            json!({
                "environment": record.environment.as_str(),
                "evidence_id": record.evidence_id,
                "gate": record.gate.as_str(),
                "observed_date": format_date(record.observed_date),
                "status": record.status.as_str(),
            })
        })
        .collect::<Vec<_>>();
    let mandatory = mandatory_requirements()
        .iter()
        .map(|(gate, environment)| {
            json!({"environment": environment.as_str(), "gate": gate.as_str()})
        })
        .collect::<Vec<_>>();
    let missing = summary
        .missing_requirements
        .iter()
        .map(|(gate, environment)| {
            json!({"environment": environment.as_str(), "gate": gate.as_str()})
        })
        .collect::<Vec<_>>();
    canonical_pretty_json(&json!({
        "active_records": active_records,
        "counts": {
            "active_failed": summary.active_failed,
            "active_passed": summary.active_passed,
            "active_pending": summary.active_pending,
            "mandatory_requirements": summary.mandatory_requirements,
            "satisfied_requirements": summary.satisfied_requirements,
            "total_records": summary.total_records,
        },
        "eligible_for_production_review": summary.eligible_for_production_review,
        "mandatory_requirements": mandatory,
        "missing_requirements": missing,
        "policy_version": 1,
        "schema_version": 1,
    }))
    .map_err(AdapterError::new)
}

fn validate_evidence_id(value: &str) -> Result<u32, AdapterError> {
    let Some(rest) = value.strip_prefix("ev-") else {
        return Err(AdapterError::new(
            "external evidence id must start with ev-",
        ));
    };
    let Some((date, suffix)) = rest.split_once('-') else {
        return Err(AdapterError::new("external evidence id is missing suffix"));
    };
    if date.len() != 8 || !date.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(AdapterError::new("external evidence id date is malformed"));
    }
    if !(3..=48).contains(&suffix.len())
        || !suffix
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        || !suffix
            .as_bytes()
            .first()
            .is_some_and(u8::is_ascii_alphanumeric)
    {
        return Err(AdapterError::new(
            "external evidence id suffix is malformed",
        ));
    }
    date.parse::<u32>()
        .map_err(|error| AdapterError::new(format!("invalid evidence date: {error}")))
}

fn validate_subject_id(value: &str) -> Result<(), AdapterError> {
    if value == "none" || is_token(value) {
        return Ok(());
    }
    Err(AdapterError::new(
        "external evidence subject_id must be opaque and token-shaped",
    ))
}

fn is_token(value: &str) -> bool {
    (3..=96).contains(&value.len())
        && value.as_bytes().first().is_some_and(u8::is_ascii_lowercase)
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'.' | b'_' | b'-')
        })
}

fn is_github_login(value: &str) -> bool {
    if value.is_empty() || value.len() > 39 {
        return false;
    }
    let bytes = value.as_bytes();
    bytes.first().is_some_and(u8::is_ascii_alphanumeric)
        && bytes.last().is_some_and(u8::is_ascii_alphanumeric)
        && bytes
            .iter()
            .all(|byte| byte.is_ascii_alphanumeric() || *byte == b'-')
}

fn validate_reference(value: &str, terminal_review: bool) -> Result<(), AdapterError> {
    if let Some(path) = value.strip_prefix("https://github.com/") {
        if path.contains('?') || path.contains('@') || path.contains("/../") || path.contains("/./")
        {
            return Err(AdapterError::new("unsafe GitHub evidence reference"));
        }
        let parts = path.split('/').filter(|part| !part.is_empty()).count();
        if parts < 4 {
            return Err(AdapterError::new(
                "GitHub evidence reference is not reviewable",
            ));
        }
        if terminal_review
            && !(value.contains("#issuecomment-")
                || value.contains("#pullrequestreview-")
                || value.contains("#discussion_r"))
        {
            return Err(AdapterError::new(
                "terminal evidence review must identify an exact GitHub review/comment",
            ));
        }
        return Ok(());
    }
    if terminal_review {
        return Err(AdapterError::new(
            "terminal evidence review must use a GitHub reference",
        ));
    }
    if let Some(token) = value.strip_prefix("provider-case:") {
        if is_token(token) {
            return Ok(());
        }
    }
    if let Some(digest) = value.strip_prefix("review-report:sha256:") {
        if is_lower_hex(digest, 64) {
            return Ok(());
        }
    }
    Err(AdapterError::new("unsupported external evidence reference"))
}

fn validate_bounded_unique_strings(
    values: &[String],
    minimum: usize,
    maximum: usize,
    field: &str,
) -> Result<(), AdapterError> {
    if !(minimum..=maximum).contains(&values.len()) {
        return Err(AdapterError::new(format!(
            "{field} count must be between {minimum} and {maximum}"
        )));
    }
    let unique = values.iter().map(String::as_str).collect::<BTreeSet<_>>();
    if unique.len() != values.len() {
        return Err(AdapterError::new(format!(
            "{field} contains duplicate values"
        )));
    }
    Ok(())
}

fn parse_timestamp(value: &str) -> Result<(u64, u32), AdapterError> {
    let bytes = value.as_bytes();
    if bytes.len() != 20
        || bytes[4] != b'-'
        || bytes[7] != b'-'
        || bytes[10] != b'T'
        || bytes[13] != b':'
        || bytes[16] != b':'
        || bytes[19] != b'Z'
        || bytes.iter().enumerate().any(|(index, byte)| {
            !matches!(index, 4 | 7 | 10 | 13 | 16 | 19) && !byte.is_ascii_digit()
        })
    {
        return Err(AdapterError::new(
            "timestamp must use exact whole-second YYYY-MM-DDTHH:MM:SSZ",
        ));
    }
    let year = parse_decimal(&value[0..4])?;
    let month = parse_decimal(&value[5..7])?;
    let day = parse_decimal(&value[8..10])?;
    let hour = parse_decimal(&value[11..13])?;
    let minute = parse_decimal(&value[14..16])?;
    let second = parse_decimal(&value[17..19])?;
    if !(1..=12).contains(&month)
        || day == 0
        || day > days_in_month(year, month)
        || hour > 23
        || minute > 59
        || second > 59
    {
        return Err(AdapterError::new(
            "timestamp contains an invalid UTC calendar value",
        ));
    }
    let date = year * 10_000 + month * 100 + day;
    let sort_key = u64::from(date) * 1_000_000
        + u64::from(hour) * 10_000
        + u64::from(minute) * 100
        + u64::from(second);
    Ok((sort_key, date))
}

fn parse_decimal(value: &str) -> Result<u32, AdapterError> {
    value
        .parse::<u32>()
        .map_err(|error| AdapterError::new(format!("invalid timestamp number: {error}")))
}

const fn days_in_month(year: u32, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap_year(year) => 29,
        2 => 28,
        _ => 0,
    }
}

const fn is_leap_year(year: u32) -> bool {
    year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400))
}

fn reject_sensitive_text(raw: &str, path: &Path) -> Result<(), AdapterError> {
    let lower = raw.to_ascii_lowercase();
    for marker in [
        "-----begin ",
        "authorization:",
        "bearer ",
        "basic ",
        "c:\\users\\",
        "/home/",
        "/users/",
    ] {
        if lower.contains(marker) {
            return Err(AdapterError::new(format!(
                "{} contains prohibited sensitive material marker",
                path.display()
            )));
        }
    }
    for token in raw.split(|character: char| {
        !(character.is_ascii_alphanumeric()
            || matches!(
                character,
                '@' | '.' | '_' | '%' | '+' | '-' | ':' | '[' | ']'
            ))
    }) {
        if token.contains('@') && token.contains('.') {
            return Err(AdapterError::new(format!(
                "{} contains an email-like identifier",
                path.display()
            )));
        }
        let address_candidate = token.trim_matches(['[', ']']);
        if Ipv4Addr::from_str(address_candidate).is_ok()
            || Ipv6Addr::from_str(address_candidate).is_ok()
        {
            return Err(AdapterError::new(format!(
                "{} contains a raw IP address",
                path.display()
            )));
        }
    }
    Ok(())
}

fn is_lower_hex(value: &str, expected_length: usize) -> bool {
    value.len() == expected_length
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn format_date(value: u32) -> String {
    format!(
        "{:04}-{:02}-{:02}",
        value / 10_000,
        (value / 100) % 100,
        value % 100
    )
}

#[test]
fn current_repository_external_evidence_uses_typed_rust_authority(
) -> Result<(), Box<dyn std::error::Error>> {
    let root = repository_root();
    let summary = validate_tree(&root)?;
    validate_projection(&root, &summary)?;
    assert!(!summary.eligible_for_production_review);
    Ok(())
}

#[test]
fn positive_external_evidence_fixtures_pass() -> Result<(), Box<dyn std::error::Error>> {
    let root = repository_root();
    for fixture in ["valid", "valid-passed"] {
        validate_tree(&root.join("tests/external-evidence/fixtures").join(fixture))?;
    }
    Ok(())
}

#[test]
fn repository_line_endings_normalize_without_weakening_canonical_text(
) -> Result<(), Box<dyn std::error::Error>> {
    let root = repository_root().join("tests/external-readiness/fixtures/empty");
    let summary = validate_tree(&root)?;
    let canonical = render_summary(&summary)?;
    let crlf = canonical.replace('\n', "\r\n");
    let cr = canonical.replace('\n', "\r");
    assert_eq!(normalize_repository_text(&crlf).as_ref(), canonical.as_str());
    assert_eq!(normalize_repository_text(&cr).as_ref(), canonical.as_str());

    let noncanonical = canonical.replacen("\"active_records\":", "\"active_records\" :", 1);
    assert_ne!(
        normalize_repository_text(&noncanonical).as_ref(),
        canonical.as_str()
    );
    Ok(())
}

#[test]
fn unsafe_and_semantically_invalid_external_evidence_fixtures_fail() {
    let root = repository_root();
    for fixture in [
        "secret-bearing",
        "invalid-passed",
        "forked-lineage",
        "dangling-lineage",
        "invalid-timestamp",
        "invalid-environment",
        "invalid-id-date",
        "invalid-ipv6",
    ] {
        assert!(
            validate_tree(&root.join("tests/external-evidence/fixtures").join(fixture)).is_err(),
            "negative fixture unexpectedly passed: {fixture}"
        );
    }
}

#[test]
fn empty_readiness_projection_matches_existing_contract() -> Result<(), Box<dyn std::error::Error>>
{
    let root = repository_root().join("tests/external-readiness/fixtures/empty");
    let summary = validate_tree(&root)?;
    let expected = render_summary(&summary)?;
    let committed = fs::read_to_string(root.join("expected-summary.json"))?;
    assert_eq!(
        expected.as_str(),
        normalize_repository_text(&committed).as_ref()
    );
    let status = fs::read_to_string(root.join("status.json"))?;
    let status_path = root.join("docs/status.json");
    fs::create_dir_all(root.join("docs"))?;
    fs::write(&status_path, status)?;
    validate_status(&root, summary.eligible_for_production_review)?;
    fs::remove_file(status_path)?;
    Ok(())
}

#[test]
fn false_production_readiness_fails_closed() -> Result<(), Box<dyn std::error::Error>> {
    let root = repository_root().join("tests/external-readiness/fixtures/empty");
    let summary = validate_tree(&root)?;
    let status = fs::read_to_string(root.join("status-production-ready.json"))?;
    let status_path = root.join("docs/status.json");
    fs::create_dir_all(root.join("docs"))?;
    fs::write(&status_path, status)?;
    let result = validate_status(&root, summary.eligible_for_production_review);
    fs::remove_file(status_path)?;
    assert!(result.is_err());
    Ok(())
}
