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


c2 = read("crates/cloudflare-adapters/src/gmail_oauth_provisioning.rs")
require(c2, "https://www.googleapis.com/auth/gmail.readonly", "C2 readonly scope")
forbid(c2, "https://www.googleapis.com/auth/gmail.send", "C2 send scope widening")

port = read("crates/application-ports/src/outbound_mail.rs")
for needle in [
    "gmail.googleapis.com",
    "users/me/messages/send",
    "threadId",
    "In-Reply-To",
    "References",
]:
    forbid(port, needle, "provider-specific inner contract")

consent = read("crates/cloudflare-adapters/src/gmail_send_capability.rs")
require(consent, "https://www.googleapis.com/auth/gmail.send", "C5 send scope")
require(consent, "x-profile-oauth-include-granted-scopes", "incremental consent")
require(consent, "gmail/send/oauth/start", "send consent start")
require(consent, "gmail/send/oauth/complete", "send consent completion")

credential = read("crates/cloudflare-adapters/src/gmail_send_credential.rs")
require(credential, "gmail/send/resolve", "purpose-scoped credential resolver")
require(credential, "x-profile-mailbox-capability", "credential capability marker")
require(credential, '"SEND"', "send capability")

provider = read("crates/cloudflare-adapters/src/gmail_outbound_mail.rs")
positions = [
    provider.find("find_binding"),
    provider.find("resolve_gmail_send_credential"),
    provider.find("resolve_message_context"),
    provider.find("render_mime"),
    provider.find("send_gmail_message"),
]
if any(position < 0 for position in positions) or positions != sorted(positions):
    raise SystemExit("Gmail provider execution ordering drifted")
require(provider, "408 | 425 | 500..=599 => SendStatus::Ambiguous", "ambiguous send policy")
require(provider, "429 => SendStatus::RetryableNotSent", "rate-limit retry policy")
require(provider, "400..=499 => SendStatus::Rejected", "definitive rejection policy")
require(provider, 'format!("gmail:{id}")', "provider reference convention")

source = read("crates/cloudflare-adapters/src/gmail_outbound_mail/source.rs")
for needle in ["GMAIL_PROFILE_ENDPOINT", "GMAIL_MESSAGES_ENDPOINT", "Message-ID", "References"]:
    require(source, needle, "reply source translation")
forbid(source, "deny_unknown_fields", "Gmail metadata response strictness")

mime = read("crates/cloudflare-adapters/src/gmail_outbound_mail/mime.rs")
require(mime, "Content-Transfer-Encoding: base64", "MIME body encoding")
require(mime, "encode_base64url_unpadded", "Gmail raw encoding")
require(mime, "multipart/alternative", "text/html multipart encoding")

c4 = read("crates/use-cases-mailboxes/src/outbound_mail.rs")
claim = c4.find("claim_dispatch")
send = c4.find("provider.send")
if claim < 0 or send < 0 or claim >= send:
    raise SystemExit("C4 durable claim no longer precedes provider send")

for text in [consent, credential, provider, source]:
    for needle in ["println!", "console_log!", "console_error!", "log::"]:
        forbid(text, needle, "C5 provider logging")

print("Pre-2J C5 Gmail send evidence OK")
