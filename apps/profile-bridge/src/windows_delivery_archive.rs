#![forbid(unsafe_code)]

use crate::windows_delivery_staging::{
    DeliveryArchiveEntry, DeliveryArchiveEntryKind, DeliveryArchiveReader, DeliveryComponentKind,
};
use sha2::{Digest, Sha256};
use std::fmt;
use std::fs::{self, File, Metadata};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

const ZIP_LOCAL_FILE_HEADER: u32 = 0x0403_4b50;
const ZIP_CENTRAL_DIRECTORY_HEADER: u32 = 0x0201_4b50;
const ZIP_END_OF_CENTRAL_DIRECTORY: u32 = 0x0605_4b50;
const ZIP_EOCD_BYTES: u64 = 22;
const ZIP_ALLOWED_FLAGS: u16 = 1 << 11;
const ZIP_STORED_METHOD: u16 = 0;
const ZIP_UNIX_SYSTEM: u8 = 3;
const UNIX_FILE_TYPE_MASK: u32 = 0o170000;
const UNIX_REGULAR_FILE: u32 = 0o100000;
const UNIX_DIRECTORY: u32 = 0o040000;
const TAR_BLOCK_BYTES: u64 = 512;
const TAR_PAX_HEADER: u8 = b'x';
const TAR_GLOBAL_PAX_HEADER: u8 = b'g';
const TAR_REGULAR_FILE: u8 = b'0';
const TAR_DIRECTORY: u8 = b'5';
const MAX_ARCHIVE_ENTRIES: usize = 500_000;
const MAX_ARCHIVE_BYTES: u64 = 8 * 1024 * 1024 * 1024;
const MAX_PAX_HEADER_BYTES: u64 = 1024 * 1024;
const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0400;
const LOWER_HEX: &[u8; 16] = b"0123456789abcdef";

#[derive(Debug, Default)]
pub struct WindowsDeliveryArchiveReader {
    profile_bridge: Option<CachedArchive>,
    runtime_bundle: Option<CachedArchive>,
}

impl WindowsDeliveryArchiveReader {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            profile_bridge: None,
            runtime_bundle: None,
        }
    }

    fn cache_slot_mut(&mut self, component: DeliveryComponentKind) -> &mut Option<CachedArchive> {
        match component {
            DeliveryComponentKind::ProfileBridge => &mut self.profile_bridge,
            DeliveryComponentKind::RuntimeBundle => &mut self.runtime_bundle,
        }
    }
}

impl DeliveryArchiveReader for WindowsDeliveryArchiveReader {
    type Error = WindowsDeliveryArchiveError;

    fn entries(
        &mut self,
        component: DeliveryComponentKind,
        artifact_path: &Path,
    ) -> Result<Vec<DeliveryArchiveEntry>, Self::Error> {
        validate_artifact_path(artifact_path)?;
        let mut file = File::open(artifact_path).map_err(|_| WindowsDeliveryArchiveError::Io)?;
        let parsed = match component {
            DeliveryComponentKind::ProfileBridge => parse_profile_bridge_zip(&mut file)?,
            DeliveryComponentKind::RuntimeBundle => parse_runtime_pax_tar(&mut file)?,
        };
        let result = parsed
            .iter()
            .map(ParsedArchiveEntry::delivery_entry)
            .collect();
        *self.cache_slot_mut(component) = Some(CachedArchive {
            artifact_path: artifact_path.to_path_buf(),
            file,
            entries: parsed,
        });
        Ok(result)
    }

    fn copy_regular_file(
        &mut self,
        component: DeliveryComponentKind,
        artifact_path: &Path,
        entry_index: usize,
        writer: &mut dyn Write,
    ) -> Result<(), Self::Error> {
        let cache = self
            .cache_slot_mut(component)
            .as_mut()
            .ok_or(WindowsDeliveryArchiveError::CacheMiss)?;
        if cache.artifact_path != artifact_path {
            return Err(WindowsDeliveryArchiveError::CacheMiss);
        }
        let entry = cache
            .entries
            .get(entry_index)
            .ok_or(WindowsDeliveryArchiveError::InvalidArchive)?;
        if entry.kind != DeliveryArchiveEntryKind::RegularFile {
            return Err(WindowsDeliveryArchiveError::InvalidArchive);
        }
        copy_region(&mut cache.file, entry.data_offset, entry.size_bytes, writer)
    }
}

#[derive(Debug)]
struct CachedArchive {
    artifact_path: PathBuf,
    file: File,
    entries: Vec<ParsedArchiveEntry>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ParsedArchiveEntry {
    relative_path: String,
    kind: DeliveryArchiveEntryKind,
    size_bytes: u64,
    sha256: String,
    data_offset: u64,
}

impl ParsedArchiveEntry {
    fn delivery_entry(&self) -> DeliveryArchiveEntry {
        match self.kind {
            DeliveryArchiveEntryKind::RegularFile => DeliveryArchiveEntry::regular_file(
                self.relative_path.clone(),
                self.size_bytes,
                self.sha256.clone(),
            ),
            DeliveryArchiveEntryKind::LinkOrSpecial => {
                DeliveryArchiveEntry::link_or_special(self.relative_path.clone())
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WindowsDeliveryArchiveError {
    Io,
    InvalidArchive,
    CacheMiss,
}

impl fmt::Display for WindowsDeliveryArchiveError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Io => "Windows delivery archive I/O failed",
            Self::InvalidArchive => "Windows delivery archive is not the canonical safe format",
            Self::CacheMiss => "Windows delivery archive stream was not opened by this reader",
        })
    }
}

impl std::error::Error for WindowsDeliveryArchiveError {}

fn validate_artifact_path(path: &Path) -> Result<(), WindowsDeliveryArchiveError> {
    if !path.is_absolute() {
        return Err(WindowsDeliveryArchiveError::InvalidArchive);
    }
    let metadata = fs::symlink_metadata(path).map_err(|_| WindowsDeliveryArchiveError::Io)?;
    if metadata_is_link_or_reparse(&metadata)
        || !metadata.is_file()
        || metadata.len() == 0
        || metadata.len() > MAX_ARCHIVE_BYTES
    {
        return Err(WindowsDeliveryArchiveError::InvalidArchive);
    }
    Ok(())
}

fn parse_profile_bridge_zip(
    file: &mut File,
) -> Result<Vec<ParsedArchiveEntry>, WindowsDeliveryArchiveError> {
    let archive_len = file
        .metadata()
        .map_err(|_| WindowsDeliveryArchiveError::Io)?
        .len();
    if archive_len < ZIP_EOCD_BYTES || archive_len > u64::from(u32::MAX) {
        return Err(WindowsDeliveryArchiveError::InvalidArchive);
    }
    let eocd_offset = archive_len - ZIP_EOCD_BYTES;
    let eocd = read_exact_at::<22>(file, eocd_offset)?;
    if le_u32(&eocd, 0)? != ZIP_END_OF_CENTRAL_DIRECTORY
        || le_u16(&eocd, 4)? != 0
        || le_u16(&eocd, 6)? != 0
        || le_u16(&eocd, 20)? != 0
    {
        return Err(WindowsDeliveryArchiveError::InvalidArchive);
    }
    let entries_on_disk = le_u16(&eocd, 8)?;
    let entry_count = le_u16(&eocd, 10)?;
    if entries_on_disk != entry_count || usize::from(entry_count) > MAX_ARCHIVE_ENTRIES {
        return Err(WindowsDeliveryArchiveError::InvalidArchive);
    }
    let central_size = u64::from(le_u32(&eocd, 12)?);
    let central_offset = u64::from(le_u32(&eocd, 16)?);
    let central_end = checked_add(central_offset, central_size)?;
    if central_end != eocd_offset {
        return Err(WindowsDeliveryArchiveError::InvalidArchive);
    }

    let mut cursor = central_offset;
    let mut parsed = Vec::with_capacity(usize::from(entry_count));
    let mut occupied = Vec::with_capacity(usize::from(entry_count));
    for _ in 0..entry_count {
        let central = read_exact_at::<46>(file, cursor)?;
        if le_u32(&central, 0)? != ZIP_CENTRAL_DIRECTORY_HEADER {
            return Err(WindowsDeliveryArchiveError::InvalidArchive);
        }
        let version_made_by = le_u16(&central, 4)?;
        if u8::try_from(version_made_by >> 8)
            .map_err(|_| WindowsDeliveryArchiveError::InvalidArchive)?
            != ZIP_UNIX_SYSTEM
        {
            return Err(WindowsDeliveryArchiveError::InvalidArchive);
        }
        let flags = le_u16(&central, 8)?;
        let compression = le_u16(&central, 10)?;
        let expected_crc32 = le_u32(&central, 16)?;
        let compressed_size = le_u32(&central, 20)?;
        let uncompressed_size = le_u32(&central, 24)?;
        let name_len = usize::from(le_u16(&central, 28)?);
        let extra_len = usize::from(le_u16(&central, 30)?);
        let comment_len = usize::from(le_u16(&central, 32)?);
        let disk_start = le_u16(&central, 34)?;
        let external_attributes = le_u32(&central, 38)?;
        let local_header_offset = u64::from(le_u32(&central, 42)?);
        if flags & !ZIP_ALLOWED_FLAGS != 0
            || compression != ZIP_STORED_METHOD
            || compressed_size != uncompressed_size
            || name_len == 0
            || extra_len != 0
            || comment_len != 0
            || disk_start != 0
        {
            return Err(WindowsDeliveryArchiveError::InvalidArchive);
        }
        let central_record_len = 46_u64
            .checked_add(
                u64::try_from(name_len).map_err(|_| WindowsDeliveryArchiveError::InvalidArchive)?,
            )
            .and_then(|value| value.checked_add(u64::try_from(extra_len).ok()?))
            .and_then(|value| value.checked_add(u64::try_from(comment_len).ok()?))
            .ok_or(WindowsDeliveryArchiveError::InvalidArchive)?;
        let central_record_end = checked_add(cursor, central_record_len)?;
        if central_record_end > central_end {
            return Err(WindowsDeliveryArchiveError::InvalidArchive);
        }
        let name_bytes = read_bytes_at(file, checked_add(cursor, 46)?, name_len)?;
        let relative_path = utf8_archive_path(name_bytes)?;

        let local = read_exact_at::<30>(file, local_header_offset)?;
        if le_u32(&local, 0)? != ZIP_LOCAL_FILE_HEADER
            || le_u16(&local, 6)? != flags
            || le_u16(&local, 8)? != compression
            || le_u32(&local, 14)? != expected_crc32
            || le_u32(&local, 18)? != compressed_size
            || le_u32(&local, 22)? != uncompressed_size
            || usize::from(le_u16(&local, 26)?) != name_len
            || le_u16(&local, 28)? != 0
        {
            return Err(WindowsDeliveryArchiveError::InvalidArchive);
        }
        let local_name_offset = checked_add(local_header_offset, 30)?;
        if read_bytes_at(file, local_name_offset, name_len)? != relative_path.as_bytes() {
            return Err(WindowsDeliveryArchiveError::InvalidArchive);
        }
        let data_offset = checked_add(
            local_name_offset,
            u64::try_from(name_len).map_err(|_| WindowsDeliveryArchiveError::InvalidArchive)?,
        )?;
        let data_end = checked_add(data_offset, u64::from(compressed_size))?;
        if data_end > central_offset {
            return Err(WindowsDeliveryArchiveError::InvalidArchive);
        }
        let identity = hash_and_crc_region(file, data_offset, u64::from(uncompressed_size))?;
        if identity.crc32 != expected_crc32 {
            return Err(WindowsDeliveryArchiveError::InvalidArchive);
        }

        let unix_mode = external_attributes >> 16;
        let file_type = unix_mode & UNIX_FILE_TYPE_MASK;
        let trailing_slash = relative_path.ends_with('/');
        match (file_type, trailing_slash) {
            (UNIX_DIRECTORY, true) => {}
            (UNIX_REGULAR_FILE, false) => parsed.push(ParsedArchiveEntry {
                relative_path,
                kind: DeliveryArchiveEntryKind::RegularFile,
                size_bytes: u64::from(uncompressed_size),
                sha256: identity.sha256,
                data_offset,
            }),
            _ => parsed.push(ParsedArchiveEntry {
                relative_path,
                kind: DeliveryArchiveEntryKind::LinkOrSpecial,
                size_bytes: 0,
                sha256: String::new(),
                data_offset,
            }),
        }
        occupied.push((local_header_offset, data_end));
        cursor = central_record_end;
    }
    if cursor != central_end || parsed.len() > MAX_ARCHIVE_ENTRIES {
        return Err(WindowsDeliveryArchiveError::InvalidArchive);
    }
    occupied.sort_unstable_by_key(|range| range.0);
    if occupied
        .windows(2)
        .any(|pair| pair[0].1 > pair[1].0 || pair[1].1 > central_offset)
        || occupied
            .first()
            .is_some_and(|range| range.1 > central_offset)
    {
        return Err(WindowsDeliveryArchiveError::InvalidArchive);
    }
    Ok(parsed)
}

fn parse_runtime_pax_tar(
    file: &mut File,
) -> Result<Vec<ParsedArchiveEntry>, WindowsDeliveryArchiveError> {
    let archive_len = file
        .metadata()
        .map_err(|_| WindowsDeliveryArchiveError::Io)?
        .len();
    if archive_len < TAR_BLOCK_BYTES * 2
        || archive_len % TAR_BLOCK_BYTES != 0
        || archive_len > MAX_ARCHIVE_BYTES
    {
        return Err(WindowsDeliveryArchiveError::InvalidArchive);
    }

    let mut offset = 0_u64;
    let mut pending_pax: Option<PaxOverrides> = None;
    let mut parsed = Vec::new();
    let mut terminated = false;
    while offset < archive_len {
        let header = read_exact_at::<512>(file, offset)?;
        if header.iter().all(|byte| *byte == 0) {
            let second_offset = checked_add(offset, TAR_BLOCK_BYTES)?;
            if second_offset >= archive_len
                || !read_exact_at::<512>(file, second_offset)?
                    .iter()
                    .all(|byte| *byte == 0)
            {
                return Err(WindowsDeliveryArchiveError::InvalidArchive);
            }
            ensure_zero_tail(
                file,
                checked_add(second_offset, TAR_BLOCK_BYTES)?,
                archive_len,
            )?;
            terminated = true;
            break;
        }
        validate_tar_header(&header)?;
        let header_size = parse_tar_octal(&header[124..136])?;
        let data_offset = checked_add(offset, TAR_BLOCK_BYTES)?;
        let data_end = checked_add(data_offset, header_size)?;
        let next_offset = round_up_tar_block(data_end)?;
        if next_offset > archive_len {
            return Err(WindowsDeliveryArchiveError::InvalidArchive);
        }
        let header_path = tar_header_path(&header)?;
        let type_flag = header[156];

        match type_flag {
            TAR_PAX_HEADER => {
                if pending_pax.is_some() || header_size == 0 || header_size > MAX_PAX_HEADER_BYTES {
                    return Err(WindowsDeliveryArchiveError::InvalidArchive);
                }
                let payload = read_bytes_at(
                    file,
                    data_offset,
                    usize::try_from(header_size)
                        .map_err(|_| WindowsDeliveryArchiveError::InvalidArchive)?,
                )?;
                pending_pax = Some(parse_pax_overrides(&payload)?);
            }
            TAR_GLOBAL_PAX_HEADER | b'L' | b'K' => {
                return Err(WindowsDeliveryArchiveError::InvalidArchive);
            }
            TAR_REGULAR_FILE | 0 => {
                let overrides = pending_pax.take().unwrap_or_default();
                if overrides.size.is_some_and(|size| size != header_size) {
                    return Err(WindowsDeliveryArchiveError::InvalidArchive);
                }
                let relative_path = overrides.path.unwrap_or(header_path);
                if relative_path.is_empty() {
                    return Err(WindowsDeliveryArchiveError::InvalidArchive);
                }
                let identity = hash_and_crc_region(file, data_offset, header_size)?;
                parsed.push(ParsedArchiveEntry {
                    relative_path,
                    kind: DeliveryArchiveEntryKind::RegularFile,
                    size_bytes: header_size,
                    sha256: identity.sha256,
                    data_offset,
                });
            }
            TAR_DIRECTORY => {
                let overrides = pending_pax.take().unwrap_or_default();
                if header_size != 0
                    || overrides.size.is_some_and(|size| size != 0)
                    || overrides.path.unwrap_or(header_path).is_empty()
                {
                    return Err(WindowsDeliveryArchiveError::InvalidArchive);
                }
            }
            _ => {
                let overrides = pending_pax.take().unwrap_or_default();
                if overrides.size.is_some_and(|size| size != header_size) {
                    return Err(WindowsDeliveryArchiveError::InvalidArchive);
                }
                let relative_path = overrides.path.unwrap_or(header_path);
                if relative_path.is_empty() {
                    return Err(WindowsDeliveryArchiveError::InvalidArchive);
                }
                parsed.push(ParsedArchiveEntry {
                    relative_path,
                    kind: DeliveryArchiveEntryKind::LinkOrSpecial,
                    size_bytes: 0,
                    sha256: String::new(),
                    data_offset,
                });
            }
        }
        if parsed.len() > MAX_ARCHIVE_ENTRIES {
            return Err(WindowsDeliveryArchiveError::InvalidArchive);
        }
        offset = next_offset;
    }
    if !terminated || pending_pax.is_some() || parsed.is_empty() {
        return Err(WindowsDeliveryArchiveError::InvalidArchive);
    }
    Ok(parsed)
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct PaxOverrides {
    path: Option<String>,
    size: Option<u64>,
}

fn parse_pax_overrides(payload: &[u8]) -> Result<PaxOverrides, WindowsDeliveryArchiveError> {
    let mut cursor = 0_usize;
    let mut overrides = PaxOverrides::default();
    while cursor < payload.len() {
        let relative_space = payload[cursor..]
            .iter()
            .position(|byte| *byte == b' ')
            .ok_or(WindowsDeliveryArchiveError::InvalidArchive)?;
        let space = cursor
            .checked_add(relative_space)
            .ok_or(WindowsDeliveryArchiveError::InvalidArchive)?;
        let length_text = std::str::from_utf8(&payload[cursor..space])
            .map_err(|_| WindowsDeliveryArchiveError::InvalidArchive)?;
        if length_text.is_empty()
            || (length_text.len() > 1 && length_text.starts_with('0'))
            || !length_text.bytes().all(|byte| byte.is_ascii_digit())
        {
            return Err(WindowsDeliveryArchiveError::InvalidArchive);
        }
        let record_len = length_text
            .parse::<usize>()
            .map_err(|_| WindowsDeliveryArchiveError::InvalidArchive)?;
        let record_end = cursor
            .checked_add(record_len)
            .ok_or(WindowsDeliveryArchiveError::InvalidArchive)?;
        if record_end > payload.len() || record_end <= space + 2 || payload[record_end - 1] != b'\n'
        {
            return Err(WindowsDeliveryArchiveError::InvalidArchive);
        }
        let record = &payload[space + 1..record_end - 1];
        let equals = record
            .iter()
            .position(|byte| *byte == b'=')
            .ok_or(WindowsDeliveryArchiveError::InvalidArchive)?;
        let key = std::str::from_utf8(&record[..equals])
            .map_err(|_| WindowsDeliveryArchiveError::InvalidArchive)?;
        let value = std::str::from_utf8(&record[equals + 1..])
            .map_err(|_| WindowsDeliveryArchiveError::InvalidArchive)?;
        match key {
            "path" if overrides.path.is_none() && !value.is_empty() && !value.contains('\0') => {
                overrides.path = Some(value.to_owned());
            }
            "size"
                if overrides.size.is_none()
                    && !value.is_empty()
                    && value.bytes().all(|byte| byte.is_ascii_digit()) =>
            {
                overrides.size = Some(
                    value
                        .parse::<u64>()
                        .map_err(|_| WindowsDeliveryArchiveError::InvalidArchive)?,
                );
            }
            _ => return Err(WindowsDeliveryArchiveError::InvalidArchive),
        }
        cursor = record_end;
    }
    if overrides.path.is_none() && overrides.size.is_none() {
        return Err(WindowsDeliveryArchiveError::InvalidArchive);
    }
    Ok(overrides)
}

fn validate_tar_header(header: &[u8; 512]) -> Result<(), WindowsDeliveryArchiveError> {
    if &header[257..263] != b"ustar\0" || &header[263..265] != b"00" {
        return Err(WindowsDeliveryArchiveError::InvalidArchive);
    }
    let expected = parse_tar_octal(&header[148..156])?;
    let mut actual = 0_u64;
    for (index, byte) in header.iter().enumerate() {
        actual = actual
            .checked_add(u64::from(if (148..156).contains(&index) {
                b' '
            } else {
                *byte
            }))
            .ok_or(WindowsDeliveryArchiveError::InvalidArchive)?;
    }
    if actual != expected {
        return Err(WindowsDeliveryArchiveError::InvalidArchive);
    }
    Ok(())
}

fn tar_header_path(header: &[u8; 512]) -> Result<String, WindowsDeliveryArchiveError> {
    let name = tar_text(&header[..100])?;
    let prefix = tar_text(&header[345..500])?;
    if name.is_empty() {
        return Err(WindowsDeliveryArchiveError::InvalidArchive);
    }
    if prefix.is_empty() {
        Ok(name)
    } else {
        Ok(format!("{prefix}/{name}"))
    }
}

fn tar_text(field: &[u8]) -> Result<String, WindowsDeliveryArchiveError> {
    let end = field
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(field.len());
    if field[end..].iter().any(|byte| *byte != 0) {
        return Err(WindowsDeliveryArchiveError::InvalidArchive);
    }
    std::str::from_utf8(&field[..end])
        .map(str::to_owned)
        .map_err(|_| WindowsDeliveryArchiveError::InvalidArchive)
}

fn parse_tar_octal(field: &[u8]) -> Result<u64, WindowsDeliveryArchiveError> {
    let start = field
        .iter()
        .position(|byte| !matches!(*byte, 0 | b' '))
        .unwrap_or(field.len());
    let end = field
        .iter()
        .rposition(|byte| !matches!(*byte, 0 | b' '))
        .map_or(start, |index| index + 1);
    if start == end {
        return Ok(0);
    }
    let digits = &field[start..end];
    if !digits.iter().all(|byte| matches!(*byte, b'0'..=b'7')) {
        return Err(WindowsDeliveryArchiveError::InvalidArchive);
    }
    let mut value = 0_u64;
    for digit in digits {
        value = value
            .checked_mul(8)
            .and_then(|current| current.checked_add(u64::from(*digit - b'0')))
            .ok_or(WindowsDeliveryArchiveError::InvalidArchive)?;
    }
    Ok(value)
}

fn round_up_tar_block(value: u64) -> Result<u64, WindowsDeliveryArchiveError> {
    let remainder = value % TAR_BLOCK_BYTES;
    if remainder == 0 {
        Ok(value)
    } else {
        checked_add(value, TAR_BLOCK_BYTES - remainder)
    }
}

fn ensure_zero_tail(
    file: &mut File,
    mut offset: u64,
    archive_len: u64,
) -> Result<(), WindowsDeliveryArchiveError> {
    let mut buffer = [0_u8; 8192];
    file.seek(SeekFrom::Start(offset))
        .map_err(|_| WindowsDeliveryArchiveError::Io)?;
    while offset < archive_len {
        let remaining = archive_len - offset;
        let read_len = usize::try_from(remaining.min(buffer.len() as u64))
            .map_err(|_| WindowsDeliveryArchiveError::InvalidArchive)?;
        file.read_exact(&mut buffer[..read_len])
            .map_err(|_| WindowsDeliveryArchiveError::Io)?;
        if buffer[..read_len].iter().any(|byte| *byte != 0) {
            return Err(WindowsDeliveryArchiveError::InvalidArchive);
        }
        offset = checked_add(
            offset,
            u64::try_from(read_len).map_err(|_| WindowsDeliveryArchiveError::InvalidArchive)?,
        )?;
    }
    Ok(())
}

struct RegionIdentity {
    sha256: String,
    crc32: u32,
}

fn hash_and_crc_region(
    file: &mut File,
    offset: u64,
    size: u64,
) -> Result<RegionIdentity, WindowsDeliveryArchiveError> {
    file.seek(SeekFrom::Start(offset))
        .map_err(|_| WindowsDeliveryArchiveError::Io)?;
    let mut remaining = size;
    let mut sha256 = Sha256::new();
    let mut crc32 = 0xffff_ffff_u32;
    let mut buffer = [0_u8; 64 * 1024];
    while remaining > 0 {
        let read_len = usize::try_from(remaining.min(buffer.len() as u64))
            .map_err(|_| WindowsDeliveryArchiveError::InvalidArchive)?;
        file.read_exact(&mut buffer[..read_len])
            .map_err(|_| WindowsDeliveryArchiveError::Io)?;
        sha256.update(&buffer[..read_len]);
        for byte in &buffer[..read_len] {
            crc32 ^= u32::from(*byte);
            for _ in 0..8 {
                let mask = (crc32 & 1).wrapping_neg();
                crc32 = (crc32 >> 1) ^ (0xedb8_8320 & mask);
            }
        }
        remaining -=
            u64::try_from(read_len).map_err(|_| WindowsDeliveryArchiveError::InvalidArchive)?;
    }
    Ok(RegionIdentity {
        sha256: encode_lower_hex(sha256.finalize().as_slice()),
        crc32: !crc32,
    })
}

fn copy_region(
    file: &mut File,
    offset: u64,
    size: u64,
    writer: &mut dyn Write,
) -> Result<(), WindowsDeliveryArchiveError> {
    file.seek(SeekFrom::Start(offset))
        .map_err(|_| WindowsDeliveryArchiveError::Io)?;
    let mut remaining = size;
    let mut buffer = [0_u8; 64 * 1024];
    while remaining > 0 {
        let read_len = usize::try_from(remaining.min(buffer.len() as u64))
            .map_err(|_| WindowsDeliveryArchiveError::InvalidArchive)?;
        file.read_exact(&mut buffer[..read_len])
            .map_err(|_| WindowsDeliveryArchiveError::Io)?;
        writer
            .write_all(&buffer[..read_len])
            .map_err(|_| WindowsDeliveryArchiveError::Io)?;
        remaining -=
            u64::try_from(read_len).map_err(|_| WindowsDeliveryArchiveError::InvalidArchive)?;
    }
    Ok(())
}

fn read_exact_at<const N: usize>(
    file: &mut File,
    offset: u64,
) -> Result<[u8; N], WindowsDeliveryArchiveError> {
    let mut buffer = [0_u8; N];
    file.seek(SeekFrom::Start(offset))
        .map_err(|_| WindowsDeliveryArchiveError::Io)?;
    file.read_exact(&mut buffer)
        .map_err(|_| WindowsDeliveryArchiveError::Io)?;
    Ok(buffer)
}

fn read_bytes_at(
    file: &mut File,
    offset: u64,
    length: usize,
) -> Result<Vec<u8>, WindowsDeliveryArchiveError> {
    let mut buffer = vec![0_u8; length];
    file.seek(SeekFrom::Start(offset))
        .map_err(|_| WindowsDeliveryArchiveError::Io)?;
    file.read_exact(&mut buffer)
        .map_err(|_| WindowsDeliveryArchiveError::Io)?;
    Ok(buffer)
}

fn utf8_archive_path(bytes: Vec<u8>) -> Result<String, WindowsDeliveryArchiveError> {
    let value =
        String::from_utf8(bytes).map_err(|_| WindowsDeliveryArchiveError::InvalidArchive)?;
    if value.is_empty() || value.contains('\0') {
        return Err(WindowsDeliveryArchiveError::InvalidArchive);
    }
    Ok(value)
}

fn le_u16(bytes: &[u8], offset: usize) -> Result<u16, WindowsDeliveryArchiveError> {
    let value = bytes
        .get(offset..offset + 2)
        .ok_or(WindowsDeliveryArchiveError::InvalidArchive)?;
    Ok(u16::from_le_bytes([value[0], value[1]]))
}

fn le_u32(bytes: &[u8], offset: usize) -> Result<u32, WindowsDeliveryArchiveError> {
    let value = bytes
        .get(offset..offset + 4)
        .ok_or(WindowsDeliveryArchiveError::InvalidArchive)?;
    Ok(u32::from_le_bytes([value[0], value[1], value[2], value[3]]))
}

fn checked_add(left: u64, right: u64) -> Result<u64, WindowsDeliveryArchiveError> {
    left.checked_add(right)
        .ok_or(WindowsDeliveryArchiveError::InvalidArchive)
}

fn encode_lower_hex(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(char::from(LOWER_HEX[usize::from(byte >> 4)]));
        output.push(char::from(LOWER_HEX[usize::from(byte & 0x0f)]));
    }
    output
}

fn metadata_is_link_or_reparse(metadata: &Metadata) -> bool {
    if metadata.file_type().is_symlink() {
        return true;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
    }
    #[cfg(not(windows))]
    {
        let _ = FILE_ATTRIBUTE_REPARSE_POINT;
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io;
    use std::sync::atomic::{AtomicU64, Ordering};

    type TestResult = Result<(), Box<dyn std::error::Error>>;
    static TEST_COUNTER: AtomicU64 = AtomicU64::new(1);

    struct TestDirectory(PathBuf);

    impl TestDirectory {
        fn create(label: &str) -> Result<Self, io::Error> {
            let sequence = TEST_COUNTER.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "profile-bridge-delivery-archive-{label}-{}-{sequence}",
                std::process::id()
            ));
            fs::create_dir(&path)?;
            Ok(Self(path))
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    struct ZipFixture<'a> {
        name: &'a str,
        content: &'a [u8],
        mode: u32,
    }

    fn write_stored_zip(path: &Path, files: &[ZipFixture<'_>]) -> Result<(), io::Error> {
        let mut local = Vec::new();
        let mut central = Vec::new();
        for file in files {
            let name = file.name.as_bytes();
            let local_offset = u32::try_from(local.len()).map_err(io::Error::other)?;
            let size = u32::try_from(file.content.len()).map_err(io::Error::other)?;
            let crc = crc32_bytes(file.content);
            push_u32(&mut local, ZIP_LOCAL_FILE_HEADER);
            push_u16(&mut local, 20);
            push_u16(&mut local, 0);
            push_u16(&mut local, ZIP_STORED_METHOD);
            push_u16(&mut local, 0);
            push_u16(&mut local, 33);
            push_u32(&mut local, crc);
            push_u32(&mut local, size);
            push_u32(&mut local, size);
            push_u16(
                &mut local,
                u16::try_from(name.len()).map_err(io::Error::other)?,
            );
            push_u16(&mut local, 0);
            local.extend_from_slice(name);
            local.extend_from_slice(file.content);

            push_u32(&mut central, ZIP_CENTRAL_DIRECTORY_HEADER);
            push_u16(&mut central, (u16::from(ZIP_UNIX_SYSTEM) << 8) | 20);
            push_u16(&mut central, 20);
            push_u16(&mut central, 0);
            push_u16(&mut central, ZIP_STORED_METHOD);
            push_u16(&mut central, 0);
            push_u16(&mut central, 33);
            push_u32(&mut central, crc);
            push_u32(&mut central, size);
            push_u32(&mut central, size);
            push_u16(
                &mut central,
                u16::try_from(name.len()).map_err(io::Error::other)?,
            );
            push_u16(&mut central, 0);
            push_u16(&mut central, 0);
            push_u16(&mut central, 0);
            push_u16(&mut central, 0);
            push_u32(&mut central, file.mode << 16);
            push_u32(&mut central, local_offset);
            central.extend_from_slice(name);
        }
        let central_offset = u32::try_from(local.len()).map_err(io::Error::other)?;
        let central_size = u32::try_from(central.len()).map_err(io::Error::other)?;
        let count = u16::try_from(files.len()).map_err(io::Error::other)?;
        local.extend_from_slice(&central);
        push_u32(&mut local, ZIP_END_OF_CENTRAL_DIRECTORY);
        push_u16(&mut local, 0);
        push_u16(&mut local, 0);
        push_u16(&mut local, count);
        push_u16(&mut local, count);
        push_u32(&mut local, central_size);
        push_u32(&mut local, central_offset);
        push_u16(&mut local, 0);
        fs::write(path, local)
    }

    fn push_u16(output: &mut Vec<u8>, value: u16) {
        output.extend_from_slice(&value.to_le_bytes());
    }

    fn push_u32(output: &mut Vec<u8>, value: u32) {
        output.extend_from_slice(&value.to_le_bytes());
    }

    fn crc32_bytes(bytes: &[u8]) -> u32 {
        let mut crc = 0xffff_ffff_u32;
        for byte in bytes {
            crc ^= u32::from(*byte);
            for _ in 0..8 {
                let mask = (crc & 1).wrapping_neg();
                crc = (crc >> 1) ^ (0xedb8_8320 & mask);
            }
        }
        !crc
    }

    fn tar_header(name: &str, size: u64, type_flag: u8) -> Result<[u8; 512], io::Error> {
        if name.len() > 100 {
            return Err(io::Error::other("fixture tar name too long"));
        }
        let mut header = [0_u8; 512];
        header[..name.len()].copy_from_slice(name.as_bytes());
        write_octal(&mut header[100..108], 0o644)?;
        write_octal(&mut header[108..116], 0)?;
        write_octal(&mut header[116..124], 0)?;
        write_octal(&mut header[124..136], size)?;
        write_octal(&mut header[136..148], 0)?;
        header[148..156].fill(b' ');
        header[156] = type_flag;
        header[257..263].copy_from_slice(b"ustar\0");
        header[263..265].copy_from_slice(b"00");
        let checksum: u64 = header.iter().map(|byte| u64::from(*byte)).sum();
        let text = format!("{checksum:06o}");
        header[148..154].copy_from_slice(text.as_bytes());
        header[154] = 0;
        header[155] = b' ';
        Ok(header)
    }

    fn write_octal(field: &mut [u8], value: u64) -> Result<(), io::Error> {
        field.fill(b'0');
        let digits = format!("{value:o}");
        if digits.len() + 1 > field.len() {
            return Err(io::Error::other("fixture tar value too large"));
        }
        let start = field.len() - 1 - digits.len();
        field[start..start + digits.len()].copy_from_slice(digits.as_bytes());
        field[field.len() - 1] = 0;
        Ok(())
    }

    fn append_tar_member(
        output: &mut Vec<u8>,
        name: &str,
        content: &[u8],
        type_flag: u8,
    ) -> Result<(), io::Error> {
        output.extend_from_slice(&tar_header(
            name,
            u64::try_from(content.len()).map_err(io::Error::other)?,
            type_flag,
        )?);
        output.extend_from_slice(content);
        while !output
            .len()
            .is_multiple_of(usize::try_from(TAR_BLOCK_BYTES).map_err(io::Error::other)?)
        {
            output.push(0);
        }
        Ok(())
    }

    fn pax_record(key: &str, value: &str) -> Vec<u8> {
        let body = format!("{key}={value}\n");
        let mut length = body.len() + 2;
        loop {
            let record = format!("{length} {body}");
            if record.len() == length {
                return record.into_bytes();
            }
            length = record.len();
        }
    }

    fn write_runtime_tar(path: &Path, include_special: bool) -> Result<String, io::Error> {
        let mut archive = Vec::new();
        append_tar_member(
            &mut archive,
            "runtime-manifest.json",
            b"{\"release_id\":\"fixture\"}\n",
            TAR_REGULAR_FILE,
        )?;
        append_tar_member(
            &mut archive,
            "python/python.exe",
            b"python-runtime",
            TAR_REGULAR_FILE,
        )?;
        let long_path = format!("python/site-packages/{}/module.py", "nested".repeat(20));
        let pax = pax_record("path", &long_path);
        append_tar_member(&mut archive, "PaxHeaders/module.py", &pax, TAR_PAX_HEADER)?;
        append_tar_member(
            &mut archive,
            "module.py",
            b"print('fixture')\n",
            TAR_REGULAR_FILE,
        )?;
        if include_special {
            append_tar_member(&mut archive, "runtime/link", &[], b'2')?;
        }
        archive.extend_from_slice(&[0_u8; 1024]);
        fs::write(path, archive)?;
        Ok(long_path)
    }

    #[test]
    fn canonical_bridge_zip_and_runtime_pax_tar_are_streamed_exactly() -> TestResult {
        let directory = TestDirectory::create("canonical")?;
        let bridge = directory.0.join("profile-bridge.zip");
        let runtime = directory.0.join("runtime-bundle.tar");
        write_stored_zip(
            &bridge,
            &[
                ZipFixture {
                    name: "profile-bridge-manifest.json",
                    content: b"{}\n",
                    mode: 0o100644,
                },
                ZipFixture {
                    name: "profile-bridge.exe",
                    content: b"bridge-executable",
                    mode: 0o100755,
                },
            ],
        )?;
        let long_path = write_runtime_tar(&runtime, false)?;

        let mut reader = WindowsDeliveryArchiveReader::new();
        let bridge_entries = reader.entries(DeliveryComponentKind::ProfileBridge, &bridge)?;
        assert_eq!(bridge_entries.len(), 2);
        assert_eq!(
            bridge_entries[0].relative_path(),
            "profile-bridge-manifest.json"
        );
        assert_eq!(bridge_entries[1].relative_path(), "profile-bridge.exe");
        assert_eq!(bridge_entries[1].size_bytes(), 17);
        assert_eq!(bridge_entries[1].sha256(), sha256_hex(b"bridge-executable"));

        let runtime_entries = reader.entries(DeliveryComponentKind::RuntimeBundle, &runtime)?;
        assert_eq!(runtime_entries.len(), 3);
        assert_eq!(runtime_entries[0].relative_path(), "runtime-manifest.json");
        assert_eq!(runtime_entries[1].relative_path(), "python/python.exe");
        assert_eq!(runtime_entries[2].relative_path(), long_path);

        let mut bridge_bytes = Vec::new();
        reader.copy_regular_file(
            DeliveryComponentKind::ProfileBridge,
            &bridge,
            1,
            &mut bridge_bytes,
        )?;
        assert_eq!(bridge_bytes, b"bridge-executable");
        let mut runtime_bytes = Vec::new();
        reader.copy_regular_file(
            DeliveryComponentKind::RuntimeBundle,
            &runtime,
            2,
            &mut runtime_bytes,
        )?;
        assert_eq!(runtime_bytes, b"print('fixture')\n");
        Ok(())
    }

    #[test]
    fn special_tar_members_are_surfaced_for_staging_rejection() -> TestResult {
        let directory = TestDirectory::create("special")?;
        let runtime = directory.0.join("runtime-bundle.tar");
        write_runtime_tar(&runtime, true)?;
        let mut reader = WindowsDeliveryArchiveReader::new();
        let entries = reader.entries(DeliveryComponentKind::RuntimeBundle, &runtime)?;
        let special = entries
            .iter()
            .find(|entry| entry.relative_path() == "runtime/link")
            .ok_or_else(|| io::Error::other("special fixture entry missing"))?;
        assert_eq!(special.kind(), DeliveryArchiveEntryKind::LinkOrSpecial);
        Ok(())
    }

    #[test]
    fn noncanonical_zip_compression_and_corrupt_tar_checksum_fail_closed() -> TestResult {
        let directory = TestDirectory::create("corrupt")?;
        let bridge = directory.0.join("profile-bridge.zip");
        write_stored_zip(
            &bridge,
            &[ZipFixture {
                name: "profile-bridge.exe",
                content: b"bridge",
                mode: 0o100755,
            }],
        )?;
        let mut bridge_bytes = fs::read(&bridge)?;
        bridge_bytes[8] = 8;
        fs::write(&bridge, bridge_bytes)?;
        let mut reader = WindowsDeliveryArchiveReader::new();
        assert_eq!(
            reader.entries(DeliveryComponentKind::ProfileBridge, &bridge),
            Err(WindowsDeliveryArchiveError::InvalidArchive)
        );

        let runtime = directory.0.join("runtime-bundle.tar");
        write_runtime_tar(&runtime, false)?;
        let mut runtime_bytes = fs::read(&runtime)?;
        runtime_bytes[0] ^= 1;
        fs::write(&runtime, runtime_bytes)?;
        assert_eq!(
            reader.entries(DeliveryComponentKind::RuntimeBundle, &runtime),
            Err(WindowsDeliveryArchiveError::InvalidArchive)
        );
        Ok(())
    }

    #[test]
    fn archive_cache_is_component_and_path_bound() -> TestResult {
        let directory = TestDirectory::create("cache")?;
        let first = directory.0.join("first.zip");
        let second = directory.0.join("second.zip");
        write_stored_zip(
            &first,
            &[ZipFixture {
                name: "profile-bridge.exe",
                content: b"first",
                mode: 0o100755,
            }],
        )?;
        write_stored_zip(
            &second,
            &[ZipFixture {
                name: "profile-bridge.exe",
                content: b"second",
                mode: 0o100755,
            }],
        )?;
        let mut reader = WindowsDeliveryArchiveReader::new();
        reader.entries(DeliveryComponentKind::ProfileBridge, &first)?;
        assert_eq!(
            reader.copy_regular_file(
                DeliveryComponentKind::ProfileBridge,
                &second,
                0,
                &mut Vec::new()
            ),
            Err(WindowsDeliveryArchiveError::CacheMiss)
        );
        Ok(())
    }

    fn sha256_hex(bytes: &[u8]) -> String {
        encode_lower_hex(Sha256::digest(bytes).as_slice())
    }
}
