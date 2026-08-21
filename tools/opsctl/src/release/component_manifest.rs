use crate::release::digest::{canonical_json, sha256_hex, sha256_reader_hex};
use crate::release::model::{ReleaseComponentIdentity, ReleaseModelError, ReleaseSetManifest};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File};
use std::io::{ErrorKind, Read, Seek, SeekFrom};
use std::path::Path;

const TAR_BLOCK: usize = 512;
const MAX_MANIFEST_BYTES: u64 = 4 * 1024 * 1024;
const MAX_PAX_BYTES: u64 = 1024 * 1024;
const ZIP_LOCAL_HEADER_SIGNATURE: u32 = 0x0403_4b50;
const ZIP_CENTRAL_HEADER_SIGNATURE: u32 = 0x0201_4b50;
const ZIP_END_SIGNATURE: u32 = 0x0605_4b50;
const PROFILE_BRIDGE_MANIFEST: &str = "profile-bridge-manifest.json";
const PROFILE_BRIDGE_EXECUTABLE: &str = "profile-bridge.exe";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComponentManifestVerification {
    pub verified_components: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ProfileBridgeArchiveIdentity {
    manifest: Vec<u8>,
    executable_sha256: String,
    executable_size_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ZipLocalIdentity {
    local_offset: u64,
    flags: u16,
    compression: u16,
    compressed_size: u64,
    uncompressed_size: u64,
}

pub fn verify_component_manifests(
    manifest: &ReleaseSetManifest,
    artifact_root: &Path,
) -> Result<ComponentManifestVerification, ReleaseModelError> {
    let mut verified = Vec::with_capacity(manifest.components.len());
    for component in manifest.components.values() {
        let archive = artifact_root.join(&component.artifact_path);
        if component.component_id == "profile_bridge" {
            let bridge = read_profile_bridge_zip(&archive)?;
            verify_manifest(component, manifest, &bridge.manifest, Some(&bridge))?;
        } else {
            let bytes = match component.component_id.as_str() {
                "control_plane" | "frontend" | "secret_resolver" => {
                    read_unique_tar_member_by_basename(&archive, "release-manifest.json")?
                }
                "runtime_bundle" => {
                    read_unique_tar_member_by_basename(&archive, "runtime-manifest.json")?
                }
                other => {
                    return Err(mismatch(format!(
                        "unsupported component manifest location for {other}"
                    )));
                }
            };
            verify_manifest(component, manifest, &bytes, None)?;
        }
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
    profile_bridge: Option<&ProfileBridgeArchiveIdentity>,
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
        .ok_or_else(|| {
            mismatch(format!(
                "{} manifest lacks release_id",
                component.component_id
            ))
        })?;
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
        .ok_or_else(|| {
            mismatch(format!(
                "{} manifest lacks source SHA",
                component.component_id
            ))
        })?;
    if source_sha != release_set.source.commit_sha || source_sha != component.source_commit_sha {
        return Err(ReleaseModelError::new(format!(
            "SOURCE_IDENTITY_MISMATCH: component {} embedded manifest source differs from Release Set",
            component.component_id
        )));
    }
    if component.component_id == "profile_bridge" {
        verify_profile_bridge_manifest(object, release_set, release_id, profile_bridge)?;
    } else if profile_bridge.is_some() {
        return Err(mismatch(
            "Profile Bridge archive identity supplied for a non-bridge component",
        ));
    }
    Ok(())
}

fn verify_profile_bridge_manifest(
    object: &serde_json::Map<String, Value>,
    release_set: &ReleaseSetManifest,
    release_id: &str,
    profile_bridge: Option<&ProfileBridgeArchiveIdentity>,
) -> Result<(), ReleaseModelError> {
    let allowed = BTreeSet::from([
        "schema_version",
        "kind",
        "source_commit_sha",
        "protocol_version",
        "executable",
        "release_id",
    ]);
    if object.keys().any(|key| !allowed.contains(key.as_str())) || object.len() != allowed.len() {
        return Err(mismatch(
            "profile_bridge manifest field inventory is not the canonical v2 contract",
        ));
    }
    if object.get("schema_version").and_then(Value::as_u64) != Some(2)
        || object.get("kind").and_then(Value::as_str) != Some("PROFILE_BRIDGE_COMPONENT")
        || object.get("protocol_version").and_then(Value::as_u64)
            != Some(release_set.protocols.profile_bridge_protocol_version)
    {
        return Err(mismatch(
            "profile_bridge manifest identity/protocol differs from Release Set",
        ));
    }
    let bridge =
        profile_bridge.ok_or_else(|| mismatch("Profile Bridge ZIP identity is missing"))?;
    let executable = object
        .get("executable")
        .and_then(Value::as_object)
        .ok_or_else(|| mismatch("profile_bridge manifest executable must be an object"))?;
    let executable_allowed = BTreeSet::from(["path", "sha256", "size_bytes"]);
    if executable
        .keys()
        .any(|key| !executable_allowed.contains(key.as_str()))
        || executable.len() != executable_allowed.len()
    {
        return Err(mismatch(
            "profile_bridge executable field inventory is not canonical",
        ));
    }
    if executable.get("path").and_then(Value::as_str) != Some(PROFILE_BRIDGE_EXECUTABLE)
        || executable.get("sha256").and_then(Value::as_str)
            != Some(bridge.executable_sha256.as_str())
        || executable.get("size_bytes").and_then(Value::as_u64)
            != Some(bridge.executable_size_bytes)
    {
        return Err(mismatch(
            "profile_bridge executable identity differs from ZIP payload",
        ));
    }

    let mut release_identity = Value::Object(object.clone());
    release_identity
        .as_object_mut()
        .ok_or_else(|| mismatch("profile_bridge identity disappeared"))?
        .remove("release_id");
    let canonical = canonical_json(&release_identity).map_err(mismatch)?;
    let expected_release_id = format!(
        "profile-bridge-v2-sha256-{}",
        sha256_hex(canonical.as_bytes())
    );
    if release_id != expected_release_id {
        return Err(mismatch(
            "profile_bridge release_id is not content-addressed by its manifest identity",
        ));
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
        if typeflag == b'5' && size != 0 {
            return Err(mismatch("tar directory member must have zero payload size"));
        }
        let effective_path = pending_path.take().unwrap_or(raw_path);
        validate_archive_member_path(&effective_path, typeflag)?;
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

fn read_profile_bridge_zip(
    archive_path: &Path,
) -> Result<ProfileBridgeArchiveIdentity, ReleaseModelError> {
    regular_archive(archive_path)?;
    let mut file = File::open(archive_path).map_err(|error| {
        mismatch(format!(
            "cannot open ZIP component archive {}: {error}",
            archive_path.display()
        ))
    })?;
    let mut local = BTreeMap::<String, ZipLocalIdentity>::new();
    let mut manifest: Option<Vec<u8>> = None;
    let mut executable_sha256: Option<String> = None;
    let mut executable_size_bytes: Option<u64> = None;

    let first_central_signature = loop {
        let local_offset = file
            .stream_position()
            .map_err(|error| mismatch(format!("cannot locate ZIP record: {error}")))?;
        let signature = read_zip_signature(&mut file, archive_path)?;
        if signature == ZIP_CENTRAL_HEADER_SIGNATURE || signature == ZIP_END_SIGNATURE {
            break signature;
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
        let compressed_size =
            u32::from_le_bytes([fixed[14], fixed[15], fixed[16], fixed[17]]) as u64;
        let uncompressed_size =
            u32::from_le_bytes([fixed[18], fixed[19], fixed[20], fixed[21]]) as u64;
        let name_len = u16::from_le_bytes([fixed[22], fixed[23]]) as usize;
        let extra_len = u16::from_le_bytes([fixed[24], fixed[25]]) as u64;
        validate_zip_encoding(flags, compression, compressed_size, uncompressed_size)?;
        let name = read_zip_name(&mut file, name_len)?;
        validate_zip_member_name(&name)?;
        if !matches!(
            name.as_str(),
            PROFILE_BRIDGE_MANIFEST | PROFILE_BRIDGE_EXECUTABLE
        ) {
            return Err(mismatch(format!(
                "unknown member in Profile Bridge ZIP: {name}"
            )));
        }
        if local.contains_key(&name) {
            return Err(mismatch(format!("duplicate {name} in Profile Bridge ZIP")));
        }
        seek_forward(&mut file, extra_len, "ZIP extra data")?;
        if name == PROFILE_BRIDGE_MANIFEST {
            if uncompressed_size > MAX_MANIFEST_BYTES {
                return Err(mismatch("embedded Profile Bridge manifest is too large"));
            }
            let length = usize::try_from(uncompressed_size)
                .map_err(|_| mismatch("ZIP member length does not fit memory index"))?;
            let mut bytes = vec![0_u8; length];
            file.read_exact(&mut bytes)
                .map_err(|error| mismatch(format!("truncated ZIP member payload: {error}")))?;
            manifest = Some(bytes);
        } else {
            let mut payload = (&mut file).take(uncompressed_size);
            let digest = sha256_reader_hex(&mut payload).map_err(|error| {
                mismatch(format!("cannot hash Profile Bridge executable: {error}"))
            })?;
            if payload.limit() != 0 {
                return Err(mismatch("truncated Profile Bridge executable payload"));
            }
            executable_sha256 = Some(digest);
            executable_size_bytes = Some(uncompressed_size);
        }
        local.insert(
            name,
            ZipLocalIdentity {
                local_offset,
                flags,
                compression,
                compressed_size,
                uncompressed_size,
            },
        );
    };

    if local.len() != 2
        || !local.contains_key(PROFILE_BRIDGE_MANIFEST)
        || !local.contains_key(PROFILE_BRIDGE_EXECUTABLE)
    {
        return Err(mismatch(
            "Profile Bridge ZIP must contain exactly its manifest and executable",
        ));
    }
    verify_zip_central_directory(&mut file, archive_path, &local, first_central_signature)?;

    Ok(ProfileBridgeArchiveIdentity {
        manifest: manifest.ok_or_else(|| mismatch("Profile Bridge manifest payload missing"))?,
        executable_sha256: executable_sha256
            .ok_or_else(|| mismatch("Profile Bridge executable payload missing"))?,
        executable_size_bytes: executable_size_bytes
            .ok_or_else(|| mismatch("Profile Bridge executable size missing"))?,
    })
}

fn verify_zip_central_directory(
    file: &mut File,
    archive_path: &Path,
    local: &BTreeMap<String, ZipLocalIdentity>,
    mut signature: u32,
) -> Result<(), ReleaseModelError> {
    let mut central_names = BTreeSet::new();
    while signature == ZIP_CENTRAL_HEADER_SIGNATURE {
        let mut fixed = [0_u8; 42];
        file.read_exact(&mut fixed).map_err(|error| {
            mismatch(format!(
                "truncated ZIP central directory in {}: {error}",
                archive_path.display()
            ))
        })?;
        let flags = u16::from_le_bytes([fixed[4], fixed[5]]);
        let compression = u16::from_le_bytes([fixed[6], fixed[7]]);
        let compressed_size =
            u32::from_le_bytes([fixed[16], fixed[17], fixed[18], fixed[19]]) as u64;
        let uncompressed_size =
            u32::from_le_bytes([fixed[20], fixed[21], fixed[22], fixed[23]]) as u64;
        let name_len = u16::from_le_bytes([fixed[24], fixed[25]]) as usize;
        let extra_len = u16::from_le_bytes([fixed[26], fixed[27]]) as u64;
        let comment_len = u16::from_le_bytes([fixed[28], fixed[29]]) as u64;
        let disk_start = u16::from_le_bytes([fixed[30], fixed[31]]);
        let local_offset = u32::from_le_bytes([fixed[38], fixed[39], fixed[40], fixed[41]]) as u64;
        if disk_start != 0 {
            return Err(mismatch("multi-disk Profile Bridge ZIP is forbidden"));
        }
        validate_zip_encoding(flags, compression, compressed_size, uncompressed_size)?;
        let name = read_zip_name(file, name_len)?;
        validate_zip_member_name(&name)?;
        let observed = local
            .get(&name)
            .ok_or_else(|| mismatch(format!("central ZIP entry has no local member: {name}")))?;
        if !central_names.insert(name.clone()) {
            return Err(mismatch(format!("duplicate central ZIP entry: {name}")));
        }
        if observed.local_offset != local_offset
            || observed.flags != flags
            || observed.compression != compression
            || observed.compressed_size != compressed_size
            || observed.uncompressed_size != uncompressed_size
        {
            return Err(mismatch(format!(
                "central/local ZIP identity mismatch for {name}"
            )));
        }
        seek_forward(file, extra_len, "ZIP central extra data")?;
        seek_forward(file, comment_len, "ZIP central comment")?;
        signature = read_zip_signature(file, archive_path)?;
    }
    if signature != ZIP_END_SIGNATURE || central_names.len() != local.len() {
        return Err(mismatch(
            "Profile Bridge ZIP central directory inventory is incomplete or ambiguous",
        ));
    }

    let mut end = [0_u8; 18];
    file.read_exact(&mut end)
        .map_err(|error| mismatch(format!("truncated ZIP end record: {error}")))?;
    let disk = u16::from_le_bytes([end[0], end[1]]);
    let central_disk = u16::from_le_bytes([end[2], end[3]]);
    let entries_on_disk = u16::from_le_bytes([end[4], end[5]]) as usize;
    let total_entries = u16::from_le_bytes([end[6], end[7]]) as usize;
    let comment_len = u16::from_le_bytes([end[16], end[17]]) as u64;
    if disk != 0
        || central_disk != 0
        || entries_on_disk != local.len()
        || total_entries != local.len()
        || comment_len != 0
    {
        return Err(mismatch(
            "Profile Bridge ZIP end record is not canonical single-disk/no-comment form",
        ));
    }
    let mut trailing = [0_u8; 1];
    match file.read(&mut trailing) {
        Ok(0) => Ok(()),
        Ok(_) => Err(mismatch(
            "trailing bytes after Profile Bridge ZIP end record",
        )),
        Err(error) => Err(mismatch(format!("cannot read ZIP trailer: {error}"))),
    }
}

fn validate_zip_encoding(
    flags: u16,
    compression: u16,
    compressed_size: u64,
    uncompressed_size: u64,
) -> Result<(), ReleaseModelError> {
    if flags & 0x0001 != 0 || flags & 0x0008 != 0 || compression != 0 {
        return Err(mismatch(
            "Profile Bridge ZIP must be unencrypted, STORED, and have sizes in local headers",
        ));
    }
    if compressed_size != uncompressed_size {
        return Err(mismatch("STORED ZIP member size mismatch"));
    }
    Ok(())
}

fn read_zip_signature(file: &mut File, archive_path: &Path) -> Result<u32, ReleaseModelError> {
    let mut signature = [0_u8; 4];
    file.read_exact(&mut signature).map_err(|error| {
        mismatch(format!(
            "cannot read ZIP signature in {}: {error}",
            archive_path.display()
        ))
    })?;
    Ok(u32::from_le_bytes(signature))
}

fn read_zip_name(file: &mut File, name_len: usize) -> Result<String, ReleaseModelError> {
    if name_len == 0 {
        return Err(mismatch("ZIP member name is empty"));
    }
    let mut name = vec![0_u8; name_len];
    file.read_exact(&mut name)
        .map_err(|error| mismatch(format!("truncated ZIP member name: {error}")))?;
    std::str::from_utf8(&name)
        .map(str::to_owned)
        .map_err(|error| mismatch(format!("ZIP member name is not UTF-8: {error}")))
}

fn validate_zip_member_name(name: &str) -> Result<(), ReleaseModelError> {
    if name.starts_with('/')
        || name.contains('\\')
        || name
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == "..")
    {
        return Err(mismatch(format!("unsafe ZIP member path: {name}")));
    }
    Ok(())
}

fn seek_forward(file: &mut File, distance: u64, label: &str) -> Result<(), ReleaseModelError> {
    if distance == 0 {
        return Ok(());
    }
    let offset = i64::try_from(distance).map_err(|_| mismatch(format!("{label} is too large")))?;
    file.seek(SeekFrom::Current(offset))
        .map_err(|error| mismatch(format!("cannot skip {label}: {error}")))?;
    Ok(())
}

fn regular_archive(path: &Path) -> Result<(), ReleaseModelError> {
    let metadata = fs::symlink_metadata(path).map_err(|error| {
        mismatch(format!(
            "component archive unavailable {}: {error}",
            path.display()
        ))
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

fn validate_archive_member_path(path: &str, typeflag: u8) -> Result<(), ReleaseModelError> {
    let normalized = if typeflag == b'5' {
        path.strip_suffix('/').unwrap_or(path)
    } else {
        path
    };
    if normalized.is_empty()
        || normalized.starts_with('/')
        || normalized.contains('\\')
        || normalized
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == "..")
    {
        return Err(mismatch(format!("unsafe tar member path: {path}")));
    }
    Ok(())
}

fn basename(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

fn mismatch(message: impl Into<String>) -> ReleaseModelError {
    ReleaseModelError::new(format!("COMPONENT_MANIFEST_MISMATCH: {}", message.into()))
}

#[cfg(test)]
mod tests {
    use super::{
        PROFILE_BRIDGE_EXECUTABLE, ZIP_CENTRAL_HEADER_SIGNATURE, ZIP_END_SIGNATURE,
        ZIP_LOCAL_HEADER_SIGNATURE, read_profile_bridge_zip, read_unique_tar_member_by_basename,
        validate_archive_member_path,
    };
    use std::fs;
    use std::io::Write;
    use std::path::Path;

    #[test]
    fn rejects_non_archive() -> Result<(), Box<dyn std::error::Error>> {
        let path = std::env::temp_dir().join(format!(
            "opsctl-component-manifest-invalid-{}",
            std::process::id()
        ));
        fs::write(&path, b"not an archive")?;
        assert!(read_unique_tar_member_by_basename(&path, "release-manifest.json").is_err());
        assert!(read_profile_bridge_zip(&path).is_err());
        fs::remove_file(path)?;
        Ok(())
    }

    #[test]
    fn safe_tar_directory_trailing_slash_is_type_bound() {
        assert!(validate_archive_member_path("cloudflare-release/", b'5').is_ok());
        assert!(validate_archive_member_path("cloudflare-release/", b'0').is_err());
        assert!(validate_archive_member_path("../", b'5').is_err());
        assert!(validate_archive_member_path("nested//", b'5').is_err());
        assert!(validate_archive_member_path("/absolute/", b'5').is_err());
        assert!(validate_archive_member_path("nested\\escape/", b'5').is_err());
    }

    #[test]
    fn unsafe_tar_member_is_rejected() -> Result<(), Box<dyn std::error::Error>> {
        let path = std::env::temp_dir().join(format!(
            "opsctl-component-manifest-unsafe-tar-{}.tar",
            std::process::id()
        ));
        write_tar_members(&path, &[("../release-manifest.json", b"{}")])?;
        assert!(read_unique_tar_member_by_basename(&path, "release-manifest.json").is_err());
        fs::remove_file(path)?;
        Ok(())
    }

    #[test]
    fn duplicate_tar_manifest_is_rejected() -> Result<(), Box<dyn std::error::Error>> {
        let path = std::env::temp_dir().join(format!(
            "opsctl-component-manifest-duplicate-tar-{}.tar",
            std::process::id()
        ));
        write_tar_members(
            &path,
            &[
                ("one/release-manifest.json", b"{}"),
                ("two/release-manifest.json", b"{}"),
            ],
        )?;
        assert!(read_unique_tar_member_by_basename(&path, "release-manifest.json").is_err());
        fs::remove_file(path)?;
        Ok(())
    }

    #[test]
    fn empty_zip_cannot_fake_manifest() -> Result<(), Box<dyn std::error::Error>> {
        let path = std::env::temp_dir().join(format!(
            "opsctl-component-manifest-empty-{}.zip",
            std::process::id()
        ));
        let mut file = fs::File::create(&path)?;
        file.write_all(&ZIP_END_SIGNATURE.to_le_bytes())?;
        file.write_all(&[0_u8; 18])?;
        assert!(read_profile_bridge_zip(&path).is_err());
        fs::remove_file(path)?;
        Ok(())
    }

    #[test]
    fn unsafe_zip_member_is_rejected() -> Result<(), Box<dyn std::error::Error>> {
        let path = std::env::temp_dir().join(format!(
            "opsctl-component-manifest-unsafe-{}.zip",
            std::process::id()
        ));
        write_local_member(&path, "../profile-bridge-manifest.json", b"{}")?;
        assert!(read_profile_bridge_zip(&path).is_err());
        fs::remove_file(path)?;
        Ok(())
    }

    #[test]
    fn unknown_zip_member_is_rejected() -> Result<(), Box<dyn std::error::Error>> {
        let path = std::env::temp_dir().join(format!(
            "opsctl-component-manifest-unknown-{}.zip",
            std::process::id()
        ));
        write_local_member(&path, "unexpected.txt", b"x")?;
        assert!(read_profile_bridge_zip(&path).is_err());
        fs::remove_file(path)?;
        Ok(())
    }

    fn write_tar_members(
        path: &Path,
        members: &[(&str, &[u8])],
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut file = fs::File::create(path)?;
        for (name, payload) in members {
            if name.len() > 100 {
                return Err("test TAR member name too long".into());
            }
            let mut header = [0_u8; 512];
            header[..name.len()].copy_from_slice(name.as_bytes());
            let size = format!("{:011o}\0", payload.len());
            header[124..136].copy_from_slice(size.as_bytes());
            header[156] = b'0';
            file.write_all(&header)?;
            file.write_all(payload)?;
            let padding = (512 - (payload.len() % 512)) % 512;
            file.write_all(&vec![0_u8; padding])?;
        }
        file.write_all(&[0_u8; 1024])?;
        Ok(())
    }

    fn write_local_member(
        path: &Path,
        name: &str,
        payload: &[u8],
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut file = fs::File::create(path)?;
        file.write_all(&ZIP_LOCAL_HEADER_SIGNATURE.to_le_bytes())?;
        let mut fixed = [0_u8; 26];
        fixed[0..2].copy_from_slice(&20_u16.to_le_bytes());
        fixed[14..18].copy_from_slice(&(payload.len() as u32).to_le_bytes());
        fixed[18..22].copy_from_slice(&(payload.len() as u32).to_le_bytes());
        fixed[22..24].copy_from_slice(&(name.len() as u16).to_le_bytes());
        file.write_all(&fixed)?;
        file.write_all(name.as_bytes())?;
        file.write_all(payload)?;
        file.write_all(&ZIP_CENTRAL_HEADER_SIGNATURE.to_le_bytes())?;
        file.write_all(&[0_u8; 42])?;
        file.write_all(&ZIP_END_SIGNATURE.to_le_bytes())?;
        file.write_all(&[0_u8; 18])?;
        let _ = PROFILE_BRIDGE_EXECUTABLE;
        Ok(())
    }
}
