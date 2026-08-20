use crate::release::digest::sha256_hex;
use crate::release::model::{ReleaseComponentIdentity, ReleaseModelError, ReleaseSetManifest};
use serde_json::Value;
use std::fs::{self, File};
use std::io::{ErrorKind, Read, Seek, SeekFrom};
use std::path::Path;

const TAR_BLOCK: usize = 512;
const MAX_MANIFEST_BYTES: u64 = 4 * 1024 * 1024;
const MAX_PAX_BYTES: u64 = 1024 * 1024;
const ZIP_LOCAL_HEADER_SIGNATURE: u32 = 0x0403_4b50;
const ZIP_CENTRAL_HEADER_SIGNATURE: u32 = 0x0201_4b50;
const ZIP_END_SIGNATURE: u32 = 0x0605_4b50;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComponentManifestVerification {
    pub verified_components: Vec<String>,
}

pub fn verify_component_manifests(
    manifest: &ReleaseSetManifest,
    artifact_root: &Path,
) -> Result<ComponentManifestVerification, ReleaseModelError> {
    let mut verified = Vec::with_capacity(manifest.components.len());
    for component in manifest.components.values() {
        let archive = artifact_root.join(&component.artifact_path);
        let bytes = match component.component_id.as_str() {
            "control_plane" | "frontend" | "secret_resolver" => {
                read_unique_tar_member_by_basename(&archive, "release-manifest.json")?
            }
            "runtime_bundle" => {
                read_unique_tar_member_by_basename(&archive, "runtime-manifest.json")?
            }
            "profile_bridge" => {
                read_unique_zip_member(&archive, "profile-bridge-manifest.json")?
            }
            other => {
                return Err(mismatch(format!(
                    "unsupported component manifest location for {other}"
                )));
            }
        };
        verify_manifest(component, manifest, &bytes)?;
        verified.push(component.component_id.clone());
    }
    verified.sort();
    Ok(ComponentManifestVerification {
        verified_components: verified,
    })
}

fn verify_manifest(
    component: &ReleaseComponentIdentity,
    release_set: &ReleaseSetManifest,
    bytes: &[u8],
) -> Result<(), ReleaseModelError> {
    let observed = sha256_hex(bytes);
    if observed != component.component_manifest_sha256 {
        return Err(mismatch(format!(
            "{} manifest digest expected={} observed={observed}",
            component.component_id, component.component_manifest_sha256
        )));
    }
    let value: Value = serde_json::from_slice(bytes).map_err(|error| {
        mismatch(format!(
            "{} embedded manifest is invalid JSON: {error}",
            component.component_id
        ))
    })?;
    let object = value.as_object().ok_or_else(|| {
        mismatch(format!(
            "{} embedded manifest root must be an object",
            component.component_id
        ))
    })?;
    let release_id = object
        .get("release_id")
        .and_then(Value::as_str)
        .ok_or_else(|| mismatch(format!("{} manifest lacks release_id", component.component_id)))?;
    if release_id != component.release_id {
        return Err(mismatch(format!(
            "{} release_id differs from embedded manifest",
            component.component_id
        )));
    }
    let source_sha = object
        .get("source")
        .and_then(Value::as_object)
        .and_then(|source| source.get("commit_sha"))
        .and_then(Value::as_str)
        .or_else(|| object.get("source_commit_sha").and_then(Value::as_str))
        .ok_or_else(|| mismatch(format!("{} manifest lacks source SHA", component.component_id)))?;
    if source_sha != release_set.source.commit_sha || source_sha != component.source_commit_sha {
        return Err(ReleaseModelError::new(format!(
            "SOURCE_IDENTITY_MISMATCH: component {} embedded manifest source differs from Release Set",
            component.component_id
        )));
    }
    if component.component_id == "profile_bridge" {
        if object.get("schema_version").and_then(Value::as_u64) != Some(2)
            || object.get("kind").and_then(Value::as_str) != Some("PROFILE_BRIDGE_COMPONENT")
            || object.get("protocol_version").and_then(Value::as_u64)
                != Some(release_set.protocols.profile_bridge_protocol_version)
        {
            return Err(mismatch(
                "profile_bridge manifest identity/protocol differs from Release Set",
            ));
        }
    }
    Ok(())
}

fn read_unique_tar_member_by_basename(
    archive_path: &Path,
    target_basename: &str,
) -> Result<Vec<u8>, ReleaseModelError> {
    regular_archive(archive_path)?;
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
            let path = text.trim_end_matches(['\0', '\n']);
            if path.is_empty() {
                return Err(mismatch("GNU tar long path is empty"));
            }
            pending_path = Some(path.to_owned());
            continue;
        }
        let effective_path = pending_path.take().unwrap_or(raw_path);
        if matches!(typeflag, 0 | b'0') && basename(&effective_path) == target_basename {
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

fn read_unique_zip_member(
    archive_path: &Path,
    target_name: &str,
) -> Result<Vec<u8>, ReleaseModelError> {
    regular_archive(archive_path)?;
    let mut file = File::open(archive_path).map_err(|error| {
        mismatch(format!(
            "cannot open ZIP component archive {}: {error}",
            archive_path.display()
        ))
    })?;
    let mut found: Option<Vec<u8>> = None;
    loop {
        let mut signature = [0_u8; 4];
        match file.read_exact(&mut signature) {
            Ok(()) => {}
            Err(error) if error.kind() == ErrorKind::UnexpectedEof => break,
            Err(error) => {
                return Err(mismatch(format!(
                    "cannot read ZIP signature in {}: {error}",
                    archive_path.display()
                )));
            }
        }
        let signature = u32::from_le_bytes(signature);
        if matches!(signature, ZIP_CENTRAL_HEADER_SIGNATURE | ZIP_END_SIGNATURE) {
            break;
        }
        if signature != ZIP_LOCAL_HEADER_SIGNATURE {
            return Err(mismatch(format!(
                "unsupported ZIP record signature {signature:#x} in {}",
                archive_path.display()
            )));
        }
        let mut fixed = [0_u8; 26];
        file.read_exact(&mut fixed).map_err(|error| {
            mismatch(format!(
                "truncated ZIP local header in {}: {error}",
                archive_path.display()
            ))
        })?;
        let flags = u16::from_le_bytes([fixed[2], fixed[3]]);
        let compression = u16::from_le_bytes([fixed[4], fixed[5]]);
        let compressed_size = u32::from_le_bytes([fixed[14], fixed[15], fixed[16], fixed[17]]) as u64;
        let uncompressed_size = u32::from_le_bytes([fixed[18], fixed[19], fixed[20], fixed[21]]) as u64;
        let name_len = u16::from_le_bytes([fixed[22], fixed[23]]) as usize;
        let extra_len = u16::from_le_bytes([fixed[24], fixed[25]]) as usize;
        if flags & 0x0001 != 0 || flags & 0x0008 != 0 || compression != 0 {
            return Err(mismatch(
                "Profile Bridge ZIP must be unencrypted, STORED, and have sizes in local headers",
            ));
        }
        if compressed_size != uncompressed_size {
            return Err(mismatch("STORED ZIP member size mismatch"));
        }
        let mut name = vec![0_u8; name_len];
        file.read_exact(&mut name)
            .map_err(|error| mismatch(format!("truncated ZIP member name: {error}")))?;
        let name = std::str::from_utf8(&name)
            .map_err(|error| mismatch(format!("ZIP member name is not UTF-8: {error}")))?;
        if name.starts_with('/') || name.contains("..") || name.contains('\\') {
            return Err(mismatch(format!("unsafe ZIP member path: {name}")));
        }
        if extra_len > 0 {
            file.seek(SeekFrom::Current(i64::from(extra_len as u32)))
                .map_err(|error| mismatch(format!("cannot skip ZIP extra data: {error}")))?;
        }
        if uncompressed_size > MAX_MANIFEST_BYTES && name == target_name {
            return Err(mismatch("embedded Profile Bridge manifest is too large"));
        }
        if name == target_name {
            if found.is_some() {
                return Err(mismatch(format!("duplicate {target_name} in Profile Bridge ZIP")));
            }
            let length = usize::try_from(uncompressed_size)
                .map_err(|_| mismatch("ZIP member length does not fit memory index"))?;
            let mut bytes = vec![0_u8; length];
            file.read_exact(&mut bytes)
                .map_err(|error| mismatch(format!("truncated ZIP member payload: {error}")))?;
            found = Some(bytes);
        } else {
            let offset = i64::try_from(compressed_size)
                .map_err(|_| mismatch("ZIP member offset too large"))?;
            file.seek(SeekFrom::Current(offset))
                .map_err(|error| mismatch(format!("cannot skip ZIP member payload: {error}")))?;
        }
    }
    found.ok_or_else(|| mismatch(format!("{target_name} missing from Profile Bridge ZIP")))
}

fn regular_archive(path: &Path) -> Result<(), ReleaseModelError> {
    let metadata = fs::symlink_metadata(path).map_err(|error| {
        mismatch(format!("component archive unavailable {}: {error}", path.display()))
    })?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(mismatch(format!(
            "component archive must be a regular file: {}",
            path.display()
        )));
    }
    Ok(())
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
    let end = field.iter().position(|byte| *byte == 0).unwrap_or(field.len());
    std::str::from_utf8(&field[..end])
        .map(str::to_owned)
        .map_err(|error| mismatch(format!("tar {label} is not UTF-8: {error}")))
}

fn tar_octal(field: &[u8], label: &str) -> Result<u64, ReleaseModelError> {
    if field.first().is_some_and(|byte| *byte & 0x80 != 0) {
        return Err(mismatch(format!(
            "base-256 tar {label} is not allowed for component manifest discovery"
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
        let end = index
            .checked_add(length)
            .ok_or_else(|| mismatch("PAX record length overflow"))?;
        if length == 0 || end > payload.len() || end <= space + 1 || payload[end - 1] != b'\n' {
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

fn mismatch(message: impl Into<String>) -> ReleaseModelError {
    ReleaseModelError::new(format!("COMPONENT_MANIFEST_MISMATCH: {}", message.into()))
}

#[cfg(test)]
mod tests {
    use super::{read_unique_tar_member_by_basename, read_unique_zip_member};
    use std::fs;
    use std::io::Write;
    use std::path::PathBuf;

    #[test]
    fn rejects_non_archive() -> Result<(), Box<dyn std::error::Error>> {
        let path = PathBuf::from(std::env::temp_dir()).join(format!(
            "opsctl-component-manifest-invalid-{}",
            std::process::id()
        ));
        fs::write(&path, b"not an archive")?;
        assert!(read_unique_tar_member_by_basename(&path, "release-manifest.json").is_err());
        assert!(read_unique_zip_member(&path, "profile-bridge-manifest.json").is_err());
        fs::remove_file(path)?;
        Ok(())
    }

    #[test]
    fn empty_zip_cannot_fake_manifest() -> Result<(), Box<dyn std::error::Error>> {
        let path = PathBuf::from(std::env::temp_dir()).join(format!(
            "opsctl-component-manifest-empty-{}.zip",
            std::process::id()
        ));
        let mut file = fs::File::create(&path)?;
        file.write_all(&ZIP_END_SIGNATURE.to_le_bytes())?;
        assert!(read_unique_zip_member(&path, "profile-bridge-manifest.json").is_err());
        fs::remove_file(path)?;
        Ok(())
    }
}
