from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def read(path: str) -> str:
    return (ROOT / path).read_text(encoding="utf-8")


def require(text: str, needle: str, context: str) -> None:
    if needle not in text:
        raise SystemExit(f"missing {context}: {needle}")


def forbid(text: str, needle: str, context: str) -> None:
    if needle in text:
        raise SystemExit(f"forbidden {context}: {needle}")


port = read("crates/application-ports/src/outbound_mail.rs")
for needle in [
    "RetryableNotSent",
    "Rejected",
    "Ambiguous",
    "provider_message_reference: Option<ProviderMessageReference>",
]:
    require(port, needle, "authoritative C4 provider outcome")
for needle in ["smtp.office365.com", "smtp.gmail.com", "STARTTLS", "XOAUTH2"]:
    forbid(port, needle, "provider-specific inner contract")

provider = read("crates/cloudflare-adapters/src/smtp_outbound_mail.rs")
marker = "impl OutboundMailProviderPort"
require(provider, marker, "SMTP provider implementation")
send_impl = provider.split(marker, 1)[1]
positions = [
    send_impl.find("find_binding"),
    send_impl.find("resolve_smtp_send_credential"),
    send_impl.find("resolve_source_context"),
    send_impl.find("render_mime"),
    send_impl.find("SmtpSession::connect"),
    send_impl.find("send_message"),
]
if any(position < 0 for position in positions) or positions != sorted(positions):
    raise SystemExit("SMTP provider execution ordering drifted")
require(provider, "binding.provider() != MailboxProvider::Imap", "standards provider revalidation")
require(provider, "!binding.is_executable()", "executable binding revalidation")
require(provider, "provider_message_reference: None", "no invented SMTP provider reference")
require(provider, "SmtpSendFailure::Ambiguous", "C4 ambiguity mapping")

credential = read("crates/cloudflare-adapters/src/smtp_send_credential.rs")
require(credential, '"MAILBOX_SECRET_RESOLVER"', "sole credential resolver")
require(credential, '"SMTP_SEND"', "purpose-scoped SMTP credential resolution")
require(credential, "(SmtpTlsMode::Implicit, 465)", "implicit TLS endpoint")
require(credential, "(SmtpTlsMode::StartTls, 587)", "STARTTLS endpoint")
require(credential, "password.zeroize()", "password zeroization")
require(credential, "access_token.zeroize()", "access token zeroization")
for needle in ["D1Database", "INSERT INTO", "UPDATE mailbox", "println!", "log::"]:
    forbid(credential, needle, "credential persistence or logging")

transport = read("crates/cloudflare-adapters/src/smtp_session.rs")
require(transport, "SecureTransport::On", "implicit encrypted transport")
require(transport, "SecureTransport::StartTls", "STARTTLS bootstrap")
require(transport, "session.socket.start_tls()", "STARTTLS upgrade")
require(transport, 'send_command(&mut self.socket, "DATA", false)', "SMTP DATA boundary")
require(transport, "read_reply_after_data", "post-DATA ambiguity read")
require(transport, "SmtpSendFailure::Ambiguous", "ambiguous post-acceptance outcome")
require(transport, "400..=499 => Err(SmtpSendFailure::RetryableNotSent)", "safe retry classification")
require(transport, "500..=599 => Err(SmtpSendFailure::Rejected)", "definitive rejection classification")
require(transport, 'self.ehlo.capability("SMTPUTF8")', "SMTPUTF8 capability negotiation")
require(transport, 'format!("MAIL FROM:<{envelope_from}> SMTPUTF8")', "SMTPUTF8 MAIL FROM option")
require(transport, "wire.zeroize()", "post-DATA wire zeroization")
require(transport, "checked_add(read)", "bounded SMTP reply growth")
for needle in ["SecureTransport::Off", "println!", "console_log!", "log::"]:
    forbid(transport, needle, "plaintext transport or logging")

auth = read("crates/cloudflare-adapters/src/smtp_session/auth.rs")
for needle in ["AUTH PLAIN", "AUTH LOGIN", "AUTH XOAUTH2", "auth=Bearer"]:
    require(auth, needle, "password and XOAUTH2 SMTP authentication")
for needle in ["payload.zeroize()", "encoded.zeroize()", "command.zeroize()"]:
    require(auth, needle, "SASL material zeroization")
for needle in ["println!", "console_log!", "console_error!", "log::"]:
    forbid(auth, needle, "SMTP authentication logging")

mime = read("crates/cloudflare-adapters/src/smtp_outbound_mail/mime.rs")
require(mime, "multipart/alternative", "text/html multipart encoding")
require(mime, 'push_address_header(&mut output, "To"', "To header rendering")
require(mime, 'push_address_header(&mut output, "Cc"', "Cc header rendering")
require(mime, ".chain(recipients.bcc())", "Bcc envelope delivery")
forbid(mime, 'push_address_header(&mut output, "Bcc"', "Bcc header disclosure")
forbid(mime, 'push_header(&mut output, "Bcc"', "Bcc header disclosure")
require(mime, "Content-Transfer-Encoding: base64", "7-bit-safe MIME body transport")
require(mime, 'word.push_str("=?UTF-8?B?")', "RFC 2047 UTF-8 subject encoding")
forbid(mime, "Content-Transfer-Encoding: 8bit", "unnegotiated 8BITMIME dependency")
require(mime, "MAX_RENDERED_MESSAGE_BYTES", "bounded rendered MIME")
require(mime, "text_only_and_html_only_are_base64_encoded", "deterministic text/html fixtures")
require(
    mime,
    "multipart_render_is_deterministic_encoded_and_bcc_is_envelope_only",
    "multipart and envelope fixture",
)

source = read("crates/cloudflare-adapters/src/smtp_outbound_mail/source.rs")
for needle in ["EXAMINE INBOX", "UID FETCH", "MESSAGE-ID", "REFERENCES"]:
    require(source, needle, "standards reply source translation")
require(source, "checked_add(1)", "fail-closed address nesting")
require(source, "reply_recipients", "reply envelope derivation")
require(source, "reply_all_recipients", "reply-all envelope derivation")
for needle in ["gmail.googleapis.com", "GmailMessageMetadataResponse", "threadId"]:
    forbid(source, needle, "Gmail DTO leakage")

c4 = read("crates/use-cases-mailboxes/src/outbound_mail.rs")
claim = c4.find("claim_dispatch")
send = c4.find("provider.send")
if claim < 0 or send < 0 or claim >= send:
    raise SystemExit("C4 durable claim no longer precedes SMTP provider send")

for text in [provider, source, mime]:
    for needle in ["println!", "console_log!", "console_error!", "log::"]:
        forbid(text, needle, "C6 provider logging")

print("Pre-2J C6 SMTP send evidence OK")
