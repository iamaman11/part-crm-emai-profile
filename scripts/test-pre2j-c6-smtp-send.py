from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def read(path: str) -> str:
    return (ROOT / path).read_text(encoding="utf-8")


def require(text: str, *needles: str) -> None:
    missing = [needle for needle in needles if needle not in text]
    if missing:
        raise SystemExit(f"missing C6 evidence: {missing}")


def forbid(text: str, *needles: str) -> None:
    present = [needle for needle in needles if needle in text]
    if present:
        raise SystemExit(f"forbidden C6 evidence: {present}")


port = read("crates/application-ports/src/outbound_mail.rs")
provider = read("crates/cloudflare-adapters/src/smtp_outbound_mail.rs")
credential = read("crates/cloudflare-adapters/src/smtp_send_credential.rs")
transport = read("crates/cloudflare-adapters/src/smtp_session.rs")
auth = read("crates/cloudflare-adapters/src/smtp_session/auth.rs")
mime = read("crates/cloudflare-adapters/src/smtp_outbound_mail/mime.rs")
encoding = read("crates/cloudflare-adapters/src/smtp_outbound_mail/mime/encoding.rs")
mime_tests = read("crates/cloudflare-adapters/src/smtp_outbound_mail/mime/tests.rs")
source = read("crates/cloudflare-adapters/src/smtp_outbound_mail/source.rs")
c4 = read("crates/use-cases-mailboxes/src/outbound_mail.rs")

require(port, "RetryableNotSent", "Rejected", "Ambiguous", "OutboundMailProviderPort")
forbid(port, "smtp.gmail.com", "smtp.office365.com", "XOAUTH2")

send_impl = provider.split("impl OutboundMailProviderPort", 1)[1]
ordered = [
    "find_binding",
    "resolve_smtp_send_credential",
    "resolve_source_context",
    "render_mime",
    "SmtpSession::connect",
    "send_message",
]
positions = [send_impl.find(value) for value in ordered]
if any(position < 0 for position in positions) or positions != sorted(positions):
    raise SystemExit("C6 provider ordering drifted")
require(provider, "MailboxProvider::Imap", "!binding.is_executable()", "provider_message_reference: None")

require(credential, "MAILBOX_SECRET_RESOLVER", "SMTP_SEND", "SmtpTlsMode::Implicit, 465", "SmtpTlsMode::StartTls, 587")
require(credential, "password.zeroize()", "access_token.zeroize()")
forbid(credential, "D1Database", "INSERT INTO", "println!")

require(transport, "SecureTransport::On", "SecureTransport::StartTls", "start_tls()")
require(transport, 'send_command(&mut self.socket, "DATA", false)', "read_reply_after_data")
require(transport, "SmtpSendFailure::Ambiguous", "RetryableNotSent", "Rejected", "SMTPUTF8")
require(transport, "wire.zeroize()", "checked_add(read)", "authentication_capability_accepts_standard_and_legacy_forms")
forbid(transport, "SecureTransport::Off", "println!")

require(auth, "AUTH PLAIN", "AUTH LOGIN", "AUTH XOAUTH2", "payload.zeroize()", "encoded.zeroize()", "command.zeroize()")
forbid(auth, "println!", "console_log!", "console_error!")

require(mime, "multipart/alternative", "MAX_RENDERED_MESSAGE_BYTES", ".chain(recipients.bcc())")
require(encoding, "Content-Transfer-Encoding: base64", "=?UTF-8?B?", "push_reference_header")
forbid(encoding, "Content-Transfer-Encoding: 8bit")
require(mime_tests, "multipart_render_is_deterministic_encoded_and_bcc_is_envelope_only", "text_only_and_html_only_are_base64_encoded", "long_ascii_subject_is_folded_as_encoded_words")

require(source, "EXAMINE INBOX", "UID FETCH", "MESSAGE-ID", "REFERENCES", "reply_recipients", "reply_all_recipients", "OutboundMailOperation::Forward")
forbid(source, "gmail.googleapis.com", "GmailMessageMetadataResponse", "threadId")

claim = c4.find("claim_dispatch")
send = c4.find("provider.send")
if claim < 0 or send < 0 or claim >= send:
    raise SystemExit("C4 durable claim no longer precedes SMTP execution")

for text in (provider, credential, transport, auth, mime, encoding, source):
    forbid(text, "console_log!", "console_error!", "log::")

print("Pre-2J C6 SMTP send evidence OK")
