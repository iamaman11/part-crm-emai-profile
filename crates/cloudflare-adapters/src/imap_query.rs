use crate::cloud_mailbox_secrets::ImapCredential;
use crate::imap_session::{
    ImapCommandResponse, ImapSession, ImapTaggedStatus, ImapTransportError, push_imap_quoted,
};
use application_ports::query::{QueryCursor, QueryPage, QueryPortError, QueryPortErrorClass};
use application_ports::query_mail_provider::{
    MAX_MAIL_BODY_BYTES, MailMessageBody, MailMessageSummary, MailboxMessageReference,
    SearchClientMailboxMessagesRequest,
};
use mailbox_domain::MailboxBinding;
use profile_platform_primitives::UnixMillis;

const IMAP_CURSOR_PREFIX: &str = "imap:";
const IMAP_REFERENCE_PREFIX: &str = "imap:";
const MAX_IMAP_QUERY_PAGE_SIZE: u16 = 25;
const IMAP_SEARCH_WINDOW_UIDS: u64 = 500;
const MAX_IMAP_SEARCH_WINDOWS: usize = 8;
const MAX_IMAP_CONTROL_RESPONSE_BYTES: usize = 64 * 1024;
const MAX_IMAP_METADATA_RESPONSE_BYTES: usize = 64 * 1024;
const MAX_IMAP_BODY_RESPONSE_BYTES: usize = 2 * 1024 * 1024;
const MAX_HEADER_BLOCK_BYTES: usize = 64 * 1024;
const MAX_HEADER_VALUE_BYTES: usize = 8 * 1024;
const MAX_MIME_PARTS: usize = 256;
const MAX_MIME_DEPTH: usize = 16;
const MAX_BOUNDARY_BYTES: usize = 200;
const MAX_MESSAGE_BYTES_FOR_BODY_READ: u64 = 2 * 1024 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ImapMailboxSnapshot {
    uid_validity: u64,
    uid_next: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ImapMessageMetadata {
    uid: u64,
    internal_date: UnixMillis,
    subject: Option<String>,
    sender: Option<String>,
    content_type: Option<String>,
    transfer_encoding: Option<String>,
    content_disposition: Option<String>,
    rfc822_size: Option<u64>,
}

pub(crate) async fn search_imap_messages(
    binding: &MailboxBinding,
    request: &SearchClientMailboxMessagesRequest,
    credential: &ImapCredential,
) -> Result<QueryPage<MailMessageSummary>, QueryPortError> {
    let page_size = usize::from(request.page().limit().value().min(MAX_IMAP_QUERY_PAGE_SIZE));
    let mut session = ImapSession::connect(credential)
        .await
        .map_err(map_transport_error)?;
    let snapshot = examine_inbox(&mut session).await?;
    let mut before_uid = request
        .page()
        .cursor()
        .map(|cursor| parse_imap_cursor(cursor, snapshot.uid_validity))
        .transpose()?
        .unwrap_or(snapshot.uid_next);
    if before_uid == 0 || before_uid > snapshot.uid_next {
        return Err(invalid_cursor());
    }

    let mut selected = Vec::with_capacity(page_size);
    let mut continuation_before_uid = before_uid;
    let mut exhausted = before_uid <= 1;
    for _ in 0..MAX_IMAP_SEARCH_WINDOWS {
        if before_uid <= 1 || selected.len() >= page_size {
            exhausted = before_uid <= 1;
            break;
        }
        let end = before_uid - 1;
        let start = end.saturating_sub(IMAP_SEARCH_WINDOW_UIDS - 1).max(1);
        let mut uids = search_uid_window(&mut session, start, end, request).await?;
        uids.sort_unstable();
        uids.dedup();
        uids.retain(|uid| *uid >= start && *uid <= end);
        uids.reverse();
        for uid in uids {
            if selected.len() == page_size {
                break;
            }
            selected.push(uid);
        }
        continuation_before_uid = start;
        before_uid = start;
        exhausted = start == 1;
    }

    let selected_full = selected.len() == page_size;
    let next_before_uid = if selected_full {
        selected.last().copied()
    } else if exhausted {
        None
    } else {
        Some(continuation_before_uid)
    };

    let mut items = Vec::with_capacity(selected.len());
    for uid in &selected {
        let Some(metadata) = fetch_metadata(&mut session, *uid).await? else {
            continue;
        };
        items.push(summary_from_imap(
            binding,
            snapshot.uid_validity,
            &metadata,
        )?);
    }

    let next_cursor = next_before_uid
        .filter(|before_uid| *before_uid > 1)
        .map(|before_uid| imap_query_cursor(snapshot.uid_validity, before_uid))
        .transpose()?;
    Ok(QueryPage::new(items, next_cursor))
}

pub(crate) async fn get_imap_message(
    binding: &MailboxBinding,
    provider_reference: &str,
    credential: &ImapCredential,
) -> Result<Option<MailMessageBody>, QueryPortError> {
    let (reference_uid_validity, uid) = parse_imap_reference(provider_reference)?;
    let mut session = ImapSession::connect(credential)
        .await
        .map_err(map_transport_error)?;
    let snapshot = examine_inbox(&mut session).await?;
    if snapshot.uid_validity != reference_uid_validity || uid == 0 || uid >= snapshot.uid_next {
        return Ok(None);
    }
    let Some(metadata) = fetch_metadata(&mut session, uid).await? else {
        return Ok(None);
    };
    if metadata
        .rfc822_size
        .is_some_and(|size| size > MAX_MESSAGE_BYTES_FOR_BODY_READ)
    {
        return Err(integrity_failure());
    }
    let body = fetch_body_text(&mut session, uid).await?;
    let summary = summary_from_imap(binding, snapshot.uid_validity, &metadata)?;
    let (text_body, html_body) = extract_imap_bodies(&metadata, &body)?;
    MailMessageBody::new(summary, text_body, html_body)
        .map(Some)
        .map_err(|_| integrity_failure())
}

async fn examine_inbox(session: &mut ImapSession) -> Result<ImapMailboxSnapshot, QueryPortError> {
    let response = session
        .execute("EXAMINE INBOX", MAX_IMAP_CONTROL_RESPONSE_BYTES)
        .await
        .map_err(map_transport_error)?;
    require_ok(&response)?;
    let text = response.text_lossy();
    let uid_validity = parse_bracket_u64(&text, "UIDVALIDITY").ok_or_else(integrity_failure)?;
    let uid_next = parse_bracket_u64(&text, "UIDNEXT").ok_or_else(integrity_failure)?;
    if uid_validity == 0 || uid_next == 0 {
        return Err(integrity_failure());
    }
    Ok(ImapMailboxSnapshot {
        uid_validity,
        uid_next,
    })
}

async fn search_uid_window(
    session: &mut ImapSession,
    start: u64,
    end: u64,
    request: &SearchClientMailboxMessagesRequest,
) -> Result<Vec<u64>, QueryPortError> {
    if start == 0 || end < start {
        return Err(integrity_failure());
    }
    let mut command = String::from("UID SEARCH ");
    if request.term().is_some_and(|term| !term.as_str().is_ascii()) {
        command.push_str("CHARSET UTF-8 ");
    }
    command.push_str("UID ");
    command.push_str(&start.to_string());
    command.push(':');
    command.push_str(&end.to_string());
    if let Some(term) = request.term() {
        command.push_str(" TEXT ");
        push_imap_quoted(&mut command, term.as_str());
    }
    let response = session
        .execute(&command, MAX_IMAP_CONTROL_RESPONSE_BYTES)
        .await
        .map_err(map_transport_error)?;
    require_ok(&response)?;
    parse_search_uids(&response.text_lossy())
}

async fn fetch_metadata(
    session: &mut ImapSession,
    uid: u64,
) -> Result<Option<ImapMessageMetadata>, QueryPortError> {
    let command = format!(
        "UID FETCH {uid} (UID INTERNALDATE RFC822.SIZE BODY.PEEK[HEADER.FIELDS (SUBJECT FROM CONTENT-TYPE CONTENT-TRANSFER-ENCODING CONTENT-DISPOSITION)])"
    );
    let response = session
        .execute(&command, MAX_IMAP_METADATA_RESPONSE_BYTES)
        .await
        .map_err(map_transport_error)?;
    require_ok(&response)?;
    if !response
        .bytes()
        .windows(7)
        .any(|window| window == b" FETCH ")
    {
        return Ok(None);
    }
    let response_text = response.text_lossy();
    let metadata_prefix = response_text
        .split('{')
        .next()
        .ok_or_else(integrity_failure)?;
    let parsed_uid = parse_fetch_atom_u64(metadata_prefix, "UID").ok_or_else(integrity_failure)?;
    if parsed_uid != uid {
        return Err(integrity_failure());
    }
    let internal_date = parse_internal_date_from_fetch(metadata_prefix)?;
    let rfc822_size = parse_fetch_atom_u64(metadata_prefix, "RFC822.SIZE");
    let header_bytes = extract_first_literal(response.bytes())?.unwrap_or_default();
    if header_bytes.len() > MAX_HEADER_BLOCK_BYTES {
        return Err(integrity_failure());
    }
    let headers = parse_header_block(header_bytes)?;
    Ok(Some(ImapMessageMetadata {
        uid,
        internal_date,
        subject: bounded_header_value(&headers, "subject")?,
        sender: bounded_header_value(&headers, "from")?,
        content_type: bounded_header_value(&headers, "content-type")?,
        transfer_encoding: bounded_header_value(&headers, "content-transfer-encoding")?,
        content_disposition: bounded_header_value(&headers, "content-disposition")?,
        rfc822_size,
    }))
}

async fn fetch_body_text(session: &mut ImapSession, uid: u64) -> Result<Vec<u8>, QueryPortError> {
    let command = format!("UID FETCH {uid} (BODY.PEEK[TEXT])");
    let response = session
        .execute(&command, MAX_IMAP_BODY_RESPONSE_BYTES)
        .await
        .map_err(map_transport_error)?;
    require_ok(&response)?;
    Ok(extract_first_literal(response.bytes())?
        .map(<[u8]>::to_vec)
        .unwrap_or_default())
}

fn summary_from_imap(
    binding: &MailboxBinding,
    uid_validity: u64,
    metadata: &ImapMessageMetadata,
) -> Result<MailMessageSummary, QueryPortError> {
    let reference = MailboxMessageReference::new(
        binding.binding_id().clone(),
        format!("{IMAP_REFERENCE_PREFIX}{}:{}", uid_validity, metadata.uid),
    )
    .map_err(|_| integrity_failure())?;
    Ok(MailMessageSummary::new(
        reference,
        metadata.subject.clone(),
        metadata.sender.clone(),
        metadata.internal_date,
    ))
}

fn extract_imap_bodies(
    metadata: &ImapMessageMetadata,
    body: &[u8],
) -> Result<(Option<String>, Option<String>), QueryPortError> {
    if body.len() > MAX_IMAP_BODY_RESPONSE_BYTES {
        return Err(integrity_failure());
    }
    let headers = PartHeaders {
        content_type: metadata
            .content_type
            .clone()
            .unwrap_or_else(|| "text/plain; charset=us-ascii".to_owned()),
        transfer_encoding: metadata
            .transfer_encoding
            .clone()
            .unwrap_or_else(|| "7bit".to_owned()),
        content_disposition: metadata.content_disposition.clone(),
    };
    let mut output = BodyAccumulator::default();
    let mut visited = 0_usize;
    walk_mime_part(&headers, body, 0, &mut visited, &mut output)?;
    Ok((
        (!output.text.is_empty()).then_some(output.text),
        (!output.html.is_empty()).then_some(output.html),
    ))
}

#[derive(Clone, Debug)]
struct PartHeaders {
    content_type: String,
    transfer_encoding: String,
    content_disposition: Option<String>,
}

#[derive(Default)]
struct BodyAccumulator {
    text: String,
    html: String,
}

fn walk_mime_part(
    headers: &PartHeaders,
    body: &[u8],
    depth: usize,
    visited: &mut usize,
    output: &mut BodyAccumulator,
) -> Result<(), QueryPortError> {
    *visited = visited.checked_add(1).ok_or_else(integrity_failure)?;
    if *visited > MAX_MIME_PARTS || depth > MAX_MIME_DEPTH {
        return Err(integrity_failure());
    }
    if headers
        .content_disposition
        .as_deref()
        .is_some_and(is_attachment_disposition)
    {
        return Ok(());
    }
    let parsed_type = parse_content_type(&headers.content_type)?;
    if parsed_type.media_type.starts_with("multipart/") {
        let boundary = parsed_type.boundary.ok_or_else(integrity_failure)?;
        for section in multipart_sections(body, boundary.as_bytes())? {
            let (header_block, part_body) = split_headers_body(section)?;
            let parsed = parse_header_block(header_block)?;
            let part_headers = PartHeaders {
                content_type: bounded_header_value(&parsed, "content-type")?
                    .unwrap_or_else(|| "text/plain; charset=us-ascii".to_owned()),
                transfer_encoding: bounded_header_value(&parsed, "content-transfer-encoding")?
                    .unwrap_or_else(|| "7bit".to_owned()),
                content_disposition: bounded_header_value(&parsed, "content-disposition")?,
            };
            walk_mime_part(&part_headers, part_body, depth + 1, visited, output)?;
        }
        return Ok(());
    }
    let target = if parsed_type.media_type == "text/plain" {
        Some(&mut output.text)
    } else if parsed_type.media_type == "text/html" {
        Some(&mut output.html)
    } else {
        None
    };
    let Some(target) = target else {
        return Ok(());
    };
    let decoded = decode_transfer_encoding(body, &headers.transfer_encoding)?;
    let Some(decoded) = decode_charset(&decoded, parsed_type.charset.as_deref())? else {
        return Ok(());
    };
    if !target.is_empty() {
        target.push('\n');
    }
    target.push_str(&decoded);
    if output.text.len().saturating_add(output.html.len()) > MAX_MAIL_BODY_BYTES {
        return Err(integrity_failure());
    }
    Ok(())
}

#[derive(Debug)]
struct ParsedContentType {
    media_type: String,
    boundary: Option<String>,
    charset: Option<String>,
}

fn parse_content_type(value: &str) -> Result<ParsedContentType, QueryPortError> {
    let segments = split_header_parameters(value)?;
    let media_type = segments
        .first()
        .map_or("", String::as_str)
        .trim()
        .to_ascii_lowercase();
    if media_type.is_empty() || !media_type.contains('/') {
        return Err(integrity_failure());
    }
    let mut boundary = None;
    let mut charset = None;
    for parameter in segments.iter().skip(1) {
        let Some((name, value)) = parameter.split_once('=') else {
            continue;
        };
        let name = name.trim().to_ascii_lowercase();
        let value = unquote_parameter(value.trim())?;
        if name == "boundary" {
            if value.is_empty()
                || value.len() > MAX_BOUNDARY_BYTES
                || value
                    .bytes()
                    .any(|byte| matches!(byte, b'\r' | b'\n' | b'\0'))
            {
                return Err(integrity_failure());
            }
            boundary = Some(value);
        } else if name == "charset" {
            if value.len() > 64 {
                return Err(integrity_failure());
            }
            charset = Some(value.to_ascii_lowercase());
        }
    }
    Ok(ParsedContentType {
        media_type,
        boundary,
        charset,
    })
}

fn split_header_parameters(value: &str) -> Result<Vec<String>, QueryPortError> {
    if value
        .bytes()
        .any(|byte| matches!(byte, b'\r' | b'\n' | b'\0'))
    {
        return Err(integrity_failure());
    }
    let mut segments = Vec::new();
    let mut current = String::new();
    let mut quoted = false;
    let mut escaped = false;
    for character in value.chars() {
        if escaped {
            current.push(character);
            escaped = false;
            continue;
        }
        if quoted && character == '\\' {
            current.push(character);
            escaped = true;
            continue;
        }
        if character == '"' {
            quoted = !quoted;
            current.push(character);
            continue;
        }
        if character == ';' && !quoted {
            segments.push(current.trim().to_owned());
            current.clear();
            continue;
        }
        current.push(character);
    }
    if quoted || escaped {
        return Err(integrity_failure());
    }
    segments.push(current.trim().to_owned());
    if segments.len() > 32 {
        return Err(integrity_failure());
    }
    Ok(segments)
}

fn unquote_parameter(value: &str) -> Result<String, QueryPortError> {
    if !value.starts_with('"') {
        return Ok(value.trim().to_owned());
    }
    if !value.ends_with('"') || value.len() < 2 {
        return Err(integrity_failure());
    }
    let mut output = String::new();
    let mut escaped = false;
    for character in value[1..value.len() - 1].chars() {
        if escaped {
            output.push(character);
            escaped = false;
        } else if character == '\\' {
            escaped = true;
        } else {
            output.push(character);
        }
    }
    if escaped {
        return Err(integrity_failure());
    }
    Ok(output)
}

fn multipart_sections<'a>(
    body: &'a [u8],
    boundary: &[u8],
) -> Result<Vec<&'a [u8]>, QueryPortError> {
    if boundary.is_empty() || boundary.len() > MAX_BOUNDARY_BYTES {
        return Err(integrity_failure());
    }
    let mut delimiter = Vec::with_capacity(boundary.len() + 2);
    delimiter.extend_from_slice(b"--");
    delimiter.extend_from_slice(boundary);
    let mut delimiters = Vec::new();
    let mut cursor = 0_usize;
    while cursor < body.len() {
        let line_start = cursor == 0 || body.get(cursor - 1) == Some(&b'\n');
        if !line_start || !body[cursor..].starts_with(&delimiter) {
            cursor += 1;
            continue;
        }
        let after = cursor + delimiter.len();
        let suffix = body.get(after).copied();
        if !matches!(suffix, None | Some(b'-' | b'\r' | b'\n' | b' ' | b'\t')) {
            cursor += 1;
            continue;
        }
        let closing = body.get(after..after + 2) == Some(b"--");
        let line_end = body[cursor..]
            .iter()
            .position(|byte| *byte == b'\n')
            .map(|offset| cursor + offset)
            .unwrap_or(body.len());
        let content_start = if line_end < body.len() {
            line_end + 1
        } else {
            line_end
        };
        delimiters.push((cursor, content_start, closing));
        if delimiters.len() > MAX_MIME_PARTS + 1 {
            return Err(integrity_failure());
        }
        cursor = content_start.max(cursor + 1);
        if closing {
            break;
        }
    }
    if delimiters.len() < 2 || !delimiters.last().is_some_and(|(_, _, closing)| *closing) {
        return Err(integrity_failure());
    }
    let mut sections = Vec::new();
    for pair in delimiters.windows(2) {
        let (_, content_start, closing) = pair[0];
        if closing {
            break;
        }
        let (next_start, _, _) = pair[1];
        let section_end = trim_trailing_line_break(body, content_start, next_start);
        if section_end >= content_start {
            sections.push(&body[content_start..section_end]);
        }
    }
    Ok(sections)
}

fn trim_trailing_line_break(body: &[u8], start: usize, mut end: usize) -> usize {
    while end > start && matches!(body[end - 1], b'\r' | b'\n') {
        end -= 1;
    }
    end
}

fn split_headers_body(section: &[u8]) -> Result<(&[u8], &[u8]), QueryPortError> {
    if let Some(index) = find_bytes(section, b"\r\n\r\n") {
        if index > MAX_HEADER_BLOCK_BYTES {
            return Err(integrity_failure());
        }
        return Ok((&section[..index], &section[index + 4..]));
    }
    if let Some(index) = find_bytes(section, b"\n\n") {
        if index > MAX_HEADER_BLOCK_BYTES {
            return Err(integrity_failure());
        }
        return Ok((&section[..index], &section[index + 2..]));
    }
    Err(integrity_failure())
}

fn parse_header_block(bytes: &[u8]) -> Result<Vec<(String, String)>, QueryPortError> {
    if bytes.len() > MAX_HEADER_BLOCK_BYTES {
        return Err(integrity_failure());
    }
    let text = String::from_utf8_lossy(bytes);
    let normalized = text.replace("\r\n", "\n");
    let mut headers: Vec<(String, String)> = Vec::new();
    for line in normalized.lines() {
        if line.starts_with(' ') || line.starts_with('\t') {
            let Some((_, value)) = headers.last_mut() else {
                return Err(integrity_failure());
            };
            if !value.is_empty() {
                value.push(' ');
            }
            value.push_str(line.trim());
            continue;
        }
        if line.is_empty() {
            continue;
        }
        let Some((name, value)) = line.split_once(':') else {
            return Err(integrity_failure());
        };
        if name.is_empty()
            || name.len() > 128
            || !name
                .bytes()
                .all(|byte| (33..=126).contains(&byte) && byte != b':')
        {
            return Err(integrity_failure());
        }
        headers.push((name.to_ascii_lowercase(), value.trim().to_owned()));
        if headers.len() > 256 {
            return Err(integrity_failure());
        }
    }
    Ok(headers)
}

fn bounded_header_value(
    headers: &[(String, String)],
    name: &str,
) -> Result<Option<String>, QueryPortError> {
    let value = headers
        .iter()
        .find(|(candidate, _)| candidate == name)
        .map(|(_, value)| value.trim().to_owned());
    if value.as_ref().is_some_and(|value| {
        value.len() > MAX_HEADER_VALUE_BYTES
            || value
                .chars()
                .any(|character| character.is_control() && character != '\t')
    }) {
        return Err(integrity_failure());
    }
    Ok(value.filter(|value| !value.is_empty()))
}

fn decode_transfer_encoding(body: &[u8], encoding: &str) -> Result<Vec<u8>, QueryPortError> {
    let encoding = encoding.trim().to_ascii_lowercase();
    match encoding.as_str() {
        "" | "7bit" | "8bit" | "binary" => {
            if body.len() > MAX_MAIL_BODY_BYTES {
                return Err(integrity_failure());
            }
            Ok(body.to_vec())
        }
        "base64" => decode_base64(body, MAX_MAIL_BODY_BYTES),
        "quoted-printable" => decode_quoted_printable(body, MAX_MAIL_BODY_BYTES),
        _ => Err(integrity_failure()),
    }
}

fn decode_base64(input: &[u8], maximum_bytes: usize) -> Result<Vec<u8>, QueryPortError> {
    let mut clean = Vec::with_capacity(input.len());
    for byte in input.iter().copied() {
        if !byte.is_ascii_whitespace() {
            clean.push(byte);
        }
    }
    let padding = clean.iter().rev().take_while(|byte| **byte == b'=').count();
    if padding > 2 {
        return Err(integrity_failure());
    }
    let unpadded_len = clean.len().saturating_sub(padding);
    if clean[..unpadded_len].contains(&b'=') || unpadded_len % 4 == 1 {
        return Err(integrity_failure());
    }
    if padding > 0 && clean.len() % 4 != 0 {
        return Err(integrity_failure());
    }
    let mut output = Vec::new();
    let mut accumulator = 0_u32;
    let mut bits = 0_u8;
    for byte in clean[..unpadded_len].iter().copied() {
        let value = base64_value(byte).ok_or_else(integrity_failure)?;
        accumulator = (accumulator << 6) | u32::from(value);
        bits += 6;
        while bits >= 8 {
            bits -= 8;
            if output.len() == maximum_bytes {
                return Err(integrity_failure());
            }
            output.push(((accumulator >> bits) & 0xff) as u8);
        }
        if bits == 0 {
            accumulator = 0;
        } else {
            accumulator &= (1_u32 << bits) - 1;
        }
    }
    if bits > 0 && accumulator != 0 {
        return Err(integrity_failure());
    }
    Ok(output)
}

fn base64_value(byte: u8) -> Option<u8> {
    match byte {
        b'A'..=b'Z' => Some(byte - b'A'),
        b'a'..=b'z' => Some(byte - b'a' + 26),
        b'0'..=b'9' => Some(byte - b'0' + 52),
        b'+' | b'-' => Some(62),
        b'/' | b'_' => Some(63),
        _ => None,
    }
}

fn decode_quoted_printable(input: &[u8], maximum_bytes: usize) -> Result<Vec<u8>, QueryPortError> {
    let mut output = Vec::with_capacity(input.len().min(maximum_bytes));
    let mut index = 0_usize;
    while index < input.len() {
        if input[index] != b'=' {
            if output.len() == maximum_bytes {
                return Err(integrity_failure());
            }
            output.push(input[index]);
            index += 1;
            continue;
        }
        if input.get(index + 1..index + 3) == Some(b"\r\n") {
            index += 3;
            continue;
        }
        if input.get(index + 1) == Some(&b'\n') {
            index += 2;
            continue;
        }
        if output.len() == maximum_bytes {
            return Err(integrity_failure());
        }
        let high = *input.get(index + 1).ok_or_else(integrity_failure)?;
        let low = *input.get(index + 2).ok_or_else(integrity_failure)?;
        output.push(
            (hex_value(high).ok_or_else(integrity_failure)? << 4)
                | hex_value(low).ok_or_else(integrity_failure)?,
        );
        index += 3;
    }
    Ok(output)
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn decode_charset(bytes: &[u8], charset: Option<&str>) -> Result<Option<String>, QueryPortError> {
    let charset = charset.unwrap_or("us-ascii").trim().to_ascii_lowercase();
    match charset.as_str() {
        "utf-8" | "utf8" => core::str::from_utf8(bytes)
            .map(str::to_owned)
            .map(Some)
            .map_err(|_| integrity_failure()),
        "us-ascii" | "ascii" => {
            if !bytes.is_ascii() {
                return Ok(None);
            }
            Ok(Some(
                String::from_utf8(bytes.to_vec()).map_err(|_| integrity_failure())?,
            ))
        }
        "iso-8859-1" | "latin1" | "latin-1" => Ok(Some(
            bytes.iter().copied().map(char::from).collect::<String>(),
        )),
        "windows-1252" | "cp1252" => Ok(Some(decode_windows_1252(bytes))),
        _ => {
            if let Ok(value) = core::str::from_utf8(bytes) {
                Ok(Some(value.to_owned()))
            } else {
                Ok(None)
            }
        }
    }
}

fn decode_windows_1252(bytes: &[u8]) -> String {
    bytes
        .iter()
        .copied()
        .map(|byte| match byte {
            0x80 => '\u{20AC}',
            0x82 => '\u{201A}',
            0x83 => '\u{0192}',
            0x84 => '\u{201E}',
            0x85 => '\u{2026}',
            0x86 => '\u{2020}',
            0x87 => '\u{2021}',
            0x88 => '\u{02C6}',
            0x89 => '\u{2030}',
            0x8A => '\u{0160}',
            0x8B => '\u{2039}',
            0x8C => '\u{0152}',
            0x8E => '\u{017D}',
            0x91 => '\u{2018}',
            0x92 => '\u{2019}',
            0x93 => '\u{201C}',
            0x94 => '\u{201D}',
            0x95 => '\u{2022}',
            0x96 => '\u{2013}',
            0x97 => '\u{2014}',
            0x98 => '\u{02DC}',
            0x99 => '\u{2122}',
            0x9A => '\u{0161}',
            0x9B => '\u{203A}',
            0x9C => '\u{0153}',
            0x9E => '\u{017E}',
            0x9F => '\u{0178}',
            _ => char::from(byte),
        })
        .collect()
}

fn is_attachment_disposition(value: &str) -> bool {
    value
        .split(';')
        .next()
        .is_some_and(|value| value.trim().eq_ignore_ascii_case("attachment"))
}

fn parse_search_uids(response: &str) -> Result<Vec<u64>, QueryPortError> {
    let Some(line) = response.lines().find(|line| line.starts_with("* SEARCH")) else {
        return Ok(Vec::new());
    };
    line.strip_prefix("* SEARCH")
        .unwrap_or_default()
        .split_ascii_whitespace()
        .map(|value| {
            value
                .parse::<u64>()
                .ok()
                .filter(|value| *value > 0)
                .ok_or_else(integrity_failure)
        })
        .collect()
}

fn parse_bracket_u64(response: &str, name: &str) -> Option<u64> {
    let marker = format!("[{name} ");
    let start = response.find(&marker)? + marker.len();
    let tail = response.get(start..)?;
    let end = tail.find(']')?;
    tail[..end].trim().parse::<u64>().ok()
}

fn parse_fetch_atom_u64(response: &str, name: &str) -> Option<u64> {
    let marker = format!("{name} ");
    let start = response.find(&marker)? + marker.len();
    let digits = response
        .get(start..)?
        .chars()
        .take_while(char::is_ascii_digit)
        .collect::<String>();
    digits.parse::<u64>().ok()
}

fn parse_internal_date_from_fetch(response: &str) -> Result<UnixMillis, QueryPortError> {
    let marker = "INTERNALDATE \"";
    let start = response.find(marker).ok_or_else(integrity_failure)? + marker.len();
    let tail = response.get(start..).ok_or_else(integrity_failure)?;
    let end = tail.find('"').ok_or_else(integrity_failure)?;
    parse_imap_internal_date(&tail[..end]).ok_or_else(integrity_failure)
}

fn parse_imap_internal_date(value: &str) -> Option<UnixMillis> {
    let mut fields = value.split_ascii_whitespace();
    let date = fields.next()?;
    let time = fields.next()?;
    let zone = fields.next()?;
    if fields.next().is_some() {
        return None;
    }
    let mut date_parts = date.split('-');
    let day = date_parts.next()?.trim().parse::<u32>().ok()?;
    let month = parse_month(date_parts.next()?)?;
    let year = date_parts.next()?.parse::<i32>().ok()?;
    if date_parts.next().is_some() || day == 0 || day > days_in_month(year, month) {
        return None;
    }
    let mut time_parts = time.split(':');
    let hour = time_parts.next()?.parse::<u32>().ok()?;
    let minute = time_parts.next()?.parse::<u32>().ok()?;
    let second = time_parts.next()?.parse::<u32>().ok()?;
    if time_parts.next().is_some() || hour > 23 || minute > 59 || second > 60 {
        return None;
    }
    let zone_bytes = zone.as_bytes();
    if zone_bytes.len() != 5 || !matches!(zone_bytes[0], b'+' | b'-') {
        return None;
    }
    let zone_hours = core::str::from_utf8(&zone_bytes[1..3])
        .ok()?
        .parse::<i64>()
        .ok()?;
    let zone_minutes = core::str::from_utf8(&zone_bytes[3..5])
        .ok()?
        .parse::<i64>()
        .ok()?;
    if zone_hours > 23 || zone_minutes > 59 {
        return None;
    }
    let zone_seconds = zone_hours
        .checked_mul(3_600)?
        .checked_add(zone_minutes.checked_mul(60)?)?;
    let zone_seconds = if zone_bytes[0] == b'-' {
        -zone_seconds
    } else {
        zone_seconds
    };
    let days = days_from_civil(year, month, day)?;
    let local_seconds = days
        .checked_mul(86_400)?
        .checked_add(i64::from(hour).checked_mul(3_600)?)?
        .checked_add(i64::from(minute).checked_mul(60)?)?
        .checked_add(i64::from(second.min(59)))?;
    let utc_seconds = local_seconds.checked_sub(zone_seconds)?;
    let utc_seconds = u64::try_from(utc_seconds).ok()?;
    Some(UnixMillis::new(utc_seconds.checked_mul(1_000)?))
}

fn parse_month(value: &str) -> Option<u32> {
    match value.to_ascii_lowercase().as_str() {
        "jan" => Some(1),
        "feb" => Some(2),
        "mar" => Some(3),
        "apr" => Some(4),
        "may" => Some(5),
        "jun" => Some(6),
        "jul" => Some(7),
        "aug" => Some(8),
        "sep" => Some(9),
        "oct" => Some(10),
        "nov" => Some(11),
        "dec" => Some(12),
        _ => None,
    }
}

fn days_in_month(year: i32, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap_year(year) => 29,
        2 => 28,
        _ => 0,
    }
}

fn is_leap_year(year: i32) -> bool {
    year % 4 == 0 && (year % 100 != 0 || year % 400 == 0)
}

fn days_from_civil(year: i32, month: u32, day: u32) -> Option<i64> {
    if !(1..=12).contains(&month) || day == 0 || day > days_in_month(year, month) {
        return None;
    }
    let adjusted_year = i64::from(year) - i64::from(month <= 2);
    let era = if adjusted_year >= 0 {
        adjusted_year
    } else {
        adjusted_year - 399
    } / 400;
    let year_of_era = adjusted_year - era * 400;
    let adjusted_month = i64::from(month) + if month > 2 { -3 } else { 9 };
    let day_of_year = (153 * adjusted_month + 2) / 5 + i64::from(day) - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    Some(era * 146_097 + day_of_era - 719_468)
}

fn extract_first_literal(response: &[u8]) -> Result<Option<&[u8]>, QueryPortError> {
    let Some(open) = response.iter().position(|byte| *byte == b'{') else {
        return Ok(None);
    };
    let close = response
        .get(open + 1..)
        .and_then(|tail| tail.iter().position(|byte| *byte == b'}'))
        .map(|offset| open + 1 + offset)
        .ok_or_else(integrity_failure)?;
    let length = core::str::from_utf8(&response[open + 1..close])
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .ok_or_else(integrity_failure)?;
    let mut data_start = close + 1;
    if response.get(data_start..data_start + 2) == Some(b"\r\n") {
        data_start += 2;
    } else if response.get(data_start) == Some(&b'\n') {
        data_start += 1;
    } else {
        return Err(integrity_failure());
    }
    let data_end = data_start
        .checked_add(length)
        .ok_or_else(integrity_failure)?;
    let data = response
        .get(data_start..data_end)
        .ok_or_else(integrity_failure)?;
    Ok(Some(data))
}

fn parse_imap_cursor(cursor: &QueryCursor, uid_validity: u64) -> Result<u64, QueryPortError> {
    let value = cursor
        .as_str()
        .strip_prefix(IMAP_CURSOR_PREFIX)
        .ok_or_else(invalid_cursor)?;
    let (cursor_validity, before_uid) = value.split_once(':').ok_or_else(invalid_cursor)?;
    let cursor_validity = cursor_validity
        .parse::<u64>()
        .ok()
        .filter(|value| *value > 0)
        .ok_or_else(invalid_cursor)?;
    let before_uid = before_uid
        .parse::<u64>()
        .ok()
        .filter(|value| *value > 0)
        .ok_or_else(invalid_cursor)?;
    if cursor_validity != uid_validity {
        return Err(invalid_cursor());
    }
    Ok(before_uid)
}

fn imap_query_cursor(uid_validity: u64, before_uid: u64) -> Result<QueryCursor, QueryPortError> {
    QueryCursor::parse(format!("{IMAP_CURSOR_PREFIX}{uid_validity}:{before_uid}"))
        .map_err(|_| integrity_failure())
}

fn parse_imap_reference(reference: &str) -> Result<(u64, u64), QueryPortError> {
    let value = reference
        .strip_prefix(IMAP_REFERENCE_PREFIX)
        .ok_or_else(integrity_failure)?;
    let (uid_validity, uid) = value.split_once(':').ok_or_else(integrity_failure)?;
    let uid_validity = uid_validity
        .parse::<u64>()
        .ok()
        .filter(|value| *value > 0)
        .ok_or_else(integrity_failure)?;
    let uid = uid
        .parse::<u64>()
        .ok()
        .filter(|value| *value > 0)
        .ok_or_else(integrity_failure)?;
    Ok((uid_validity, uid))
}

fn require_ok(response: &ImapCommandResponse) -> Result<(), QueryPortError> {
    match response.status() {
        ImapTaggedStatus::Ok => Ok(()),
        ImapTaggedStatus::No | ImapTaggedStatus::Bad => Err(dependency_unavailable()),
    }
}

fn map_transport_error(error: ImapTransportError) -> QueryPortError {
    match error {
        ImapTransportError::IntegrityFailure => integrity_failure(),
        ImapTransportError::Authentication
        | ImapTransportError::ProviderPolicy
        | ImapTransportError::DependencyUnavailable => dependency_unavailable(),
    }
}

fn find_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() {
        return Some(0);
    }
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

fn invalid_cursor() -> QueryPortError {
    QueryPortError::new(QueryPortErrorClass::InvalidCursor)
}

fn integrity_failure() -> QueryPortError {
    QueryPortError::new(QueryPortErrorClass::IntegrityFailure)
}

fn dependency_unavailable() -> QueryPortError {
    QueryPortError::new(QueryPortErrorClass::DependencyUnavailable)
}

#[cfg(test)]
mod tests {
    use super::{
        decode_base64, decode_quoted_printable, extract_first_literal, multipart_sections,
        parse_imap_internal_date, parse_search_uids,
    };

    #[test]
    fn search_parser_keeps_only_numeric_uids() -> Result<(), Box<dyn std::error::Error>> {
        assert_eq!(
            parse_search_uids("* SEARCH 7 9 10\r\np000003 OK done\r\n")?,
            vec![7, 9, 10]
        );
        assert!(parse_search_uids("* SEARCH nope\r\np000003 OK done\r\n").is_err());
        Ok(())
    }

    #[test]
    fn imap_internal_date_respects_numeric_zone() -> Result<(), Box<dyn std::error::Error>> {
        let utc =
            parse_imap_internal_date("09-Aug-2026 20:00:00 +0300").ok_or("date parse failed")?;
        assert_eq!(utc.value(), 1_786_294_800_000);
        Ok(())
    }

    #[test]
    fn literal_parser_uses_declared_length_not_delimiters() -> Result<(), Box<dyn std::error::Error>>
    {
        let response = b"* 1 FETCH (BODY[TEXT] {5}\r\na\r\nbc)\r\np000004 OK done\r\n";
        assert_eq!(extract_first_literal(response)?, Some(&b"a\r\nbc"[..]));
        Ok(())
    }

    #[test]
    fn transfer_decoders_are_bounded() -> Result<(), Box<dyn std::error::Error>> {
        assert_eq!(decode_base64(b"SGVsbG8=", 64)?, b"Hello");
        assert!(decode_base64(b"SGVsbG8=", 4).is_err());
        assert_eq!(
            decode_quoted_printable(b"hello=20world=\r\nnext", 64)?,
            b"hello worldnext"
        );
        Ok(())
    }

    #[test]
    fn multipart_split_is_boundary_scoped() -> Result<(), Box<dyn std::error::Error>> {
        let body = b"preamble\r\n--b\r\nContent-Type: text/plain\r\n\r\none\r\n--b\r\nContent-Type: text/html\r\n\r\n<b>two</b>\r\n--b--\r\n";
        let sections = multipart_sections(body, b"b")?;
        assert_eq!(sections.len(), 2);
        assert!(sections[0].ends_with(b"one"));
        assert!(sections[1].ends_with(b"<b>two</b>"));
        Ok(())
    }
}
