use opsctl::release::component_manifest::verify_component_manifests;
use opsctl::release::digest::{canonical_json, sha256_hex};
use opsctl::release::model::{RELEASE_SET_ID_PREFIX, ReleaseSetManifest};
use serde_json::{Value, json};
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};

const REPOSITORY: &str = "iamaman11/part-crm-emai-profile";
const GIT_SHA: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const CONTROL_MANIFEST: &[u8] = b"{\"release_id\":\"cp\"}\n";
const RESOLVER_MANIFEST: &[u8] = b"{\"release_id\":\"rs\"}\n";
const RUNTIME_MANIFEST: &[u8] = b"{\"release_id\":\"rt\"}\n";
const TAR_BLOCK: usize = 512;
const OVERSIZED_MANIFEST_BYTES: u64 = 4 * 1024 * 1024 + 1;

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

fn temp_dir(label: &str) -> std::io::Result<PathBuf> {
    let root = std::env::temp_dir().join(format!(
        "opsctl-component-manifest-integration-{label}-{}",
        std::process::id()
    ));
    if root.exists() {
        fs::remove_dir_all(&root)?;
    }
    fs::create_dir_all(root.join("components"))?;
    Ok(root)
}

fn tar_header(name: &str, size: u64, typeflag: u8) -> TestResult<[u8; TAR_BLOCK]> {
    let mut header = [0_u8; TAR_BLOCK];
    let name_bytes = name.as_bytes();
    if name_bytes.is_empty() || name_bytes.len() > 100 {
        return Err(format!("test tar name must fit ustar name field: {name}").into());
    }
    header[..name_bytes.len()].copy_from_slice(name_bytes);
    header[100..108].copy_from_slice(b"0000644\0");
    header[108..116].copy_from_slice(b"0000000\0");
    header[116..124].copy_from_slice(b"0000000\0");
    let size_text = format!("{size:011o}\0");
    if size_text.len() != 12 {
        return Err(format!("test tar size does not fit octal field: {size}").into());
    }
    header[124..136].copy_from_slice(size_text.as_bytes());
    header[136..148].copy_from_slice(b"00000000000\0");
    header[148..156].fill(b' ');
    header[156] = typeflag;
    header[257..263].copy_from_slice(b"ustar\0");
    header[263..265].copy_from_slice(b"00");
    let checksum: u64 = header.iter().map(|byte| u64::from(*byte)).sum();
    let checksum_text = format!("{checksum:06o}\0 ");
    if checksum_text.len() != 8 {
        return Err("test tar checksum does not fit checksum field".into());
    }
    header[148..156].copy_from_slice(checksum_text.as_bytes());
    Ok(header)
}

fn write_payload(file: &mut File, payload: &[u8]) -> std::io::Result<()> {
    file.write_all(payload)?;
    let padding = (TAR_BLOCK - (payload.len() % TAR_BLOCK)) % TAR_BLOCK;
    if padding != 0 {
        file.write_all(&vec![0_u8; padding])?;
    }
    Ok(())
}

fn write_entry(file: &mut File, name: &str, typeflag: u8, payload: &[u8]) -> TestResult {
    file.write_all(&tar_header(name, payload.len() as u64, typeflag)?)?;
    write_payload(file, payload)?;
    Ok(())
}

fn finish_tar(file: &mut File) -> std::io::Result<()> {
    file.write_all(&[0_u8; TAR_BLOCK * 2])
}

fn write_simple_tar(path: &Path, name: &str, payload: &[u8]) -> TestResult {
    let mut file = File::create(path)?;
    write_entry(&mut file, name, b'0', payload)?;
    finish_tar(&mut file)?;
    Ok(())
}

fn pax_record(key: &str, value: &str) -> String {
    let body = format!("{key}={value}\n");
    let mut length = body.len() + 2;
    loop {
        let record = format!("{length} {body}");
        if record.len() == length {
            return record;
        }
        length = record.len();
    }
}

fn write_pax_tar(path: &Path, logical_path: &str, payload: &[u8]) -> TestResult {
    let mut file = File::create(path)?;
    let pax = pax_record("path", logical_path);
    write_entry(&mut file, "PaxHeader", b'x', pax.as_bytes())?;
    write_entry(&mut file, "placeholder", b'0', payload)?;
    finish_tar(&mut file)?;
    Ok(())
}

fn write_gnu_long_name_tar(path: &Path, logical_path: &str, payload: &[u8]) -> TestResult {
    let mut file = File::create(path)?;
    let mut long_name = logical_path.as_bytes().to_vec();
    long_name.push(0);
    write_entry(&mut file, "././@LongLink", b'L', &long_name)?;
    write_entry(&mut file, "placeholder", b'0', payload)?;
    finish_tar(&mut file)?;
    Ok(())
}

fn write_duplicate_manifest_tar(path: &Path, payload: &[u8]) -> TestResult {
    let mut file = File::create(path)?;
    write_entry(&mut file, "first/release-manifest.json", b'0', payload)?;
    write_entry(&mut file, "second/release-manifest.json", b'0', payload)?;
    finish_tar(&mut file)?;
    Ok(())
}

fn write_oversized_manifest_header(path: &Path) -> TestResult {
    let mut file = File::create(path)?;
    file.write_all(&tar_header(
        "release-manifest.json",
        OVERSIZED_MANIFEST_BYTES,
        b'0',
    )?)?;
    finish_tar(&mut file)?;
    Ok(())
}

fn write_malformed_pax_tar(path: &Path) -> TestResult {
    let mut file = File::create(path)?;
    write_entry(&mut file, "PaxHeader", b'x', b"12 path=bad")?;
    write_entry(&mut file, "release-manifest.json", b'0', CONTROL_MANIFEST)?;
    finish_tar(&mut file)?;
    Ok(())
}

fn accepted_main_evidence() -> Result<String, String> {
    let identity = json!({
        "authority": "accepted-main",
        "commit_sha": GIT_SHA,
        "repository": REPOSITORY,
    });
    Ok(sha256_hex(canonical_json(&identity)?.as_bytes()))
}

fn artifact(root: &Path, path: &str) -> TestResult<Value> {
    let bytes = fs::read(root.join(path))?;
    Ok(json!({
        "path": path,
        "sha256": sha256_hex(&bytes),
        "size_bytes": bytes.len(),
        "kind": "component",
    }))
}

fn component(root: &Path, release_id: &str, path: &str, manifest: &[u8]) -> TestResult<Value> {
    let bytes = fs::read(root.join(path))?;
    Ok(json!({
        "release_id": release_id,
        "source_commit_sha": GIT_SHA,
        "artifact_path": path,
        "artifact_sha256": sha256_hex(&bytes),
        "artifact_size_bytes": bytes.len(),
        "component_manifest_sha256": sha256_hex(manifest),
    }))
}

fn fixture(root: &Path) -> TestResult<ReleaseSetManifest> {
    write_simple_tar(
        &root.join("components/secret-resolver.tar"),
        "release-manifest.json",
        RESOLVER_MANIFEST,
    )?;
    write_simple_tar(
        &root.join("components/runtime-bundle.tar"),
        "runtime-manifest.json",
        RUNTIME_MANIFEST,
    )?;

    let evidence = accepted_main_evidence()?;
    let mut value = json!({
        "schema_version": 1,
        "release_set_id": format!("{RELEASE_SET_ID_PREFIX}{}", "0".repeat(64)),
        "source": {
            "repository": REPOSITORY,
            "commit_sha": GIT_SHA,
            "accepted_main": true,
            "accepted_main_evidence_sha256": evidence,
        },
        "components": {
            "control_plane": component(
                root,
                "cp",
                "components/control-plane.tar",
                CONTROL_MANIFEST,
            )?,
            "secret_resolver": component(
                root,
                "rs",
                "components/secret-resolver.tar",
                RESOLVER_MANIFEST,
            )?,
            "runtime_bundle": component(
                root,
                "rt",
                "components/runtime-bundle.tar",
                RUNTIME_MANIFEST,
            )?,
        },
        "contracts": {},
        "protocols": {},
        "schemas": {},
        "runtime_compatibility": {},
        "capability_profile_compatibility": ["rehearsal-core-v1"],
        "build_provenance": {},
        "artifact_inventory": [
            artifact(root, "components/control-plane.tar")?,
            artifact(root, "components/secret-resolver.tar")?,
            artifact(root, "components/runtime-bundle.tar")?,
        ],
    });
    let mut identity = value.clone();
    identity
        .as_object_mut()
        .ok_or("release identity must be an object")?
        .remove("release_set_id");
    let digest = sha256_hex(canonical_json(&identity)?.as_bytes());
    value["release_set_id"] = Value::String(format!("{RELEASE_SET_ID_PREFIX}{digest}"));
    Ok(ReleaseSetManifest::parse_json(&serde_json::to_string(
        &value,
    )?)?)
}

fn assert_manifest_mismatch<T, E>(result: Result<T, E>) -> TestResult
where
    E: std::fmt::Display,
{
    let error = match result {
        Ok(_) => return Err("malformed component manifest storage unexpectedly passed".into()),
        Err(error) => error.to_string(),
    };
    if !error.contains("COMPONENT_MANIFEST_MISMATCH") {
        return Err(format!("unexpected error: {error}").into());
    }
    Ok(())
}

#[test]
fn accepts_pax_path_for_embedded_manifest() -> TestResult {
    let root = temp_dir("pax")?;
    write_pax_tar(
        &root.join("components/control-plane.tar"),
        "deep/generated/release-manifest.json",
        CONTROL_MANIFEST,
    )?;
    let manifest = fixture(&root)?;
    let result = verify_component_manifests(&manifest, &root)?;
    assert_eq!(result.verified_components, 3);
    assert_eq!(result.durable_manifest_bindings, 3);
    assert_eq!(result.legacy_reconstructed_manifests, 0);
    fs::remove_dir_all(root)?;
    Ok(())
}

#[test]
fn accepts_gnu_long_path_for_embedded_manifest() -> TestResult {
    let root = temp_dir("gnu-long")?;
    write_gnu_long_name_tar(
        &root.join("components/control-plane.tar"),
        "very/long/generated/path/release-manifest.json",
        CONTROL_MANIFEST,
    )?;
    let manifest = fixture(&root)?;
    let result = verify_component_manifests(&manifest, &root)?;
    assert_eq!(result.verified_components, 3);
    assert_eq!(result.durable_manifest_bindings, 3);
    fs::remove_dir_all(root)?;
    Ok(())
}

#[test]
fn rejects_duplicate_embedded_manifest_basename() -> TestResult {
    let root = temp_dir("duplicate")?;
    write_duplicate_manifest_tar(&root.join("components/control-plane.tar"), CONTROL_MANIFEST)?;
    let manifest = fixture(&root)?;
    assert_manifest_mismatch(verify_component_manifests(&manifest, &root))?;
    fs::remove_dir_all(root)?;
    Ok(())
}

#[test]
fn rejects_oversized_manifest_before_allocation() -> TestResult {
    let root = temp_dir("oversized")?;
    write_oversized_manifest_header(&root.join("components/control-plane.tar"))?;
    let manifest = fixture(&root)?;
    assert_manifest_mismatch(verify_component_manifests(&manifest, &root))?;
    fs::remove_dir_all(root)?;
    Ok(())
}

#[test]
fn rejects_malformed_pax_record() -> TestResult {
    let root = temp_dir("malformed-pax")?;
    write_malformed_pax_tar(&root.join("components/control-plane.tar"))?;
    let manifest = fixture(&root)?;
    assert_manifest_mismatch(verify_component_manifests(&manifest, &root))?;
    fs::remove_dir_all(root)?;
    Ok(())
}
