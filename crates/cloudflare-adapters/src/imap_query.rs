use crate::cloud_mailbox_secrets::ImapCredential;
use crate::imap_session::{
    ImapCommandResponse, ImapSession, ImapTaggedStatus, ImapTransportError, push_imap_quoted,
};
use application_ports::query::{
    QueryCursor, QueryPage, QueryPageRequest, QueryPortError, QueryPortErrorClass,
};
use application_ports::query_mail_provider::{
    ClientMailProviderQueryPort, MailMessageBody, MailMessageSummary, MailboxMessageReference,
    SearchClientMailboxMessagesRequest,
};
use mailbox_domain::MailboxBinding;
use profile_platform_primitives::{MailboxBindingId, UnixMillis};
use std::collections::BTreeMap;

const MAX_IMAP_QUERY_PAGE_SIZE: u16 = 25;
const MAX_IMAP_UID_WINDOW: u64 = 500;
const MAX_IMAP_SEARCH_WINDOWS: usize = 8;
const MAX_IMAP_CONTROL_RESPONSE_BYTES: usize = 64 * 1024;
const MAX_IMAP_METADATA_RESPONSE_BYTES: usize = 128 * 1024;
const MAX_IMAP_BODY_RESPONSE_BYTES: usize = 1024 * 1024 + 128 * 1024;
const MAX_HEADER_BLOCK_BYTES: usize = 64 * 1024;
const MAX_MIME_DEPTH: usize = 8;
const MAX_MIME_PARTS: usize = 128;
const MAX_BOUNDARY_BYTES: usize = 200;
const IMAP_CURSOR_PREFIX: &str = "imap:";
const IMAP_REFERENCE_PREFIX: &str = "imap:";

pub(crate) struct ImapMailQueryProvider {
    credential: ImapCredential,
}

impl ImapMailQueryProvider {
    #[must_use]
    pub(crate) const fn new(credential: ImapCredential) -> Self {
        Self { credential }
    }
}

impl ClientMailProviderQueryPort for ImapMailQueryProvider {
    async fn search(
        &self,
        binding: &MailboxBinding,
        request: &SearchClientMailboxMessagesRequest,
    ) -> Result<QueryPage<MailMessageSummary>, QueryPortError> {
        search_imap(binding, request, &self.credential).await
    }

    async fn get_body(
        &self,
        binding: &MailboxBinding,
        reference: &MailboxMessageReference,
    ) -> Result<Option<MailMessageBody>, QueryPortError> {
        get_imap_body(binding, reference, &self.credential).await
    }
}

async fn search_imap(
    binding: &MailboxBinding,
    request: &SearchClientMailboxMessagesRequest,
    credential: &ImapCredential,
) -> Result<QueryPage<MailMessageSummary>, QueryPortError> {
    let page_size = usize::from(request.page().limit().value().min(MAX_IMAP_QUERY_PAGE_SIZE));
    let mut session = ImapSession::connect(credential)
        .await
        .map_err(map_transport_error)?;
    let snapshot = examine_inbox(&mut session).await?;
    let mut before_uid = snapshot.uid_next;
    if let Some(cursor) = request.page().cursor() {
        let (uid_validity, cursor_before_uid) = parse_imap_cursor(cursor.as_str())?;
        if uid_validity != snapshot.uid_validity || cursor_before_uid == 0 {
            return Err(invalid_cursor());
        }
        before_uid = cursor_before_uid.min(snapshot.uid_next);
    }

    let mut selected = Vec::with_capacity(page_size);
    let mut next_before_uid = None;
    let mut window_end = before_uid.saturating_sub(1);
    for _ in 0..MAX_IMAP_SEARCH_WINDOWS {
        if window_end == 0 || selected.len() >= page_size {
            break;
        }
        let window_start = window_end
            .saturating_sub(MAX_IMAP_UID_WINDOW.saturating_sub(1))
            .max(1);
        let mut matches = search_uid_window(
            &mut session,
            window_start,
            window_end,
            request,
        )
        .await?;
        matches.retain(|uid| *uid >= window_start && *uid <= window_end);
        matches.sort_unstable_by(|left, right| right.cmp(left));
        for uid in matches {
            if selected.len() == page_size {
                break;
            }
            selected.push(uid);
        }
        if selected.len() == page_size {
            next_before_uid = selected.last().copied();
            break;
        }
        if window_start == 1 {
            break;
        }
        window_end = window_start - 1;
        next_before_uid = Some(window_end.saturating_add(1));
    }

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
        .filter(|value| *value > 1)
        .map(|value| imap_query_cursor(snapshot.uid_validity, value))
        .transpose()?;
    QueryPage::new(items, next_cursor).map_err(|_| integrity_failure())
}

async fn get_imap_body(
    binding: &MailboxBinding,
    reference: &MailboxMessageReference,
    credential: &ImapCredential,
) -> Result<Option<MailMessageBody>, QueryPortError> {
    if reference.binding_id() != binding.binding_id() {
        return Err(integrity_failure());
    }
    let (reference_uid_validity, uid) = parse_imap_reference(reference.provider_reference())?;
    let mut session = ImapSession::connect(credential)
        .await
        .map_err(map_transport_error)?;
    let snapshot = examine_inbox(&mut session).await?;
    if snapshot.uid_validity != reference_uid_validity {
        return Ok(None);
    }
    let command = format!("UID FETCH {uid} (UID BODY.PEEK[])");
    let response = session
        .execute(&command, MAX_IMAP_BODY_RESPONSE_BYTES)
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
    let raw_message = extract_first_literal(response.bytes())?.ok_or_else(integrity_failure)?;
    if raw_message.len() > 1024 * 1024 {
        return Err(integrity_failure());
    }
    let content = extract_mime_text(raw_message, 1024 * 1024)?;
    MailMessageBody::new(reference.clone(), content).map(Some).map_err(|_| integrity_failure())
}

struct ImapMailboxSnapshot {
    uid_validity: u64,
    uid_next: u64,
}

struct ImapMessageMetadata {
    uid: u64,
    sent_at: UnixMillis,
    subject: String,
    sender: String,
    size_bytes: Option<u64>,
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
    if let Some(term) = request.term() {
        if !term.as_str().is_ascii() {
            command.push_str("CHARSET UTF-8 ");
        }
        command.push_str("UID ");
        command.push_str(&start.to_string());
        command.push(':');
        command.push_str(&end.to_string());
        command.push_str(" TEXT");
        let response = if term.as_str().is_ascii() {
            command.push(' ');
            push_imap_quoted(&mut command, term.as_str());
            session
                .execute(&command, MAX_IMAP_CONTROL_RESPONSE_BYTES)
                .await
                .map_err(map_transport_error)?
        } else {
            session
                .execute_with_literal(
                    &command,
                    term.as_str().as_bytes(),
                    MAX_IMAP_CONTROL_RESPONSE_BYTES,
                )
                .await
                .map_err(map_transport_error)?
        };
        require_ok(&response)?;
        return parse_search_uids(&response.text_lossy());
    }

    command.push_str("UID ");
    command.push_str(&start.to_string());
    command.push(':');
    command.push_str(&end.to_string());
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
    let headers = parse_headers(header_bytes)?;
    Ok(Some(ImapMessageMetadata {
        uid,
        sent_at: internal_date,
        subject: headers
            .get("subject")
            .map(String::as_str)
            .unwrap_or_default()
            .to_owned(),
        sender: headers
            .get("from")
            .map(String::as_str)
            .unwrap_or_default()
            .to_owned(),
        size_bytes: rfc822_size,
    }))
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
    MailMessageSummary::new(
        reference,
        metadata.sent_at,
        metadata.subject.clone(),
        metadata.sender.clone(),
        metadata.size_bytes,
    )
    .map_err(|_| integrity_failure())
}

fn parse_search_uids(text: &str) -> Result<Vec<u64>, QueryPortError> {
    let line = text
        .lines()
        .find(|line| line.strip_suffix('\r').unwrap_or(line).starts_with("* SEARCH"))
        .ok_or_else(integrity_failure)?;
    let line = line.strip_suffix('\r').unwrap_or(line);
    let suffix = line.strip_prefix("* SEARCH").ok_or_else(integrity_failure)?;
    let mut uids = Vec::new();
    for token in suffix.split_ascii_whitespace() {
        let uid = token.parse::<u64>().map_err(|_| integrity_failure())?;
        if uid == 0 {
            return Err(integrity_failure());
        }
        uids.push(uid);
    }
    Ok(uids)
}

fn parse_bracket_u64(text: &str, atom: &str) -> Option<u64> {
    let marker = format!("[{atom} ");
    let start = text.find(&marker)? + marker.len();
    let tail = text.get(start..)?;
    let end = tail.find(']')?;
    tail.get(..end)?.trim().parse::<u64>().ok()
}

fn parse_fetch_atom_u64(text: &str, atom: &str) -> Option<u64> {
    let marker = format!("{atom} ");
    let start = text.find(&marker)? + marker.len();
    let tail = text.get(start..)?;
    let token = tail
        .split(|character: char| character.is_ascii_whitespace() || matches!(character, ')' | '('))
        .next()?;
    token.parse::<u64>().ok()
}

fn extract_first_literal(bytes: &[u8]) -> Result<Option<&[u8]>, QueryPortError> {
    let mut cursor = 0_usize;
    while cursor < bytes.len() {
        let Some(open_offset) = bytes[cursor..].iter().position(|byte| *byte == b'{') else {
            return Ok(None);
        };
        let open = cursor
            .checked_add(open_offset)
            .ok_or_else(integrity_failure)?;
        let Some(close_offset) = bytes[open..].iter().position(|byte| *byte == b'}') else {
            return Err(integrity_failure());
        };
        let close = open
            .checked_add(close_offset)
            .ok_or_else(integrity_failure)?;
        let length_text = std::str::from_utf8(
            bytes
                .get(open + 1..close)
                .ok_or_else(integrity_failure)?,
        )
        .map_err(|_| integrity_failure())?;
        if !length_text.bytes().all(|byte| byte.is_ascii_digit()) || length_text.is_empty() {
            cursor = close.checked_add(1).ok_or_else(integrity_failure)?;
            continue;
        }
        let literal_length = length_text
            .parse::<usize>()
            .map_err(|_| integrity_failure())?;
        let literal_start = close.checked_add(3).ok_or_else(integrity_failure)?;
        if bytes.get(close + 1..literal_start) != Some(&b"\r\n"[..]) {
            return Err(integrity_failure());
        }
        let literal_end = literal_start
            .checked_add(literal_length)
            .ok_or_else(integrity_failure)?;
        let literal = bytes
            .get(literal_start..literal_end)
            .ok_or_else(integrity_failure)?;
        return Ok(Some(literal));
    }
    Ok(None)
}

fn parse_headers(bytes: &[u8]) -> Result<BTreeMap<String, String>, QueryPortError> {
    let text = std::str::from_utf8(bytes).map_err(|_| integrity_failure())?;
    let mut headers = BTreeMap::new();
    let mut current_name = None::<String>;
    let mut current_value = String::new();
    for raw_line in text.lines() {
        let line = raw_line.strip_suffix('\r').unwrap_or(raw_line);
        if line.is_empty() {
            break;
        }
        if line.starts_with([' ', '\t']) {
            if current_name.is_none() {
                return Err(integrity_failure());
            }
            if !current_value.is_empty() {
                current_value.push(' ');
            }
            current_value.push_str(line.trim());
            if current_value.len() > MAX_HEADER_BLOCK_BYTES {
                return Err(integrity_failure());
            }
            continue;
        }
        if let Some(name) = current_name.take() {
            headers.insert(name, current_value.trim().to_owned());
            current_value.clear();
        }
        let (name, value) = line.split_once(':').ok_or_else(integrity_failure)?;
        let normalized = name.trim().to_ascii_lowercase();
        if normalized.is_empty() || normalized.len() > 128 {
            return Err(integrity_failure());
        }
        current_name = Some(normalized);
        current_value.push_str(value.trim());
    }
    if let Some(name) = current_name {
        headers.insert(name, current_value.trim().to_owned());
    }
    Ok(headers)
}

fn extract_mime_text(message: &[u8], maximum_bytes: usize) -> Result<String, QueryPortError> {
    let mut budget = MimeBudget {
        remaining_parts: MAX_MIME_PARTS,
        maximum_output_bytes: maximum_bytes,
    };
    let bytes = decode_mime_entity(message, 0, &mut budget)?;
    String::from_utf8(bytes).map_err(|_| integrity_failure())
}

struct MimeBudget {
    remaining_parts: usize,
    maximum_output_bytes: usize,
}

fn decode_mime_entity(
    entity: &[u8],
    depth: usize,
    budget: &mut MimeBudget,
) -> Result<Vec<u8>, QueryPortError> {
    if depth > MAX_MIME_DEPTH || budget.remaining_parts == 0 {
        return Err(integrity_failure());
    }
    budget.remaining_parts -= 1;
    let (headers, body) = split_headers_body(entity)?;
    let parsed_headers = parse_headers(headers)?;
    let content_type = parsed_headers
        .get("content-type")
        .map(String::as_str)
        .unwrap_or("text/plain");
    if content_type.to_ascii_lowercase().starts_with("multipart/") {
        let boundary = content_type_parameter(content_type, "boundary")?
            .ok_or_else(integrity_failure)?;
        let mut output = Vec::new();
        for section in multipart_sections(body, boundary.as_bytes())? {
            let decoded = decode_mime_entity(section, depth + 1, budget)?;
            append_bounded(&mut output, &decoded, budget.maximum_output_bytes)?;
        }
        return Ok(output);
    }
    if content_type.to_ascii_lowercase().starts_with("message/rfc822") {
        return decode_mime_entity(body, depth + 1, budget);
    }
    if !content_type.to_ascii_lowercase().starts_with("text/") {
        return Ok(Vec::new());
    }
    let transfer_encoding = parsed_headers
        .get("content-transfer-encoding")
        .map(|value| value.trim().to_ascii_lowercase())
        .unwrap_or_default();
    match transfer_encoding.as_str() {
        "" | "7bit" | "8bit" => {
            if body.len() > budget.maximum_output_bytes {
                return Err(integrity_failure());
            }
            Ok(body.to_vec())
        }
        "base64" => decode_base64(body, budget.maximum_output_bytes),
        "quoted-printable" => decode_quoted_printable(body, budget.maximum_output_bytes),
        _ => Err(integrity_failure()),
    }
}

fn split_headers_body(entity: &[u8]) -> Result<(&[u8], &[u8]), QueryPortError> {
    if let Some(index) = find_bytes(entity, b"\r\n\r\n") {
        return Ok((&entity[..index], &entity[index + 4..]));
    }
    if let Some(index) = find_bytes(entity, b"\n\n") {
        return Ok((&entity[..index], &entity[index + 2..]));
    }
    Ok((&[][..], entity))
}

fn content_type_parameter(content_type: &str, name: &str) -> Result<Option<String>, QueryPortError> {
    let mut parts = content_type.split(';');
    let _ = parts.next();
    for part in parts {
        let (parameter_name, value) = part.split_once('=').ok_or_else(integrity_failure)?;
        if parameter_name.trim().eq_ignore_ascii_case(name) {
            let value = value.trim();
            let value = if value.starts_with('"') {
                if !value.ends_with('"') || value.len() < 2 {
                    return Err(integrity_failure());
                }
                &value[1..value.len() - 1]
            } else {
                value
            };
            if value.is_empty() || value.len() > MAX_BOUNDARY_BYTES {
                return Err(integrity_failure());
            }
            return Ok(Some(value.to_owned()));
        }
    }
    Ok(None)
}

fn multipart_sections<'a>(
    body: &'a [u8],
    boundary: &[u8],
) -> Result<Vec<&'a [u8]>, QueryPortError> {
    if boundary.is_empty() || boundary.len() > MAX_BOUNDARY_BYTES {
        return Err(integrity_failure());
    }
    let marker = [b"--".as_slice(), boundary].concat();
    let closing_marker = [marker.as_slice(), b"--"].concat();
    let mut sections = Vec::new();
    let mut section_start = None::<usize>;
    let mut cursor = 0_usize;
    while cursor <= body.len() {
        let line_end = body[cursor..]
            .iter()
            .position(|byte| *byte == b'\n')
            .map(|offset| cursor + offset + 1)
            .unwrap_or(body.len());
        let mut line = &body[cursor..line_end];
        if line.ends_with(b"\n") {
            line = &line[..line.len() - 1];
        }
        if line.ends_with(b"\r") {
            line = &line[..line.len() - 1];
        }
        if line == marker.as_slice() || line == closing_marker.as_slice() {
            if let Some(start) = section_start.take() {
                let mut end = cursor;
                while end > start && matches!(body[end - 1], b'\r' | b'\n') {
                    end -= 1;
                }
                if end > start {
                    sections.push(&body[start..end]);
                    if sections.len() > MAX_MIME_PARTS {
                        return Err(integrity_failure());
                    }
                }
            }
            if line == closing_marker.as_slice() {
                break;
            }
            section_start = Some(line_end);
        }
        if line_end == body.len() {
            break;
        }
        cursor = line_end;
    }
    Ok(sections)
}

fn append_bounded(output: &mut Vec<u8>, value: &[u8], maximum_bytes: usize) -> Result<(), QueryPortError> {
    if output.len().saturating_add(value.len()) > maximum_bytes {
        return Err(integrity_failure());
    }
    output.extend_from_slice(value);
    Ok(())
}

fn decode_base64(input: &[u8], maximum_bytes: usize) -> Result<Vec<u8>, QueryPortError> {
    let compact = input
        .iter()
        .copied()
        .filter(|byte| !byte.is_ascii_whitespace())
        .collect::<Vec<_>>();
    if compact.len() > maximum_bytes.saturating_mul(2).saturating_add(16) {
        return Err(integrity_failure());
    }
    if compact.len() % 4 != 0 {
        return Err(integrity_failure());
    }
    let mut output = Vec::with_capacity((compact.len() / 4).saturating_mul(3));
    for chunk in compact.chunks_exact(4) {
        let a = base64_value(chunk[0]).ok_or_else(integrity_failure)?;
        let b = base64_value(chunk[1]).ok_or_else(integrity_failure)?;
        let c_padding = chunk[2] == b'=';
        let d_padding = chunk[3] == b'=';
        if c_padding && !d_padding {
            return Err(integrity_failure());
        }
        let c = if c_padding {
            0
        } else {
            base64_value(chunk[2]).ok_or_else(integrity_failure)?
        };
        let d = if d_padding {
            0
        } else {
            base64_value(chunk[3]).ok_or_else(integrity_failure)?
        };
        append_bounded(&mut output, &[a << 2 | b >> 4], maximum_bytes)?;
        if !c_padding {
            append_bounded(&mut output, &[b << 4 | c >> 2], maximum_bytes)?;
        }
        if !d_padding {
            append_bounded(&mut output, &[c << 6 | d], maximum_bytes)?;
        }
    }
    Ok(output)
}

fn base64_value(byte: u8) -> Option<u8> {
    match byte {
        b'A'..=b'Z' => Some(byte - b'A'),
        b'a'..=b'z' => Some(byte - b'a' + 26),
        b'0'..=b'9' => Some(byte - b'0' + 52),
        b'+' => Some(62),
        b'/' => Some(63),
        _ => None,
    }
}

fn decode_quoted_printable(input: &[u8], maximum_bytes: usize) -> Result<Vec<u8>, QueryPortError> {
    let mut output = Vec::with_capacity(input.len().min(maximum_bytes));
    let mut index = 0_usize;
    while index < input.len() {
        if input[index] != b'=' {
            append_bounded(&mut output, &input[index..index + 1], maximum_bytes)?;
            index += 1;
            continue;
        }
        if input.get(index + 1..index + 3) == Some(&b"\r\n"[..]) {
            index += 3;
            continue;
        }
        if input.get(index + 1) == Some(&b'\n') {
            index += 2;
            continue;
        }
        let high = *input.get(index + 1).ok_or_else(integrity_failure)?;
        let low = *input.get(index + 2).ok_or_else(integrity_failure)?;
        let high = hex_value(high).ok_or_else(integrity_failure)?;
        let low = hex_value(low).ok_or_else(integrity_failure)?;
        append_bounded(&mut output, &[high << 4 | low], maximum_bytes)?;
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

fn parse_internal_date_from_fetch(text: &str) -> Result<UnixMillis, QueryPortError> {
    let marker = "INTERNALDATE \"";
    let start = text.find(marker).ok_or_else(integrity_failure)? + marker.len();
    let tail = text.get(start..).ok_or_else(integrity_failure)?;
    let end = tail.find('"').ok_or_else(integrity_failure)?;
    parse_imap_internal_date(tail.get(..end).ok_or_else(integrity_failure)?)
        .ok_or_else(integrity_failure)
}

fn parse_imap_internal_date(value: &str) -> Option<UnixMillis> {
    let mut parts = value.split_ascii_whitespace();
    let date = parts.next()?;
    let time = parts.next()?;
    let zone = parts.next()?;
    if parts.next().is_some() {
        return None;
    }
    let mut date_parts = date.split('-');
    let day = date_parts.next()?.parse::<u32>().ok()?;
    let month = month_number(date_parts.next()?)?;
    let year = date_parts.next()?.parse::<i64>().ok()?;
    if date_parts.next().is_some() || !(1..=31).contains(&day) || year < 1970 {
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
    let zone_hour = std::str::from_utf8(&zone_bytes[1..3]).ok()?.parse::<i64>().ok()?;
    let zone_minute = std::str::from_utf8(&zone_bytes[3..5]).ok()?.parse::<i64>().ok()?;
    if zone_hour > 23 || zone_minute > 59 {
        return None;
    }
    let offset_seconds = (zone_hour * 60 + zone_minute) * 60;
    let offset_seconds = if zone_bytes[0] == b'-' {
        -offset_seconds
    } else {
        offset_seconds
    };
    let days = days_from_civil(year, month, day)?;
    let local_seconds = days
        .checked_mul(86_400)?
        .checked_add(i64::from(hour) * 3_600)?
        .checked_add(i64::from(minute) * 60)?
        .checked_add(i64::from(second.min(59)))?;
    let utc_seconds = local_seconds.checked_sub(offset_seconds)?;
    let utc_millis = u64::try_from(utc_seconds).ok()?.checked_mul(1_000)?;
    Some(UnixMillis::new(utc_millis))
}

fn month_number(value: &str) -> Option<u32> {
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

fn days_from_civil(year: i64, month: u32, day: u32) -> Option<i64> {
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    let adjusted_year = year - i64::from(month <= 2);
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

fn imap_query_cursor(uid_validity: u64, before_uid: u64) -> Result<QueryCursor, QueryPortError> {
    QueryCursor::parse(format!("{IMAP_CURSOR_PREFIX}{uid_validity}:{before_uid}"))
        .map_err(|_| integrity_failure())
}

fn parse_imap_cursor(cursor: &str) -> Result<(u64, u64), QueryPortError> {
    let value = cursor
        .strip_prefix(IMAP_CURSOR_PREFIX)
        .ok_or_else(invalid_cursor)?;
    let (uid_validity, before_uid) = value.split_once(':').ok_or_else(invalid_cursor)?;
    let uid_validity = uid_validity
        .parse::<u64>()
        .ok()
        .filter(|value| *value > 0)
        .ok_or_else(invalid_cursor)?;
    let before_uid = before_uid
        .parse::<u64>()
        .ok()
        .filter(|value| *value > 0)
        .ok_or_else(invalid_cursor)?;
    Ok((uid_validity, before_uid))
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
    fn multipart_split_is_boundary_scoped() -> Result<(), Box<dyn std::error::Error>> {
        let body = b"preamble\r\n--abc\r\nContent-Type: text/plain\r\n\r\none\r\n--abc\r\nContent-Type: text/plain\r\n\r\ntwo\r\n--abc--\r\nepilogue";
        let sections = multipart_sections(body, b"abc")?;
        assert_eq!(sections.len(), 2);
        assert!(sections[0].ends_with(b"one"));
        assert!(sections[1].ends_with(b"two"));
        Ok(())
    }

    #[test]
    fn transfer_decoders_are_bounded() -> Result<(), Box<dyn std::error::Error>> {
        assert_eq!(decode_base64(b"SGVsbG8=", 16)?, b"Hello");
        assert_eq!(decode_quoted_printable(b"hello=20world", 16)?, b"hello world");
        assert!(decode_base64(b"SGVsbG8=", 4).is_err());
        assert!(decode_quoted_printable(b"hello", 4).is_err());
        Ok(())
    }
}
