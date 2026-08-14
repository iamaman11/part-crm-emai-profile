use crate::model::ResolverRoute;
use serde_json::{Map, Value};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ContractError {
    MissingField,
    UnexpectedField,
    InvalidField,
}

pub fn validate_payload(
    route: ResolverRoute,
    payload: &Map<String, Value>,
) -> Result<(), ContractError> {
    match route {
        ResolverRoute::Discard => object(payload, &["actorId", "secretHandle", "provider"], &[]),
        ResolverRoute::GmailOAuthStart => oauth_onboarding_start(payload, false),
        ResolverRoute::MicrosoftGraphOAuthStart => oauth_onboarding_start(payload, true),
        ResolverRoute::GmailOAuthInspect
        | ResolverRoute::GmailSendOAuthInspect
        | ResolverRoute::MicrosoftGraphOAuthInspect
        | ResolverRoute::StandardsMicrosoftOAuthInspect => {
            object(payload, &["oauthState"], &[])?;
            oauth_state(payload, "oauthState")
        }
        ResolverRoute::GmailOAuthComplete => oauth_complete(payload, false, false),
        ResolverRoute::GmailSendOAuthComplete => oauth_complete(payload, false, false),
        ResolverRoute::MicrosoftGraphOAuthComplete => oauth_complete(payload, true, false),
        ResolverRoute::StandardsMicrosoftOAuthComplete => oauth_complete(payload, false, true),
        ResolverRoute::GmailOAuthDeny
        | ResolverRoute::GmailSendOAuthDeny
        | ResolverRoute::MicrosoftGraphOAuthDeny
        | ResolverRoute::StandardsMicrosoftOAuthDeny => {
            object(payload, &["actorId", "oauthState"], &[])?;
            identifier(payload, "actorId", 128)?;
            oauth_state(payload, "oauthState")
        }
        ResolverRoute::GmailSendOAuthStart => {
            object(
                payload,
                &[
                    "actorId",
                    "mailboxBindingId",
                    "mailboxBindingVersion",
                    "oauthScope",
                    "oauthIncludeGrantedScopes",
                ],
                &[],
            )?;
            identifier(payload, "actorId", 128)?;
            identifier(payload, "mailboxBindingId", 160)?;
            positive_integer(payload, "mailboxBindingVersion")?;
            exact_string(
                payload,
                "oauthScope",
                "https://www.googleapis.com/auth/gmail.send",
            )?;
            exact_bool(payload, "oauthIncludeGrantedScopes", true)
        }
        ResolverRoute::GmailSendResolve => {
            object(
                payload,
                &["mailboxBindingId", "secretHandle", "provider", "capability"],
                &[],
            )?;
            identifier(payload, "mailboxBindingId", 160)?;
            identifier(payload, "secretHandle", 192)?;
            exact_string(payload, "provider", "GMAIL_API")?;
            exact_string(payload, "capability", "SEND")
        }
        ResolverRoute::MicrosoftGraphCursorStore => {
            object(payload, &["mailboxBindingId", "providerCursor"], &[])?;
            identifier(payload, "mailboxBindingId", 160)?;
            bounded_string(payload, "providerCursor", 16 * 1024)?;
            if payload["providerCursor"]
                .as_str()
                .is_some_and(|cursor| cursor.starts_with("https://graph.microsoft.com/v1.0/"))
            {
                Ok(())
            } else {
                Err(ContractError::InvalidField)
            }
        }
        ResolverRoute::MicrosoftGraphCursorResolve => {
            object(payload, &["mailboxBindingId", "cursorHandle"], &[])?;
            identifier(payload, "mailboxBindingId", 160)?;
            identifier(payload, "cursorHandle", 192)
        }
        ResolverRoute::MicrosoftGraphRefresh => {
            object(
                payload,
                &["secretHandle", "provider"],
                &["credentialPurpose"],
            )?;
            identifier(payload, "secretHandle", 192)?;
            exact_string(payload, "provider", "MICROSOFT_GRAPH")?;
            optional_bounded_string(payload, "credentialPurpose", 64)
        }
        ResolverRoute::Resolve => {
            object(
                payload,
                &["secretHandle", "provider"],
                &["credentialPurpose"],
            )?;
            identifier(payload, "secretHandle", 192)?;
            identifier(payload, "provider", 64)?;
            if !matches!(
                payload.get("provider").and_then(Value::as_str),
                Some("GMAIL_API" | "MICROSOFT_GRAPH" | "IMAP")
            ) {
                return Err(ContractError::InvalidField);
            }
            optional_bounded_string(payload, "credentialPurpose", 64)
        }
        ResolverRoute::StandardsMicrosoftOAuthStart => standards_oauth_start(payload),
        ResolverRoute::StandardsPasswordProvision => password_provision(payload),
    }
}

fn oauth_onboarding_start(payload: &Map<String, Value>, graph: bool) -> Result<(), ContractError> {
    let mut required = vec![
        "actorId",
        "mailboxOnboardingId",
        "mailboxOnboardingVersion",
        "oauthScope",
    ];
    if graph {
        required.push("oauthPkceMethod");
    }
    object(payload, &required, &[])?;
    identifier(payload, "actorId", 128)?;
    identifier(payload, "mailboxOnboardingId", 160)?;
    positive_integer(payload, "mailboxOnboardingVersion")?;
    if graph {
        exact_string(
            payload,
            "oauthScope",
            "openid offline_access https://graph.microsoft.com/Mail.Read",
        )?;
        exact_string(payload, "oauthPkceMethod", "S256")
    } else {
        exact_string(
            payload,
            "oauthScope",
            "https://www.googleapis.com/auth/gmail.readonly",
        )
    }
}

fn oauth_complete(
    payload: &Map<String, Value>,
    pkce: bool,
    scopes: bool,
) -> Result<(), ContractError> {
    let mut required = vec!["actorId", "oauthState", "oauthAuthorizationCode"];
    if pkce {
        required.push("oauthPkceMethod");
    }
    if scopes {
        required.push("oauthScopes");
    }
    object(payload, &required, &[])?;
    identifier(payload, "actorId", 128)?;
    oauth_state(payload, "oauthState")?;
    bounded_string(payload, "oauthAuthorizationCode", 8 * 1024)?;
    if pkce {
        exact_string(payload, "oauthPkceMethod", "S256")?;
    }
    if scopes {
        exact_string(
            payload,
            "oauthScopes",
            "https://outlook.office.com/IMAP.AccessAsUser.All https://outlook.office.com/SMTP.Send offline_access",
        )?;
    }
    Ok(())
}

fn standards_oauth_start(payload: &Map<String, Value>) -> Result<(), ContractError> {
    object(
        payload,
        &[
            "actorId",
            "mailboxOnboardingId",
            "mailboxOnboardingVersion",
            "provider",
            "authenticationMode",
            "oauthScopes",
            "oauthProtocol",
        ],
        &[],
    )?;
    identifier(payload, "actorId", 128)?;
    identifier(payload, "mailboxOnboardingId", 160)?;
    positive_integer(payload, "mailboxOnboardingVersion")?;
    exact_string(payload, "provider", "IMAP")?;
    exact_string(payload, "authenticationMode", "MICROSOFT_OAUTH2")?;
    exact_string(
        payload,
        "oauthScopes",
        "https://outlook.office.com/IMAP.AccessAsUser.All https://outlook.office.com/SMTP.Send offline_access",
    )?;
    exact_string(payload, "oauthProtocol", "IMAP_SMTP_XOAUTH2")
}

fn password_provision(payload: &Map<String, Value>) -> Result<(), ContractError> {
    object(
        payload,
        &[
            "actorId",
            "mailboxOnboardingId",
            "mailboxOnboardingVersion",
            "provider",
            "authenticationMode",
            "idempotencyKey",
            "imap",
            "smtp",
        ],
        &[],
    )?;
    identifier(payload, "actorId", 128)?;
    identifier(payload, "mailboxOnboardingId", 160)?;
    positive_integer(payload, "mailboxOnboardingVersion")?;
    exact_string(payload, "provider", "IMAP")?;
    exact_string(payload, "authenticationMode", "PASSWORD")?;
    identifier(payload, "idempotencyKey", 192)?;
    password_protocol(payload, "imap")?;
    password_protocol(payload, "smtp")
}

fn password_protocol(payload: &Map<String, Value>, name: &str) -> Result<(), ContractError> {
    let protocol = payload
        .get(name)
        .and_then(Value::as_object)
        .ok_or(ContractError::InvalidField)?;
    object(
        protocol,
        &["host", "port", "transportSecurity", "username", "password"],
        &[],
    )?;
    bounded_string(protocol, "host", 253)?;
    positive_integer(protocol, "port")?;
    bounded_string(protocol, "transportSecurity", 32)?;
    bounded_string(protocol, "username", 512)?;
    bounded_string(protocol, "password", 8 * 1024)
}

fn object(
    payload: &Map<String, Value>,
    required: &[&str],
    optional: &[&str],
) -> Result<(), ContractError> {
    if required.iter().any(|name| !payload.contains_key(*name)) {
        return Err(ContractError::MissingField);
    }
    if payload
        .keys()
        .any(|name| !required.contains(&name.as_str()) && !optional.contains(&name.as_str()))
    {
        return Err(ContractError::UnexpectedField);
    }
    Ok(())
}

fn bounded_string(
    payload: &Map<String, Value>,
    name: &str,
    maximum: usize,
) -> Result<(), ContractError> {
    let value = payload
        .get(name)
        .and_then(Value::as_str)
        .ok_or(ContractError::InvalidField)?;
    if value.is_empty() || value.len() > maximum || value.chars().any(char::is_control) {
        return Err(ContractError::InvalidField);
    }
    Ok(())
}

fn optional_bounded_string(
    payload: &Map<String, Value>,
    name: &str,
    maximum: usize,
) -> Result<(), ContractError> {
    if payload.contains_key(name) {
        bounded_string(payload, name, maximum)
    } else {
        Ok(())
    }
}

fn identifier(
    payload: &Map<String, Value>,
    name: &str,
    maximum: usize,
) -> Result<(), ContractError> {
    bounded_string(payload, name, maximum)?;
    if payload[name].as_str().is_some_and(|value| {
        value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.' | b':'))
    }) {
        Ok(())
    } else {
        Err(ContractError::InvalidField)
    }
}

fn oauth_state(payload: &Map<String, Value>, name: &str) -> Result<(), ContractError> {
    bounded_string(payload, name, 256)?;
    let value = payload[name].as_str().ok_or(ContractError::InvalidField)?;
    let (tenant, state) = value.split_once('.').ok_or(ContractError::InvalidField)?;
    if tenant.is_empty()
        || !state.starts_with("state_")
        || !tenant
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b':'))
        || !state
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
    {
        return Err(ContractError::InvalidField);
    }
    Ok(())
}

fn positive_integer(payload: &Map<String, Value>, name: &str) -> Result<(), ContractError> {
    if payload
        .get(name)
        .and_then(Value::as_u64)
        .is_some_and(|v| v > 0)
    {
        Ok(())
    } else {
        Err(ContractError::InvalidField)
    }
}

fn exact_string(
    payload: &Map<String, Value>,
    name: &str,
    expected: &str,
) -> Result<(), ContractError> {
    if payload.get(name).and_then(Value::as_str) == Some(expected) {
        Ok(())
    } else {
        Err(ContractError::InvalidField)
    }
}

fn exact_bool(
    payload: &Map<String, Value>,
    name: &str,
    expected: bool,
) -> Result<(), ContractError> {
    if payload.get(name).and_then(Value::as_bool) == Some(expected) {
        Ok(())
    } else {
        Err(ContractError::InvalidField)
    }
}

#[cfg(test)]
mod tests {
    use super::{ContractError, validate_payload};
    use crate::model::ResolverRoute;
    use serde_json::json;

    #[test]
    fn cursor_contract_rejects_unknown_and_raw_cross_origin_state() -> Result<(), ContractError> {
        let valid = json!({
            "mailboxBindingId": "binding_01",
            "providerCursor": "https://graph.microsoft.com/v1.0/me/messages?$skiptoken=x"
        });
        assert!(
            validate_payload(
                ResolverRoute::MicrosoftGraphCursorStore,
                valid.as_object().ok_or(ContractError::InvalidField)?
            )
            .is_ok()
        );
        let unexpected = json!({
            "mailboxBindingId": "binding_01",
            "providerCursor": "cursor",
            "extra": true
        });
        assert_eq!(
            validate_payload(
                ResolverRoute::MicrosoftGraphCursorStore,
                unexpected.as_object().ok_or(ContractError::InvalidField)?
            ),
            Err(ContractError::UnexpectedField)
        );
        let cross_origin = json!({
            "mailboxBindingId": "binding_01",
            "providerCursor": "https://evil.example/v1.0/me/messages?$skiptoken=x"
        });
        assert_eq!(
            validate_payload(
                ResolverRoute::MicrosoftGraphCursorStore,
                cross_origin
                    .as_object()
                    .ok_or(ContractError::InvalidField)?
            ),
            Err(ContractError::InvalidField)
        );
        Ok(())
    }

    #[test]
    fn oauth_inspect_requires_tenant_partitioned_state() -> Result<(), ContractError> {
        let valid = json!({"oauthState": "tenant_01.state_0123456789abcdef"});
        validate_payload(
            ResolverRoute::GmailOAuthInspect,
            valid.as_object().ok_or(ContractError::InvalidField)?,
        )?;
        let invalid = json!({"oauthState": "state_0123456789abcdef"});
        assert_eq!(
            validate_payload(
                ResolverRoute::GmailOAuthInspect,
                invalid.as_object().ok_or(ContractError::InvalidField)?
            ),
            Err(ContractError::InvalidField)
        );
        Ok(())
    }
}
