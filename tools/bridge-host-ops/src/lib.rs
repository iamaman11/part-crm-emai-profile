#![forbid(unsafe_code)]

use std::error::Error;
use std::fmt::{Display, Formatter};

pub const SCHEMA_VERSION: &str = "bridge-host-ops/v1";
pub const CERTIFICATE_STORE: &str = "LocalMachine/My";
pub const CLIENT_AUTH_EKU_OID: &str = "1.3.6.1.5.5.7.3.2";
pub const SHIPPING_DEVICE_ID_ENV: &str = "PROFILE_BRIDGE_DEVICE_ID";
pub const SHIPPING_CERT_SHA1_ENV: &str = "PROFILE_BRIDGE_MACHINE_CERT_SHA1";
pub const SHIPPING_ORIGIN_ENV: &str = "PROFILE_BRIDGE_CONTROL_PLANE_ORIGIN";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HostOpsError {
    code: &'static str,
}

impl HostOpsError {
    #[must_use]
    pub const fn new(code: &'static str) -> Self {
        Self { code }
    }

    #[must_use]
    pub const fn code(self) -> &'static str {
        self.code
    }
}

impl Display for HostOpsError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.code)
    }
}

impl Error for HostOpsError {}

pub type HostOpsResult<T> = Result<T, HostOpsError>;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CertificateObservation {
    sha1_thumbprint: String,
    sha256_fingerprint: String,
}

impl CertificateObservation {
    #[must_use]
    pub fn sha1_thumbprint(&self) -> &str {
        &self.sha1_thumbprint
    }

    #[must_use]
    pub fn sha256_fingerprint(&self) -> &str {
        &self.sha256_fingerprint
    }

    #[must_use]
    pub fn selector(&self) -> String {
        format!("LocalMachine\\MY\\{}", self.sha1_thumbprint)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ImportObservation {
    sha1_thumbprint: String,
    preexisting: bool,
}

impl ImportObservation {
    #[must_use]
    pub fn sha1_thumbprint(&self) -> &str {
        &self.sha1_thumbprint
    }

    #[must_use]
    pub const fn preexisting(&self) -> bool {
        self.preexisting
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MutationReceipt {
    result_code: String,
    resource_id: String,
    aggregate_version: u64,
}

impl MutationReceipt {
    #[must_use]
    pub fn result_code(&self) -> &str {
        &self.result_code
    }

    #[must_use]
    pub fn resource_id(&self) -> &str {
        &self.resource_id
    }

    #[must_use]
    pub const fn aggregate_version(&self) -> u64 {
        self.aggregate_version
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BindingHttpPlan {
    method: &'static str,
    url: String,
    body: String,
    correlation_id: String,
    target_actor_id: String,
}

impl BindingHttpPlan {
    #[must_use]
    pub fn target_actor_id(&self) -> &str {
        &self.target_actor_id
    }

    #[must_use]
    pub fn curl_arguments(&self) -> Vec<String> {
        vec![
            "--silent".to_owned(),
            "--show-error".to_owned(),
            "--no-progress-meter".to_owned(),
            "--connect-timeout".to_owned(),
            "10".to_owned(),
            "--max-time".to_owned(),
            "30".to_owned(),
            "--proto".to_owned(),
            "=https".to_owned(),
            "--noproxy".to_owned(),
            "*".to_owned(),
            "--request".to_owned(),
            self.method.to_owned(),
            "--header".to_owned(),
            "Accept: application/json".to_owned(),
            "--header".to_owned(),
            "Content-Type: application/json".to_owned(),
            "--header".to_owned(),
            format!("X-Correlation-Id: {}", self.correlation_id),
            "--data-binary".to_owned(),
            self.body.clone(),
            "--write-out".to_owned(),
            "\n%{http_code}".to_owned(),
            "--config".to_owned(),
            "-".to_owned(),
            "--url".to_owned(),
            self.url.clone(),
        ]
    }
}

pub fn normalize_sha1_thumbprint(value: &str) -> HostOpsResult<String> {
    if value.len() != 40 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(HostOpsError::new("invalid_certificate_sha1"));
    }
    Ok(value.to_ascii_uppercase())
}

pub fn validate_sha256_fingerprint(value: &str) -> HostOpsResult<()> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(HostOpsError::new("invalid_certificate_sha256"));
    }
    Ok(())
}

pub fn validate_identifier(value: &str) -> HostOpsResult<()> {
    if !(8..=96).contains(&value.len())
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
    {
        return Err(HostOpsError::new("invalid_identifier"));
    }
    Ok(())
}

pub fn validate_correlation_id(value: &str) -> HostOpsResult<()> {
    validate_identifier(value).map_err(|_| HostOpsError::new("invalid_correlation_id"))
}

pub fn normalize_https_origin(value: &str) -> HostOpsResult<String> {
    if value.trim() != value {
        return Err(HostOpsError::new("invalid_control_plane_origin"));
    }
    let Some(rest) = value.strip_prefix("https://") else {
        return Err(HostOpsError::new("invalid_control_plane_origin"));
    };
    let authority = rest.strip_suffix('/').unwrap_or(rest);
    if authority.is_empty()
        || authority.contains('/')
        || authority.contains('?')
        || authority.contains('#')
        || authority.contains('@')
        || authority.bytes().any(|byte| byte.is_ascii_whitespace())
        || !authority.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b':' | b'[' | b']')
        })
    {
        return Err(HostOpsError::new("invalid_control_plane_origin"));
    }
    Ok(format!("https://{authority}"))
}

pub fn validate_access_token(value: &str) -> HostOpsResult<()> {
    if value.len() < 32
        || value.len() > 32_768
        || value.bytes().filter(|byte| *byte == b'.').count() != 2
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
    {
        return Err(HostOpsError::new("invalid_access_token"));
    }
    Ok(())
}

pub fn build_access_config(value: &str) -> HostOpsResult<Vec<u8>> {
    validate_access_token(value)?;
    Ok(format!("header = \"cf-access-token: {value}\"\n").into_bytes())
}

pub fn parse_certificate_observation(value: &str) -> HostOpsResult<CertificateObservation> {
    let fields: Vec<&str> = value.trim().split('\t').collect();
    if fields.len() != 5 {
        return Err(HostOpsError::new("invalid_certificate_observation"));
    }
    let sha1_thumbprint = normalize_sha1_thumbprint(fields[0])?;
    validate_sha256_fingerprint(fields[1])?;
    if fields[2] != "1" {
        return Err(HostOpsError::new("certificate_private_key_missing"));
    }
    if fields[3] != "1" {
        return Err(HostOpsError::new("certificate_client_auth_eku_missing"));
    }
    if fields[4] != "1" {
        return Err(HostOpsError::new("certificate_not_currently_valid"));
    }
    Ok(CertificateObservation {
        sha1_thumbprint,
        sha256_fingerprint: fields[1].to_owned(),
    })
}

pub fn parse_import_observation(value: &str) -> HostOpsResult<ImportObservation> {
    let fields: Vec<&str> = value.trim().split('\t').collect();
    if fields.len() != 2 {
        return Err(HostOpsError::new("invalid_import_observation"));
    }
    let sha1_thumbprint = normalize_sha1_thumbprint(fields[0])?;
    let preexisting = match fields[1] {
        "0" => false,
        "1" => true,
        _ => return Err(HostOpsError::new("invalid_import_observation")),
    };
    Ok(ImportObservation {
        sha1_thumbprint,
        preexisting,
    })
}

pub fn binding_write_plan(
    origin: &str,
    tenant_id: &str,
    actor_id: &str,
    device_id: &str,
    certificate_fingerprint: &str,
    expected_previous_version: Option<u64>,
    correlation_id: &str,
) -> HostOpsResult<BindingHttpPlan> {
    let origin = normalize_https_origin(origin)?;
    validate_identifier(tenant_id)?;
    validate_identifier(actor_id)?;
    validate_identifier(device_id)?;
    validate_sha256_fingerprint(certificate_fingerprint)?;
    validate_correlation_id(correlation_id)?;
    if expected_previous_version == Some(0) {
        return Err(HostOpsError::new("invalid_expected_version"));
    }
    let body = match expected_previous_version {
        Some(version) => format!(
            "{{\"deviceId\":{},\"certificateFingerprint\":{},\"expectedPreviousVersion\":{version}}}",
            json_string(device_id),
            json_string(certificate_fingerprint)
        ),
        None => format!(
            "{{\"deviceId\":{},\"certificateFingerprint\":{}}}",
            json_string(device_id),
            json_string(certificate_fingerprint)
        ),
    };
    Ok(BindingHttpPlan {
        method: "PUT",
        url: format!(
            "{origin}/api/v1/tenants/{tenant_id}/members/{actor_id}/device-binding"
        ),
        body,
        correlation_id: correlation_id.to_owned(),
        target_actor_id: actor_id.to_owned(),
    })
}

pub fn binding_revoke_plan(
    origin: &str,
    tenant_id: &str,
    actor_id: &str,
    expected_version: u64,
    correlation_id: &str,
) -> HostOpsResult<BindingHttpPlan> {
    let origin = normalize_https_origin(origin)?;
    validate_identifier(tenant_id)?;
    validate_identifier(actor_id)?;
    validate_correlation_id(correlation_id)?;
    if expected_version == 0 {
        return Err(HostOpsError::new("invalid_expected_version"));
    }
    Ok(BindingHttpPlan {
        method: "DELETE",
        url: format!(
            "{origin}/api/v1/tenants/{tenant_id}/members/{actor_id}/device-binding"
        ),
        body: format!("{{\"expectedVersion\":{expected_version}}}"),
        correlation_id: correlation_id.to_owned(),
        target_actor_id: actor_id.to_owned(),
    })
}

pub fn parse_control_plane_output(value: &str) -> HostOpsResult<MutationReceipt> {
    let Some((body, status_text)) = value.rsplit_once('\n') else {
        return Err(HostOpsError::new("invalid_control_plane_response"));
    };
    let status = status_text
        .trim_end_matches('\r')
        .parse::<u16>()
        .map_err(|_| HostOpsError::new("invalid_control_plane_response"))?;
    if status != 200 {
        return Err(HostOpsError::new("control_plane_rejected"));
    }
    parse_mutation_receipt(body.trim())
}

fn parse_mutation_receipt(value: &str) -> HostOpsResult<MutationReceipt> {
    if value.len() > 8_192 || !value.starts_with('{') || !value.ends_with('}') {
        return Err(HostOpsError::new("invalid_mutation_receipt"));
    }
    let inner = &value[1..value.len() - 1];
    let parts: Vec<&str> = inner.split(',').collect();
    if parts.len() != 3 {
        return Err(HostOpsError::new("invalid_mutation_receipt"));
    }
    let mut result_code: Option<String> = None;
    let mut resource_id: Option<String> = None;
    let mut aggregate_version: Option<u64> = None;
    for part in parts {
        let Some((raw_key, raw_value)) = part.split_once(':') else {
            return Err(HostOpsError::new("invalid_mutation_receipt"));
        };
        let key = parse_plain_json_string(raw_key.trim())?;
        match key.as_str() {
            "resultCode" if result_code.is_none() => {
                let parsed = parse_plain_json_string(raw_value.trim())?;
                validate_receipt_token(&parsed)?;
                result_code = Some(parsed);
            }
            "resourceId" if resource_id.is_none() => {
                let parsed = parse_plain_json_string(raw_value.trim())?;
                validate_identifier(&parsed)?;
                resource_id = Some(parsed);
            }
            "aggregateVersion" if aggregate_version.is_none() => {
                let parsed = raw_value
                    .trim()
                    .parse::<u64>()
                    .map_err(|_| HostOpsError::new("invalid_mutation_receipt"))?;
                if parsed == 0 {
                    return Err(HostOpsError::new("invalid_mutation_receipt"));
                }
                aggregate_version = Some(parsed);
            }
            _ => return Err(HostOpsError::new("invalid_mutation_receipt")),
        }
    }
    Ok(MutationReceipt {
        result_code: result_code.ok_or_else(|| HostOpsError::new("invalid_mutation_receipt"))?,
        resource_id: resource_id.ok_or_else(|| HostOpsError::new("invalid_mutation_receipt"))?,
        aggregate_version: aggregate_version
            .ok_or_else(|| HostOpsError::new("invalid_mutation_receipt"))?,
    })
}

fn parse_plain_json_string(value: &str) -> HostOpsResult<String> {
    let Some(inner) = value.strip_prefix('"').and_then(|item| item.strip_suffix('"')) else {
        return Err(HostOpsError::new("invalid_mutation_receipt"));
    };
    if inner.contains('\\') || inner.contains('"') || inner.bytes().any(|byte| byte < 0x20) {
        return Err(HostOpsError::new("invalid_mutation_receipt"));
    }
    Ok(inner.to_owned())
}

fn validate_receipt_token(value: &str) -> HostOpsResult<()> {
    if value.is_empty()
        || value.len() > 80
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
    {
        return Err(HostOpsError::new("invalid_mutation_receipt"));
    }
    Ok(())
}

#[must_use]
pub fn render_certificate_receipt(
    operation: &str,
    certificate: &CertificateObservation,
) -> String {
    format!(
        "{{\"schemaVersion\":{},\"operation\":{},\"certificateStore\":{},\"certificateSha1\":{},\"certificateSha256\":{},\"certificateSelector\":{}}}",
        json_string(SCHEMA_VERSION),
        json_string(operation),
        json_string(CERTIFICATE_STORE),
        json_string(certificate.sha1_thumbprint()),
        json_string(certificate.sha256_fingerprint()),
        json_string(&certificate.selector())
    )
}

#[must_use]
pub fn render_binding_receipt(
    operation: &str,
    origin: &str,
    device_id: &str,
    actor_id: &str,
    certificate: &CertificateObservation,
    receipt: &MutationReceipt,
    old_certificate_present_after: Option<bool>,
) -> String {
    let cleanup = old_certificate_present_after.map_or_else(String::new, |present| {
        format!(",\"oldCertificatePresentAfter\":{present}")
    });
    format!(
        "{{\"schemaVersion\":{},\"operation\":{},\"deviceId\":{},\"targetActorId\":{},\"controlPlaneOrigin\":{},\"certificateStore\":{},\"certificateSha1\":{},\"certificateSha256\":{},\"binding\":{{\"resultCode\":{},\"resourceId\":{},\"aggregateVersion\":{}}},\"shippingEnvironment\":{{{}:{},{}:{},{}:{}}}{cleanup}}}",
        json_string(SCHEMA_VERSION),
        json_string(operation),
        json_string(device_id),
        json_string(actor_id),
        json_string(origin),
        json_string(CERTIFICATE_STORE),
        json_string(certificate.sha1_thumbprint()),
        json_string(certificate.sha256_fingerprint()),
        json_string(receipt.result_code()),
        json_string(receipt.resource_id()),
        receipt.aggregate_version(),
        json_string(SHIPPING_DEVICE_ID_ENV),
        json_string(device_id),
        json_string(SHIPPING_CERT_SHA1_ENV),
        json_string(certificate.sha1_thumbprint()),
        json_string(SHIPPING_ORIGIN_ENV),
        json_string(origin),
    )
}

#[must_use]
pub fn render_revoke_receipt(
    origin: &str,
    actor_id: &str,
    sha1_thumbprint: &str,
    receipt: &MutationReceipt,
) -> String {
    format!(
        "{{\"schemaVersion\":{},\"operation\":\"revoke\",\"targetActorId\":{},\"controlPlaneOrigin\":{},\"certificateStore\":{},\"certificateSha1\":{},\"binding\":{{\"resultCode\":{},\"resourceId\":{},\"aggregateVersion\":{}}},\"localCertificatePresentAfter\":false}}",
        json_string(SCHEMA_VERSION),
        json_string(actor_id),
        json_string(origin),
        json_string(CERTIFICATE_STORE),
        json_string(sha1_thumbprint),
        json_string(receipt.result_code()),
        json_string(receipt.resource_id()),
        receipt.aggregate_version(),
    )
}

#[must_use]
pub fn json_string(value: &str) -> String {
    let mut output = String::with_capacity(value.len() + 2);
    output.push('"');
    for character in value.chars() {
        match character {
            '"' => output.push_str("\\\""),
            '\\' => output.push_str("\\\\"),
            '\n' => output.push_str("\\n"),
            '\r' => output.push_str("\\r"),
            '\t' => output.push_str("\\t"),
            value if value < ' ' => {
                use std::fmt::Write as _;
                let _ = write!(output, "\\u{:04x}", u32::from(value));
            }
            value => output.push(value),
        }
    }
    output.push('"');
    output
}

#[cfg(test)]
mod tests {
    use super::{
        binding_revoke_plan, binding_write_plan, build_access_config, normalize_https_origin,
        parse_certificate_observation, parse_control_plane_output, render_binding_receipt,
        render_certificate_receipt, validate_identifier,
    };

    const TOKEN: &str = "eyJhbGciOiJSUzI1NiJ9.eyJhdWQiOiJ0ZXN0In0.c2lnbmF0dXJlX2J5dGVz";

    #[test]
    fn same_certificate_observation_requires_private_key_client_auth_and_validity()
    -> Result<(), Box<dyn std::error::Error>> {
        let observation = parse_certificate_observation(&format!(
            "{}\t{}\t1\t1\t1",
            "AB".repeat(20),
            "cd".repeat(32)
        ))?;
        assert_eq!(observation.sha1_thumbprint(), "AB".repeat(20));
        assert_eq!(observation.sha256_fingerprint(), "cd".repeat(32));
        assert_eq!(
            observation.selector(),
            format!("LocalMachine\\MY\\{}", "AB".repeat(20))
        );
        let rendered = render_certificate_receipt("inspect", &observation);
        assert!(rendered.contains("LocalMachine\\\\MY\\\\"));
        for (private, client, current) in [("0", "1", "1"), ("1", "0", "1"), ("1", "1", "0")] {
            assert!(parse_certificate_observation(&format!(
                "{}\t{}\t{private}\t{client}\t{current}",
                "AB".repeat(20),
                "cd".repeat(32)
            ))
            .is_err());
        }
        Ok(())
    }

    #[test]
    fn api_opaque_ids_match_the_accepted_8_to_96_character_contract() {
        assert!(validate_identifier("actor_01").is_ok());
        assert!(validate_identifier("short").is_err());
        assert!(validate_identifier(&"a".repeat(97)).is_err());
        assert!(validate_identifier("actor.01").is_err());
    }

    #[test]
    fn user_access_token_is_stdin_only_and_never_enters_curl_arguments()
    -> Result<(), Box<dyn std::error::Error>> {
        let plan = binding_write_plan(
            "https://control.example.test",
            "tenant_01",
            "actor_01",
            "device_01",
            &"ab".repeat(32),
            None,
            "corr_tx2_01",
        )?;
        let arguments = plan.curl_arguments();
        assert!(!arguments.iter().any(|argument| argument.contains(TOKEN)));
        let config = build_access_config(TOKEN)?;
        let config_text = std::str::from_utf8(&config)?;
        assert!(config_text.contains("cf-access-token: "));
        assert!(config_text.contains(TOKEN));
        Ok(())
    }

    #[test]
    fn public_binding_contract_is_exact_put_delete_and_lowercase_sha256()
    -> Result<(), Box<dyn std::error::Error>> {
        let put = binding_write_plan(
            "https://control.example.test/",
            "tenant_01",
            "actor_01",
            "device_01",
            &"ab".repeat(32),
            Some(4),
            "corr_tx2_02",
        )?;
        let put_arguments = put.curl_arguments().join("\n");
        assert!(put_arguments.contains("PUT"));
        assert!(put_arguments.contains("expectedPreviousVersion"));
        assert!(put_arguments.contains(
            "https://control.example.test/api/v1/tenants/tenant_01/members/actor_01/device-binding"
        ));
        assert!(binding_write_plan(
            "https://control.example.test",
            "tenant_01",
            "actor_01",
            "device_01",
            &"AB".repeat(32),
            None,
            "corr_tx2_03"
        )
        .is_err());
        let delete = binding_revoke_plan(
            "https://control.example.test",
            "tenant_01",
            "actor_01",
            5,
            "corr_tx2_04",
        )?;
        assert!(delete.curl_arguments().join("\n").contains("DELETE"));
        Ok(())
    }

    #[test]
    fn control_plane_receipt_is_strict_and_secret_free()
    -> Result<(), Box<dyn std::error::Error>> {
        let receipt = parse_control_plane_output(
            "{\"resultCode\":\"bound\",\"resourceId\":\"actor_01\",\"aggregateVersion\":7}\n200",
        )?;
        assert_eq!(receipt.result_code(), "bound");
        assert_eq!(receipt.resource_id(), "actor_01");
        assert_eq!(receipt.aggregate_version(), 7);
        assert!(parse_control_plane_output("{}\n409").is_err());
        assert!(parse_control_plane_output(
            "{\"resultCode\":\"bound\",\"resourceId\":\"actor_01\",\"aggregateVersion\":7,\"privateKey\":\"x\"}\n200"
        )
        .is_err());
        Ok(())
    }

    #[test]
    fn rendered_observation_contains_only_non_secret_shipping_material()
    -> Result<(), Box<dyn std::error::Error>> {
        let certificate = parse_certificate_observation(&format!(
            "{}\t{}\t1\t1\t1",
            "AB".repeat(20),
            "cd".repeat(32)
        ))?;
        let receipt = parse_control_plane_output(
            "{\"resultCode\":\"bound\",\"resourceId\":\"actor_01\",\"aggregateVersion\":1}\n200",
        )?;
        let rendered = render_binding_receipt(
            "bind",
            "https://control.example.test",
            "device_01",
            "actor_01",
            &certificate,
            &receipt,
            None,
        );
        assert!(rendered.contains("PROFILE_BRIDGE_DEVICE_ID"));
        assert!(rendered.contains("PROFILE_BRIDGE_MACHINE_CERT_SHA1"));
        assert!(!rendered.to_ascii_lowercase().contains("privatekey"));
        assert!(!rendered.to_ascii_lowercase().contains("access-token"));
        Ok(())
    }

    #[test]
    fn origin_validation_is_https_only_and_pathless() {
        assert_eq!(
            normalize_https_origin("https://control.example.test/").as_deref(),
            Ok("https://control.example.test")
        );
        assert!(normalize_https_origin("http://control.example.test").is_err());
        assert!(normalize_https_origin("https://control.example.test/api").is_err());
        assert!(normalize_https_origin("https://user@control.example.test").is_err());
    }
}
