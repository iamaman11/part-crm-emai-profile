#![forbid(unsafe_code)]

use bridge_host_ops::{
    CERTIFICATE_STORE, CertificateObservation, HostOpsError, HostOpsResult, MutationReceipt,
    SCHEMA_VERSION, binding_revoke_plan, binding_write_plan, json_string, normalize_https_origin,
    normalize_sha1_thumbprint, render_binding_receipt, render_certificate_receipt,
    render_revoke_receipt, validate_identifier,
};
use std::collections::BTreeMap;
use std::env;
use std::path::Path;

#[cfg(windows)]
mod windows;

struct ReplayTrace<'a> {
    correlation_id: &'a str,
    idempotency_key: &'a str,
}

fn main() {
    match run() {
        Ok(output) => println!("{output}"),
        Err(error) => {
            eprintln!("bridge-host-ops: {}", error.code());
            std::process::exit(2);
        }
    }
}

fn run() -> HostOpsResult<String> {
    let mut arguments = env::args();
    let _program = arguments.next();
    let command = arguments
        .next()
        .ok_or_else(|| HostOpsError::new("command_required"))?;
    let flags = parse_flags(arguments)?;
    match command.as_str() {
        "self-test" => self_test(&flags),
        "inspect" => inspect(&flags),
        "import" => import(&flags),
        "bind" => bind(&flags, false),
        "rebind" => bind(&flags, true),
        "revoke" => revoke(&flags),
        _ => Err(HostOpsError::new("unknown_command")),
    }
}

fn parse_flags(arguments: impl Iterator<Item = String>) -> HostOpsResult<BTreeMap<String, String>> {
    let values: Vec<String> = arguments.collect();
    if !values.len().is_multiple_of(2) {
        return Err(HostOpsError::new("flag_value_required"));
    }
    let mut flags = BTreeMap::new();
    for pair in values.chunks_exact(2) {
        let flag = &pair[0];
        let value = &pair[1];
        if !flag.starts_with("--") || flag.len() <= 2 || flags.contains_key(flag) {
            return Err(HostOpsError::new("invalid_flag"));
        }
        flags.insert(flag.clone(), value.clone());
    }
    Ok(flags)
}

fn self_test(flags: &BTreeMap<String, String>) -> HostOpsResult<String> {
    ensure_only(flags, &[])?;
    let token = "eyJhbGciOiJSUzI1NiJ9.eyJhdWQiOiJ0eDItaG9zdC1vcHMifQ.c2lnbmF0dXJl";
    let plan = binding_write_plan(
        "https://control.example.test",
        "tenant_01",
        "actor_01",
        "device_01",
        &"ab".repeat(32),
        None,
        "corr_tx2_self_test",
    )?;
    if plan
        .curl_arguments()
        .iter()
        .any(|value| value.contains(token))
    {
        return Err(HostOpsError::new("self_test_secret_argv_failure"));
    }
    let header = idempotency_header("idem_tx2_self_test")?;
    if header != "Idempotency-Key: idem_tx2_self_test" {
        return Err(HostOpsError::new("self_test_idempotency_header_failure"));
    }
    let mut config = bridge_host_ops::build_access_config(token)?;
    if !std::str::from_utf8(&config)
        .map_err(|_| HostOpsError::new("self_test_failed"))?
        .contains(token)
    {
        config.fill(0);
        return Err(HostOpsError::new("self_test_failed"));
    }
    config.fill(0);
    Ok("{\"schemaVersion\":\"bridge-host-ops/v1\",\"status\":\"ok\"}".to_owned())
}

fn inspect(flags: &BTreeMap<String, String>) -> HostOpsResult<String> {
    ensure_only(flags, &["--thumbprint"])?;
    let thumbprint = required(flags, "--thumbprint")?;
    let _ = normalize_sha1_thumbprint(thumbprint)?;
    let certificate = windows_inspect(thumbprint)?;
    Ok(render_certificate_receipt("inspect", &certificate))
}

fn import(flags: &BTreeMap<String, String>) -> HostOpsResult<String> {
    ensure_only(flags, &["--pfx", "--password-file"])?;
    let certificate = windows_import(
        Path::new(required(flags, "--pfx")?),
        Path::new(required(flags, "--password-file")?),
    )?;
    Ok(render_certificate_receipt("import", &certificate))
}

fn bind(flags: &BTreeMap<String, String>, rebind: bool) -> HostOpsResult<String> {
    let allowed = if rebind {
        &[
            "--origin",
            "--tenant-id",
            "--actor-id",
            "--device-id",
            "--thumbprint",
            "--old-thumbprint",
            "--expected-previous-version",
            "--access-token-file",
            "--correlation-id",
            "--idempotency-key",
        ][..]
    } else {
        &[
            "--origin",
            "--tenant-id",
            "--actor-id",
            "--device-id",
            "--thumbprint",
            "--expected-previous-version",
            "--access-token-file",
            "--correlation-id",
            "--idempotency-key",
        ][..]
    };
    ensure_only(flags, allowed)?;
    let origin = normalize_https_origin(required(flags, "--origin")?)?;
    let tenant_id = required(flags, "--tenant-id")?;
    let actor_id = required(flags, "--actor-id")?;
    let device_id = required(flags, "--device-id")?;
    validate_api_opaque_id(tenant_id)?;
    validate_api_opaque_id(actor_id)?;
    validate_api_opaque_id(device_id)?;
    let thumbprint = normalize_sha1_thumbprint(required(flags, "--thumbprint")?)?;
    let expected_previous_version = optional_version(flags, "--expected-previous-version")?;
    if rebind && expected_previous_version.is_none() {
        return Err(HostOpsError::new("expected_previous_version_required"));
    }
    let trace = ReplayTrace {
        correlation_id: required(flags, "--correlation-id")?,
        idempotency_key: required(flags, "--idempotency-key")?,
    };
    validate_api_opaque_id(trace.correlation_id)?;
    validate_api_opaque_id(trace.idempotency_key)?;

    let old_thumbprint = if rebind {
        let old = normalize_sha1_thumbprint(required(flags, "--old-thumbprint")?)?;
        if old == thumbprint {
            return Err(HostOpsError::new("rebind_requires_distinct_certificate"));
        }
        Some(old)
    } else {
        None
    };

    let certificate = windows_inspect(&thumbprint)?;
    let plan = binding_write_plan(
        &origin,
        tenant_id,
        actor_id,
        device_id,
        certificate.sha256_fingerprint(),
        expected_previous_version,
        trace.correlation_id,
    )?;
    let receipt = windows_execute_binding(
        &plan,
        Path::new(required(flags, "--access-token-file")?),
        trace.idempotency_key,
    )?;

    let cleanup = match old_thumbprint {
        Some(old) => {
            if windows_remove(&old).is_err() {
                println!(
                    "{}",
                    render_rebind_cleanup_required(
                        &origin,
                        device_id,
                        actor_id,
                        &certificate,
                        &old,
                        &receipt,
                        &trace,
                    )
                );
                return Err(HostOpsError::new(
                    "local_cleanup_required_after_server_commit",
                ));
            }
            Some(false)
        }
        None => None,
    };

    Ok(render_binding_receipt(
        if rebind { "rebind" } else { "bind" },
        &origin,
        device_id,
        actor_id,
        &certificate,
        &receipt,
        cleanup,
    ))
}

fn revoke(flags: &BTreeMap<String, String>) -> HostOpsResult<String> {
    ensure_only(
        flags,
        &[
            "--origin",
            "--tenant-id",
            "--actor-id",
            "--thumbprint",
            "--expected-version",
            "--access-token-file",
            "--correlation-id",
            "--idempotency-key",
        ],
    )?;
    let origin = normalize_https_origin(required(flags, "--origin")?)?;
    let tenant_id = required(flags, "--tenant-id")?;
    let actor_id = required(flags, "--actor-id")?;
    validate_api_opaque_id(tenant_id)?;
    validate_api_opaque_id(actor_id)?;
    let thumbprint = normalize_sha1_thumbprint(required(flags, "--thumbprint")?)?;
    let expected_version = required_version(flags, "--expected-version")?;
    let trace = ReplayTrace {
        correlation_id: required(flags, "--correlation-id")?,
        idempotency_key: required(flags, "--idempotency-key")?,
    };
    validate_api_opaque_id(trace.correlation_id)?;
    validate_api_opaque_id(trace.idempotency_key)?;
    let plan = binding_revoke_plan(
        &origin,
        tenant_id,
        actor_id,
        expected_version,
        trace.correlation_id,
    )?;
    let receipt = windows_execute_binding(
        &plan,
        Path::new(required(flags, "--access-token-file")?),
        trace.idempotency_key,
    )?;
    if windows_remove(&thumbprint).is_err() {
        println!(
            "{}",
            render_revoke_cleanup_required(&origin, actor_id, &thumbprint, &receipt, &trace)
        );
        return Err(HostOpsError::new(
            "local_cleanup_required_after_server_commit",
        ));
    }
    Ok(render_revoke_receipt(
        &origin,
        actor_id,
        &thumbprint,
        &receipt,
    ))
}

fn ensure_only(flags: &BTreeMap<String, String>, allowed: &[&str]) -> HostOpsResult<()> {
    if flags.keys().any(|key| !allowed.contains(&key.as_str())) {
        return Err(HostOpsError::new("unknown_flag"));
    }
    Ok(())
}

fn required<'a>(flags: &'a BTreeMap<String, String>, name: &str) -> HostOpsResult<&'a str> {
    flags
        .get(name)
        .map(String::as_str)
        .ok_or_else(|| HostOpsError::new("required_flag_missing"))
}

fn required_version(flags: &BTreeMap<String, String>, name: &str) -> HostOpsResult<u64> {
    let value = required(flags, name)?
        .parse::<u64>()
        .map_err(|_| HostOpsError::new("invalid_expected_version"))?;
    if value == 0 {
        return Err(HostOpsError::new("invalid_expected_version"));
    }
    Ok(value)
}

fn optional_version(flags: &BTreeMap<String, String>, name: &str) -> HostOpsResult<Option<u64>> {
    match flags.get(name) {
        Some(_) => required_version(flags, name).map(Some),
        None => Ok(None),
    }
}

pub(crate) fn validate_api_opaque_id(value: &str) -> HostOpsResult<()> {
    validate_identifier(value).map_err(|_| HostOpsError::new("invalid_api_opaque_id"))
}

pub(crate) fn idempotency_header(value: &str) -> HostOpsResult<String> {
    validate_api_opaque_id(value)?;
    Ok(format!("Idempotency-Key: {value}"))
}

fn render_rebind_cleanup_required(
    origin: &str,
    device_id: &str,
    actor_id: &str,
    certificate: &CertificateObservation,
    old_thumbprint: &str,
    receipt: &MutationReceipt,
    trace: &ReplayTrace<'_>,
) -> String {
    format!(
        "{{\"schemaVersion\":{},\"operation\":\"rebind\",\"completionStatus\":\"cleanup_required\",\"deviceId\":{},\"targetActorId\":{},\"controlPlaneOrigin\":{},\"certificateStore\":{},\"certificateSha1\":{},\"certificateSha256\":{},\"oldCertificateSha1\":{},\"oldCertificatePresentAfter\":true,\"binding\":{{\"resultCode\":{},\"resourceId\":{},\"aggregateVersion\":{}}},\"correlationId\":{},\"idempotencyKey\":{}}}",
        json_string(SCHEMA_VERSION),
        json_string(device_id),
        json_string(actor_id),
        json_string(origin),
        json_string(CERTIFICATE_STORE),
        json_string(certificate.sha1_thumbprint()),
        json_string(certificate.sha256_fingerprint()),
        json_string(old_thumbprint),
        json_string(receipt.result_code()),
        json_string(receipt.resource_id()),
        receipt.aggregate_version(),
        json_string(trace.correlation_id),
        json_string(trace.idempotency_key),
    )
}

fn render_revoke_cleanup_required(
    origin: &str,
    actor_id: &str,
    thumbprint: &str,
    receipt: &MutationReceipt,
    trace: &ReplayTrace<'_>,
) -> String {
    format!(
        "{{\"schemaVersion\":{},\"operation\":\"revoke\",\"completionStatus\":\"cleanup_required\",\"targetActorId\":{},\"controlPlaneOrigin\":{},\"certificateStore\":{},\"certificateSha1\":{},\"localCertificatePresentAfter\":true,\"binding\":{{\"resultCode\":{},\"resourceId\":{},\"aggregateVersion\":{}}},\"correlationId\":{},\"idempotencyKey\":{}}}",
        json_string(SCHEMA_VERSION),
        json_string(actor_id),
        json_string(origin),
        json_string(CERTIFICATE_STORE),
        json_string(thumbprint),
        json_string(receipt.result_code()),
        json_string(receipt.resource_id()),
        receipt.aggregate_version(),
        json_string(trace.correlation_id),
        json_string(trace.idempotency_key),
    )
}

#[cfg(windows)]
fn windows_inspect(thumbprint: &str) -> HostOpsResult<CertificateObservation> {
    windows::inspect_certificate(thumbprint)
}

#[cfg(not(windows))]
fn windows_inspect(_: &str) -> HostOpsResult<CertificateObservation> {
    Err(HostOpsError::new("windows_required"))
}

#[cfg(windows)]
fn windows_import(pfx: &Path, password_file: &Path) -> HostOpsResult<CertificateObservation> {
    windows::import_certificate(pfx, password_file)
}

#[cfg(not(windows))]
fn windows_import(_: &Path, _: &Path) -> HostOpsResult<CertificateObservation> {
    Err(HostOpsError::new("windows_required"))
}

#[cfg(windows)]
fn windows_execute_binding(
    plan: &bridge_host_ops::BindingHttpPlan,
    token_file: &Path,
    idempotency_key: &str,
) -> HostOpsResult<MutationReceipt> {
    windows::execute_binding(plan, token_file, idempotency_key)
}

#[cfg(not(windows))]
fn windows_execute_binding(
    _: &bridge_host_ops::BindingHttpPlan,
    _: &Path,
    _: &str,
) -> HostOpsResult<MutationReceipt> {
    Err(HostOpsError::new("windows_required"))
}

#[cfg(windows)]
fn windows_remove(thumbprint: &str) -> HostOpsResult<()> {
    windows::remove_certificate(thumbprint)
}

#[cfg(not(windows))]
fn windows_remove(_: &str) -> HostOpsResult<()> {
    Err(HostOpsError::new("windows_required"))
}
