use crate::release::digest::{canonical_json, sha256_hex};
use crate::release::model::{ReleaseComponentIdentity, ReleaseModelError, ReleaseSetManifest};
use std::fs::{self, File};
use std::io::{ErrorKind, Read, Seek, SeekFrom};
use std::path::Path;

const TAR_BLOCK: usize = 512;
const MAX_MANIFEST_BYTES: u64 = 4 * 1024 * 1024;
const MAX_PAX_BYTES: u64 = 1024 * 1024;
const PROFILE_BRIDGE_MANIFEST_PATH: &str = "components/profile-bridge-manifest.json";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComponentManifestVerification {
    pub verified_components: usize,
    pub durable_manifest_bindings: usize,
    pub legacy_reconstructed_manifests: usize,
}

pub fn verify_component_manifests(
    manifest: &ReleaseSetManifest,
    artifact_root: &Path,
) -> Result<ComponentManifestVerification, ReleaseModelError> {
    let mut verified_components = 0_usize;
    let mut durable_manifest_bindings = 0_usize;
    let mut legacy_reconstructed_manifests = 0_usize;

    for component in manifest.components.values() {
        match component.component_id.as_str() {
            "control_plane" | "frontend" => {
                let bytes = read_unique_tar_member_by_basename(
                    &artifact_root.join(&component.artifact_path),
                    "release-manifest.json",
                )?;
                verify_manifest_digest(component, &bytes)?;
                durable_manifest_bindings += 1;
            }
            "secret_resolver" => {
                let bytes = read_unique_tar_member_by_basename(
                    &artifact_root.join(&component.artifact_path),
                    "release-manifest.json",
                )?;
                verify_manifest_digest(component, &bytes)?;
                durable_manifest_bindings += 1;
            }
            "runtime_bundle" => {
                let bytes = read_unique_tar_member_by_basename(
                    &artifact_root.join(&component.artifact_path),
                    "runtime-manifest.json",
                )?;
                verify_manifest_digest(component, &bytes)?;
                durable_manifest_bindings += 1;
            }
            "profile_bridge" => {
                if let Some(sidecar) = manifest
                    .artifact_inventory
                    .iter()
                    .find(|artifact| artifact.path == PROFILE_BRIDGE_MANIFEST_PATH)
                {
                    if sidecar.kind != "manifest"
                        || sidecar.sha256 != component.component_manifest_sha256
                    {
                        return Err(mismatch(
                            "profile_bridge sidecar inventory does not bind component_manifest_sha256",
                        ));
                    }
                    let path = artifact_root.join(PROFILE_BRIDGE_MANIFEST_PATH);
                    let bytes = read_regular_bounded(&path, MAX_MANIFEST_BYTES)?;
                    verify_manifest_digest(component, &bytes)?;
                    durable_manifest_bindings += 1;
                } else {
                    let bytes = legacy_profile_bridge_manifest(component)?;
                    verify_manifest_digest(component, &bytes)?;
                    legacy_reconstructed_manifests += 1;
                }
            }
            other => {
                return Err(mismatch(format!(
                    "unsupported component manifest location for {other}"
                )));
            }
        }
        verified_components += 1;
    }

    Ok(ComponentManifestVerification {
        verified_components,
        durable_manifest_bindings,
        legacy_reconstructed_manifests,
    })
}

fn verify_manifest_digest(
    component: &ReleaseComponentIdentity,
    bytes: &[u8],
) -> Result<(), ReleaseModelError> {
    let observed = sha256_hex(bytes);
    if observed != component.component_manifest_sha256 {
        return Err(mismatch(format!(
            "{} expected={} observed={observed}",
            component.component_id, component.component_manifest_sha256
        )));
    }
    Ok(())
}

fn legacy_profile_bridge_manifest(
    component: &ReleaseComponentIdentity,
) -> Result<Vec<u8>, ReleaseModelError> {
    let payload = serde_json::json!({
        "schema_version": 1,
        "kind": "PROFILE_BRIDGE_COMPONENT",
        "source_commit_sha": component.source_commit_sha,
        "artifact_sha256": component.artifact_sha256,
        "artifact_size_bytes": component.artifact_size_bytes,
    });
    let canonical = canonical_json(&payload).map_err(|error| {
        mismatch(format!(
            "cannot canonicalize historical profile_bridge manifest: {error}"
        ))
    })?;
    let release_id = format!(
        "profile-bridge-v1-sha256-{}",
        sha256_hex(canonical.as_bytes())
    );
    if release_id != component.release_id {
        return Err(mismatch(format!(
            "historical profile_bridge release_id expected={release_id} observed={}",
            component.release_id
        )));
    }

    let artifact_sha = serde_json::to_string(&component.artifact_sha256)
        .map_err(|error| mismatch(format!("cannot serialize artifact digest: {error}")))?;
    let kind = serde_json::to_string("PROFILE_BRIDGE_COMPONENT")
        .map_err(|error| mismatch(format!("cannot serialize profile bridge kind: {error}")))?;
    let release_id_json = serde_json::to_string(&release_id)
        .map_err(|error| mismatch(format!("cannot serialize release id: {error}")))?;
    let source_sha = serde_json::to_string(&component.source_commit_sha)
        .map_err(|error| mismatch(format!("cannot serialize source sha: {error}")))?;
    Ok(format!(
        "{{\n  \"artifact_sha256\": {artifact_sha},\n  \"artifact_size_bytes\": {},\n  \"kind\": {kind},\n  \"release_id\": {release_id_json},\n  \"schema_version\": 1,\n  \"source_commit_sha\": {source_sha}\n}}\n",
        component.artifact_size_bytes
    )
    .into_bytes())
}

fn read_unique_tar_member_by_basename(
    archive_path: &Path,
    target_basename: &str,
) -> Result<Vec<u8>, ReleaseModelError> {
    let metadata = fs::symlink_metadata(archive_path).map_err(|error| {
        mismatch(format!(
            "component archive unavailable {}: {error}",
            archive_path.display()
        ))
    })?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(mismatch(format!(
            "component archive must be a regular file: {}",
            archive_path.display()
        )));
    }

    let mut file = File::open(archive_path).map_err(|error| {
        mismatch(format!(
            "cannot open component archive {}: {error}",
            archive_path.display()
        ))
    })?;
    let mut found: Option<Vec<u8>> = None;
    let mut pending_path: Option<String> = None;

    loop {
        let mut header = [0_u8; TAR_BLOCK];
        match file.read_exact(&mut header) {
            Ok(()) => {}
            Err(error) if error.kind() == ErrorKind::UnexpectedEof => {
                return Err(mismatch(format!(
                    "truncated tar header in {}",
                    archive_path.display()
                )));
            }
            Err(error) => {
                return Err(mismatch(format!(
                    "cannot read tar header in {}: {error}",
                    archive_path.display()
                )));
            }
        }
        if header.iter().all(|byte| *byte == 0) {
            break;
        }

        let raw_path = tar_header_path(&header)?;
        let size = tar_octal(&header[124..136], "size")?;
        let typeflag = header[156];
        if typeflag == b'x' {
            let payload = read_tar_payload(&mut file, size, MAX_PAX_BYTES, archive_path)?;
            pending_path = parse_pax_path(&payload)?;
            continue;
        }
        if typeflag == b'L' {
            let payload = read_tar_payload(&mut file, size, MAX_PAX_BYTES, archive_path)?;
            let text = std::str::from_utf8(&payload)
                .map_err(|error| mismatch(format!("GNU tar long path is not UTF-8: {error}")))?;
            let value = text.trim_end_matches(|character| character == '\0' || character == '\n');
            if value.is_empty() {
                return Err(mismatch("GNU tar long path is empty"));
            }
            pending_path = Some(value.to_owned());
            continue;
        }

        let effective_path = pending_path.take().unwrap_or(raw_path);
        let regular = matches!(typeflag, 0 | b'0');
        if regular && basename(&effective_path) == target_basename {
            if found.is_some() {
                return Err(mismatch(format!(
                    "duplicate {target_basename} in {}",
                    archive_path.display()
                )));
            }
            found = Some(read_tar_payload(
                &mut file,
                size,
                MAX_MANIFEST_BYTES,
                archive_path,
            )?);
        } else {
            skip_tar_payload(&mut file, size, archive_path)?;
        }
    }

    found.ok_or_else(|| {
        mismatch(format!(
            "{target_basename} missing from {}",
            archive_path.display()
        ))
    })
}

fn tar_header_path(header: &[u8; TAR_BLOCK]) -> Result<String, ReleaseModelError> {
    let name = tar_text(&header[0..100], "name")?;
    let prefix = tar_text(&header[345..500], "prefix")?;
    if name.is_empty() {
        return Err(mismatch("tar entry name is empty"));
    }
    Ok(if prefix.is_empty() {
        name
    } else {
        format!("{prefix}/{name}")
    })
}

fn tar_text(field: &[u8], label: &str) -> Result<String, ReleaseModelError> {
    let end = field
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(field.len());
    std::str::from_utf8(&field[..end])
        .map(str::to_owned)
        .map_err(|error| mismatch(format!("tar {label} is not UTF-8: {error}")))
}

fn tar_octal(field: &[u8], label: &str) -> Result<u64, ReleaseModelError> {
    if field.first().is_some_and(|byte| *byte & 0x80 != 0) {
        return Err(mismatch(format!(
            "base-256 tar {label} is not allowed in component manifests"
        )));
    }
    let text = std::str::from_utf8(field)
        .map_err(|error| mismatch(format!("tar {label} is not UTF-8: {error}")))?;
    let trimmed = text.trim_matches(|character| character == '\0' || character == ' ');
    if trimmed.is_empty() {
        return Ok(0);
    }
    u64::from_str_radix(trimmed, 8)
        .map_err(|error| mismatch(format!("invalid tar {label}: {error}")))
}

fn read_tar_payload(
    file: &mut File,
    size: u64,
    limit: u64,
    archive_path: &Path,
) -> Result<Vec<u8>, ReleaseModelError> {
    if size > limit {
        return Err(mismatch(format!(
            "tar metadata payload too large in {}: {size}",
            archive_path.display()
        )));
    }
    let length = usize::try_from(size)
        .map_err(|_| mismatch("tar metadata payload size does not fit memory index"))?;
    let mut bytes = vec![0_u8; length];
    file.read_exact(&mut bytes).map_err(|error| {
        mismatch(format!(
            "truncated tar payload in {}: {error}",
            archive_path.display()
        ))
    })?;
    skip_padding(file, size, archive_path)?;
    Ok(bytes)
}

fn skip_tar_payload(
    file: &mut File,
    size: u64,
    archive_path: &Path,
) -> Result<(), ReleaseModelError> {
    let padded = size
        .checked_add(tar_padding(size))
        .ok_or_else(|| mismatch("tar payload offset overflow"))?;
    let offset = i64::try_from(padded).map_err(|_| mismatch("tar payload offset too large"))?;
    file.seek(SeekFrom::Current(offset)).map_err(|error| {
        mismatch(format!(
            "cannot skip tar payload in {}: {error}",
            archive_path.display()
        ))
    })?;
    Ok(())
}

fn skip_padding(file: &mut File, size: u64, archive_path: &Path) -> Result<(), ReleaseModelError> {
    let padding = tar_padding(size);
    if padding == 0 {
        return Ok(());
    }
    let offset = i64::try_from(padding).map_err(|_| mismatch("tar padding too large"))?;
    file.seek(SeekFrom::Current(offset)).map_err(|error| {
        mismatch(format!(
            "cannot skip tar padding in {}: {error}",
            archive_path.display()
        ))
    })?;
    Ok(())
}

const fn tar_padding(size: u64) -> u64 {
    (TAR_BLOCK as u64 - (size % TAR_BLOCK as u64)) % TAR_BLOCK as u64
}

fn parse_pax_path(payload: &[u8]) -> Result<Option<String>, ReleaseModelError> {
    let mut index = 0_usize;
    let mut path = None;
    while index < payload.len() {
        let relative_space = payload[index..]
            .iter()
            .position(|byte| *byte == b' ')
            .ok_or_else(|| mismatch("PAX record length delimiter missing"))?;
        let space = index + relative_space;
        let length_text = std::str::from_utf8(&payload[index..space])
            .map_err(|error| mismatch(format!("PAX record length is not UTF-8: {error}")))?;
        let length = length_text
            .parse::<usize>()
            .map_err(|error| mismatch(format!("invalid PAX record length: {error}")))?;
        if length == 0 {
            return Err(mismatch("PAX record length must be positive"));
        }
        let end = index
            .checked_add(length)
            .ok_or_else(|| mismatch("PAX record length overflow"))?;
        if end > payload.len() || end <= space + 1 || payload[end - 1] != b'\n' {
            return Err(mismatch("PAX record is truncated or malformed"));
        }
        let record = &payload[space + 1..end - 1];
        let equals = record
            .iter()
            .position(|byte| *byte == b'=')
            .ok_or_else(|| mismatch("PAX record key/value delimiter missing"))?;
        let key = std::str::from_utf8(&record[..equals])
            .map_err(|error| mismatch(format!("PAX key is not UTF-8: {error}")))?;
        if key == "path" {
            let value = std::str::from_utf8(&record[equals + 1..])
                .map_err(|error| mismatch(format!("PAX path is not UTF-8: {error}")))?;
            if value.is_empty() {
                return Err(mismatch("PAX path is empty"));
            }
            path = Some(value.to_owned());
        }
        index = end;
    }
    Ok(path)
}

fn basename(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

fn read_regular_bounded(path: &Path, limit: u64) -> Result<Vec<u8>, ReleaseModelError> {
    let metadata = fs::symlink_metadata(path).map_err(|error| {
        mismatch(format!(
            "component manifest unavailable {}: {error}",
            path.display()
        ))
    })?;
    if metadata.file_type().is_symlink() || !metadata.is_file() || metadata.len() > limit {
        return Err(mismatch(format!(
            "component manifest must be a bounded regular file: {}",
            path.display()
        )));
    }
    fs::read(path).map_err(|error| {
        mismatch(format!(
            "cannot read component manifest {}: {error}",
            path.display()
        ))
    })
}

fn mismatch(message: impl Into<String>) -> ReleaseModelError {
    ReleaseModelError::new(format!("COMPONENT_MANIFEST_MISMATCH: {}", message.into()))
}

#[cfg(test)]
mod tests {
    use super::{legacy_profile_bridge_manifest, verify_component_manifests};
    use crate::release::digest::{canonical_json, sha256_hex};
    use crate::release::model::{
        RELEASE_SET_ID_PREFIX, ReleaseComponentIdentity, ReleaseSetManifest,
    };
    use serde_json::{Value, json};
    use std::fs::{self, File};
    use std::io::Write;
    use std::path::{Path, PathBuf};

    const REPOSITORY: &str = "iamaman11/part-crm-emai-profile";
    const GIT_SHA: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

    fn temp_dir(label: &str) -> std::io::Result<PathBuf> {
        let root = std::env::temp_dir().join(format!(
            "opsctl-component-manifest-{label}-{}",
            std::process::id()
        ));
        if root.exists() {
            fs::remove_dir_all(&root)?;
        }
        fs::create_dir_all(root.join("components"))?;
        Ok(root)
    }

    fn write_tar(path: &Path, name: &str, bytes: &[u8]) -> std::io::Result<()> {
        let mut header = [0_u8; 512];
        let name_bytes = name.as_bytes();
        assert!(name_bytes.len() <= 100);
        header[..name_bytes.len()].copy_from_slice(name_bytes);
        header[100..108].copy_from_slice(b"0000644\0");
        header[108..116].copy_from_slice(b"0000000\0");
        header[116..124].copy_from_slice(b"0000000\0");
        let size = format!("{:011o}\0", bytes.len());
        header[124..136].copy_from_slice(size.as_bytes());
        header[136..148].copy_from_slice(b"00000000000\0");
        header[148..156].fill(b' ');
        header[156] = b'0';
        header[257..263].copy_from_slice(b"ustar\0");
        header[263..265].copy_from_slice(b"00");
        let checksum: u64 = header.iter().map(|byte| u64::from(*byte)).sum();
        let checksum_text = format!("{:06o}\0 ", checksum);
        header[148..156].copy_from_slice(checksum_text.as_bytes());

        let mut file = File::create(path)?;
        file.write_all(&header)?;
        file.write_all(bytes)?;
        let padding = (512 - (bytes.len() % 512)) % 512;
        file.write_all(&vec![0_u8; padding])?;
        file.write_all(&[0_u8; 1024])?;
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

    fn profile_component(archive: &[u8]) -> Result<(ReleaseComponentIdentity, Vec<u8>), String> {
        let artifact_sha = sha256_hex(archive);
        let payload = json!({
            "schema_version": 1,
            "kind": "PROFILE_BRIDGE_COMPONENT",
            "source_commit_sha": GIT_SHA,
            "artifact_sha256": artifact_sha,
            "artifact_size_bytes": archive.len(),
        });
        let release_id = format!(
            "profile-bridge-v1-sha256-{}",
            sha256_hex(canonical_json(&payload)?.as_bytes())
        );
        let temporary = ReleaseComponentIdentity {
            component_id: "profile_bridge".to_owned(),
            release_id,
            source_commit_sha: GIT_SHA.to_owned(),
            artifact_path: "components/profile-bridge.zip".to_owned(),
            artifact_sha256: artifact_sha,
            artifact_size_bytes: archive.len() as u64,
            component_manifest_sha256: String::new(),
        };
        let bytes =
            legacy_profile_bridge_manifest(&temporary).map_err(|error| error.to_string())?;
        let component = ReleaseComponentIdentity {
            component_manifest_sha256: sha256_hex(&bytes),
            ..temporary
        };
        Ok((component, bytes))
    }

    fn fixture(
        root: &Path,
        include_profile_sidecar: bool,
    ) -> Result<ReleaseSetManifest, Box<dyn std::error::Error>> {
        let control_manifest = b"{\"release_id\":\"cp\"}\n";
        let resolver_manifest = b"{\"release_id\":\"rs\"}\n";
        let runtime_manifest = b"{\"release_id\":\"rt\"}\n";
        write_tar(
            &root.join("components/control-plane.tar"),
            "release-manifest.json",
            control_manifest,
        )?;
        write_tar(
            &root.join("components/secret-resolver.tar"),
            "release-manifest.json",
            resolver_manifest,
        )?;
        write_tar(
            &root.join("components/runtime-bundle.tar"),
            "runtime-manifest.json",
            runtime_manifest,
        )?;
        let profile_archive = b"profile-bridge-executable";
        fs::write(root.join("components/profile-bridge.zip"), profile_archive)?;
        let (profile, profile_manifest) = profile_component(profile_archive)?;
        if include_profile_sidecar {
            fs::write(
                root.join("components/profile-bridge-manifest.json"),
                &profile_manifest,
            )?;
        }

        let artifact = |path: &str, kind: &str| -> Result<Value, Box<dyn std::error::Error>> {
            let bytes = fs::read(root.join(path))?;
            Ok(json!({
                "path": path,
                "sha256": sha256_hex(&bytes),
                "size_bytes": bytes.len(),
                "kind": kind,
            }))
        };
        let component = |release_id: &str,
                         path: &str,
                         manifest_bytes: &[u8]|
         -> Result<Value, Box<dyn std::error::Error>> {
            let bytes = fs::read(root.join(path))?;
            Ok(json!({
                "release_id": release_id,
                "source_commit_sha": GIT_SHA,
                "artifact_path": path,
                "artifact_sha256": sha256_hex(&bytes),
                "artifact_size_bytes": bytes.len(),
                "component_manifest_sha256": sha256_hex(manifest_bytes),
            }))
        };

        let mut inventory = vec![
            artifact("components/control-plane.tar", "component")?,
            artifact("components/profile-bridge.zip", "component")?,
            artifact("components/runtime-bundle.tar", "component")?,
            artifact("components/secret-resolver.tar", "component")?,
        ];
        if include_profile_sidecar {
            inventory.push(artifact(
                "components/profile-bridge-manifest.json",
                "manifest",
            )?);
        }

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
                "control_plane": component("cp", "components/control-plane.tar", control_manifest)?,
                "frontend": component("cp:frontend", "components/control-plane.tar", control_manifest)?,
                "secret_resolver": component("rs", "components/secret-resolver.tar", resolver_manifest)?,
                "runtime_bundle": component("rt", "components/runtime-bundle.tar", runtime_manifest)?,
                "profile_bridge": {
                    "release_id": profile.release_id,
                    "source_commit_sha": profile.source_commit_sha,
                    "artifact_path": profile.artifact_path,
                    "artifact_sha256": profile.artifact_sha256,
                    "artifact_size_bytes": profile.artifact_size_bytes,
                    "component_manifest_sha256": profile.component_manifest_sha256,
                },
            },
            "contracts": {},
            "protocols": {},
            "schemas": {},
            "runtime_compatibility": {},
            "capability_profile_compatibility": ["rehearsal-core-v1"],
            "build_provenance": {},
            "artifact_inventory": inventory,
        });
        let mut identity = value.clone();
        identity
            .as_object_mut()
            .ok_or("identity must be an object")?
            .remove("release_set_id");
        let digest = sha256_hex(canonical_json(&identity)?.as_bytes());
        value["release_set_id"] = Value::String(format!("{RELEASE_SET_ID_PREFIX}{digest}"));
        Ok(ReleaseSetManifest::parse_json(&serde_json::to_string(
            &value,
        )?)?)
    }

    #[test]
    fn verifies_embedded_manifests_and_current_profile_bridge_sidecar()
    -> Result<(), Box<dyn std::error::Error>> {
        let root = temp_dir("current")?;
        let manifest = fixture(&root, true)?;
        let result = verify_component_manifests(&manifest, &root)?;
        assert_eq!(result.verified_components, 5);
        assert_eq!(result.durable_manifest_bindings, 5);
        assert_eq!(result.legacy_reconstructed_manifests, 0);
        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn verifies_historical_profile_bridge_by_exact_legacy_reconstruction()
    -> Result<(), Box<dyn std::error::Error>> {
        let root = temp_dir("legacy")?;
        let manifest = fixture(&root, false)?;
        let result = verify_component_manifests(&manifest, &root)?;
        assert_eq!(result.verified_components, 5);
        assert_eq!(result.durable_manifest_bindings, 4);
        assert_eq!(result.legacy_reconstructed_manifests, 1);
        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn rejects_mismatched_embedded_manifest_digest() -> Result<(), Box<dyn std::error::Error>> {
        let root = temp_dir("embedded-mismatch")?;
        let manifest = fixture(&root, false)?;
        write_tar(
            &root.join("components/control-plane.tar"),
            "release-manifest.json",
            b"tampered\n",
        )?;
        let error = verify_component_manifests(&manifest, &root).expect_err("tamper must fail");
        assert!(error.to_string().contains("COMPONENT_MANIFEST_MISMATCH"));
        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn rejects_historical_profile_bridge_manifest_digest_drift()
    -> Result<(), Box<dyn std::error::Error>> {
        let root = temp_dir("legacy-mismatch")?;
        let manifest = fixture(&root, false)?;
        let mut component = manifest
            .components
            .get("profile_bridge")
            .ok_or("profile_bridge missing")?
            .clone();
        component.component_manifest_sha256 = "f".repeat(64);
        let bytes = legacy_profile_bridge_manifest(&component)?;
        assert!(super::verify_manifest_digest(&component, &bytes).is_err());
        fs::remove_dir_all(root)?;
        Ok(())
    }
}
