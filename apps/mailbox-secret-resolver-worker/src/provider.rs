use serde::Deserialize;
use sha2::{Digest, Sha256};
use worker::{Env, Fetch, Headers, Method, Request, RequestInit};
use zeroize::{Zeroize, Zeroizing};

const GOOGLE_AUTHORIZATION_ENDPOINT: &str = "https://accounts.google.com/o/oauth2/v2/auth";
const GOOGLE_TOKEN_ENDPOINT: &str = "https://oauth2.googleapis.com/token";
const MICROSOFT_AUTHORIZATION_ENDPOINT: &str =
    "https://login.microsoftonline.com/common/oauth2/v2.0/authorize";
const MICROSOFT_TOKEN_ENDPOINT: &str = "https://login.microsoftonline.com/common/oauth2/v2.0/token";
const MAX_PROVIDER_RESPONSE_BYTES: usize = 32 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OAuthProvider {
    Google,
    Microsoft,
}

impl OAuthProvider {
    const fn client_id_var(self) -> &'static str {
        match self {
            Self::Google => "GOOGLE_OAUTH_CLIENT_ID",
            Self::Microsoft => "MICROSOFT_OAUTH_CLIENT_ID",
        }
    }

    const fn redirect_uri_var(self) -> &'static str {
        match self {
            Self::Google => "GOOGLE_OAUTH_REDIRECT_URI",
            Self::Microsoft => "MICROSOFT_OAUTH_REDIRECT_URI",
        }
    }

    const fn client_secret_name(self) -> &'static str {
        match self {
            Self::Google => "GOOGLE_OAUTH_CLIENT_SECRET",
            Self::Microsoft => "MICROSOFT_OAUTH_CLIENT_SECRET",
        }
    }

    const fn authorization_endpoint(self) -> &'static str {
        match self {
            Self::Google => GOOGLE_AUTHORIZATION_ENDPOINT,
            Self::Microsoft => MICROSOFT_AUTHORIZATION_ENDPOINT,
        }
    }

    const fn token_endpoint(self) -> &'static str {
        match self {
            Self::Google => GOOGLE_TOKEN_ENDPOINT,
            Self::Microsoft => MICROSOFT_TOKEN_ENDPOINT,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProviderError {
    InvalidConfiguration,
    InvalidRequest,
    DependencyUnavailable,
    ProviderRejected,
    CredentialRejected,
    ResponseTooLarge,
    InvalidResponse,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TokenGrantKind {
    AuthorizationCode,
    RefreshToken,
}

#[derive(Deserialize)]
pub struct ProviderTokenSet {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_in: u64,
    pub scope: Option<String>,
    pub token_type: String,
    pub id_token: Option<String>,
}

impl ProviderTokenSet {
    pub fn validate(&self) -> Result<(), ProviderError> {
        if self.access_token.is_empty()
            || self.access_token.len() > 16 * 1024
            || self.expires_in == 0
            || self.expires_in > 31 * 24 * 60 * 60
            || self.token_type != "Bearer"
            || self
                .refresh_token
                .as_deref()
                .is_some_and(|value| value.is_empty() || value.len() > 16 * 1024)
            || self
                .id_token
                .as_deref()
                .is_some_and(|value| value.is_empty() || value.len() > 16 * 1024)
        {
            return Err(ProviderError::InvalidResponse);
        }
        Ok(())
    }
}

impl Drop for ProviderTokenSet {
    fn drop(&mut self) {
        self.access_token.zeroize();
        if let Some(value) = self.refresh_token.as_mut() {
            value.zeroize();
        }
        if let Some(value) = self.id_token.as_mut() {
            value.zeroize();
        }
    }
}

#[derive(Deserialize)]
struct OAuthErrorDocument {
    error: String,
}

impl Drop for OAuthErrorDocument {
    fn drop(&mut self) {
        self.error.zeroize();
    }
}

pub fn authorization_url(
    env: &Env,
    provider: OAuthProvider,
    state: &str,
    scopes: &str,
    pkce_verifier: Option<&str>,
    include_granted_scopes: bool,
) -> Result<String, ProviderError> {
    validate_oauth_input(state, scopes)?;
    let client_id = env
        .var(provider.client_id_var())
        .map_err(|_| ProviderError::InvalidConfiguration)?
        .to_string();
    let redirect_uri = env
        .var(provider.redirect_uri_var())
        .map_err(|_| ProviderError::InvalidConfiguration)?
        .to_string();
    validate_redirect_uri(&redirect_uri)?;
    let mut parameters = vec![
        ("client_id", client_id.as_str()),
        ("redirect_uri", redirect_uri.as_str()),
        ("response_type", "code"),
        ("scope", scopes),
        ("state", state),
    ];
    let challenge;
    if let Some(verifier) = pkce_verifier {
        validate_pkce_verifier(verifier)?;
        challenge = base64url_unpadded(Sha256::digest(verifier.as_bytes()).as_slice());
        parameters.push(("code_challenge", challenge.as_str()));
        parameters.push(("code_challenge_method", "S256"));
    }
    if provider == OAuthProvider::Google {
        parameters.push(("access_type", "offline"));
        parameters.push(("prompt", "consent"));
        if include_granted_scopes {
            parameters.push(("include_granted_scopes", "true"));
        }
    }
    Ok(format!(
        "{}?{}",
        provider.authorization_endpoint(),
        form_encode(&parameters)
    ))
}

pub async fn exchange_authorization_code(
    env: &Env,
    provider: OAuthProvider,
    authorization_code: &str,
    pkce_verifier: Option<&str>,
) -> Result<ProviderTokenSet, ProviderError> {
    if authorization_code.is_empty() || authorization_code.len() > 8 * 1024 {
        return Err(ProviderError::InvalidRequest);
    }
    if let Some(verifier) = pkce_verifier {
        validate_pkce_verifier(verifier)?;
    }
    let client_id = env
        .var(provider.client_id_var())
        .map_err(|_| ProviderError::InvalidConfiguration)?
        .to_string();
    let redirect_uri = env
        .var(provider.redirect_uri_var())
        .map_err(|_| ProviderError::InvalidConfiguration)?
        .to_string();
    validate_redirect_uri(&redirect_uri)?;
    let secret = Zeroizing::new(
        env.secret(provider.client_secret_name())
            .map_err(|_| ProviderError::InvalidConfiguration)?
            .to_string(),
    );
    if secret.is_empty() || secret.len() > 8 * 1024 {
        return Err(ProviderError::InvalidConfiguration);
    }
    let mut parameters = vec![
        ("client_id", client_id.as_str()),
        ("client_secret", secret.as_str()),
        ("code", authorization_code),
        ("grant_type", "authorization_code"),
        ("redirect_uri", redirect_uri.as_str()),
    ];
    if let Some(verifier) = pkce_verifier {
        parameters.push(("code_verifier", verifier));
    }
    send_token_request(
        provider.token_endpoint(),
        &parameters,
        TokenGrantKind::AuthorizationCode,
    )
    .await
}

pub async fn refresh_access_token(
    env: &Env,
    provider: OAuthProvider,
    refresh_token: &str,
    scopes: Option<&str>,
) -> Result<ProviderTokenSet, ProviderError> {
    if refresh_token.is_empty() || refresh_token.len() > 16 * 1024 {
        return Err(ProviderError::InvalidRequest);
    }
    let client_id = env
        .var(provider.client_id_var())
        .map_err(|_| ProviderError::InvalidConfiguration)?
        .to_string();
    let secret = Zeroizing::new(
        env.secret(provider.client_secret_name())
            .map_err(|_| ProviderError::InvalidConfiguration)?
            .to_string(),
    );
    if secret.is_empty() || secret.len() > 8 * 1024 {
        return Err(ProviderError::InvalidConfiguration);
    }
    let mut parameters = vec![
        ("client_id", client_id.as_str()),
        ("client_secret", secret.as_str()),
        ("refresh_token", refresh_token),
        ("grant_type", "refresh_token"),
    ];
    if let Some(value) = scopes {
        parameters.push(("scope", value));
    }
    send_token_request(
        provider.token_endpoint(),
        &parameters,
        TokenGrantKind::RefreshToken,
    )
    .await
}

async fn send_token_request(
    endpoint: &str,
    parameters: &[(&str, &str)],
    grant_kind: TokenGrantKind,
) -> Result<ProviderTokenSet, ProviderError> {
    let mut body = form_encode(parameters);
    if body.len() > 32 * 1024 {
        body.zeroize();
        return Err(ProviderError::InvalidRequest);
    }
    let headers = Headers::new();
    headers
        .set("accept", "application/json")
        .map_err(|_| ProviderError::InvalidRequest)?;
    headers
        .set("content-type", "application/x-www-form-urlencoded")
        .map_err(|_| ProviderError::InvalidRequest)?;
    let mut init = RequestInit::new();
    init.with_method(Method::Post)
        .with_headers(headers)
        .with_body(Some(body.as_str().into()));
    body.zeroize();
    let request =
        Request::new_with_init(endpoint, &init).map_err(|_| ProviderError::InvalidRequest)?;
    let mut response = Fetch::Request(request)
        .send()
        .await
        .map_err(|_| ProviderError::DependencyUnavailable)?;
    match response.status_code() {
        200 => parse_token_success(&mut response).await,
        400 | 401 | 403 => {
            let error = parse_token_error(&mut response, grant_kind).await?;
            Err(error)
        }
        408 | 425 | 429 | 500..=599 => Err(ProviderError::DependencyUnavailable),
        _ => Err(ProviderError::InvalidResponse),
    }
}

async fn parse_token_success(
    response: &mut worker::Response,
) -> Result<ProviderTokenSet, ProviderError> {
    let mut bytes = read_provider_response(response).await?;
    let parsed = serde_json::from_slice::<ProviderTokenSet>(&bytes);
    bytes.zeroize();
    let tokens = parsed.map_err(|_| ProviderError::InvalidResponse)?;
    tokens.validate()?;
    Ok(tokens)
}

async fn parse_token_error(
    response: &mut worker::Response,
    grant_kind: TokenGrantKind,
) -> Result<ProviderError, ProviderError> {
    let mut bytes = read_provider_response(response).await?;
    let parsed = serde_json::from_slice::<OAuthErrorDocument>(&bytes);
    bytes.zeroize();
    let document = parsed.map_err(|_| ProviderError::InvalidResponse)?;
    if document.error.is_empty()
        || document.error.len() > 128
        || !document
            .error
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
    {
        return Err(ProviderError::InvalidResponse);
    }
    Ok(classify_oauth_error(grant_kind, &document.error))
}

async fn read_provider_response(
    response: &mut worker::Response,
) -> Result<Vec<u8>, ProviderError> {
    if content_length_exceeds(response, MAX_PROVIDER_RESPONSE_BYTES)? {
        return Err(ProviderError::ResponseTooLarge);
    }
    let mut bytes = response
        .bytes()
        .await
        .map_err(|_| ProviderError::DependencyUnavailable)?;
    if bytes.is_empty() || bytes.len() > MAX_PROVIDER_RESPONSE_BYTES {
        bytes.zeroize();
        return Err(ProviderError::ResponseTooLarge);
    }
    Ok(bytes)
}

const fn classify_oauth_error(grant_kind: TokenGrantKind, error: &str) -> ProviderError {
    if matches!(grant_kind, TokenGrantKind::RefreshToken) && matches!(error, "invalid_grant") {
        return ProviderError::CredentialRejected;
    }
    match error {
        "invalid_client" | "unauthorized_client" | "invalid_scope" => {
            ProviderError::InvalidConfiguration
        }
        "invalid_request" | "unsupported_grant_type" => ProviderError::InvalidRequest,
        "server_error" | "temporarily_unavailable" => ProviderError::DependencyUnavailable,
        _ => ProviderError::ProviderRejected,
    }
}

fn content_length_exceeds(
    response: &worker::Response,
    maximum: usize,
) -> Result<bool, ProviderError> {
    let value = response
        .headers()
        .get("content-length")
        .map_err(|_| ProviderError::InvalidResponse)?;
    let Some(value) = value else {
        return Ok(false);
    };
    Ok(value
        .parse::<usize>()
        .map_err(|_| ProviderError::InvalidResponse)?
        > maximum)
}

fn validate_oauth_input(state: &str, scopes: &str) -> Result<(), ProviderError> {
    if state.len() < 22
        || state.len() > 256
        || state.chars().any(char::is_control)
        || scopes.is_empty()
        || scopes.len() > 2 * 1024
        || scopes.chars().any(char::is_control)
    {
        return Err(ProviderError::InvalidRequest);
    }
    Ok(())
}

fn validate_pkce_verifier(verifier: &str) -> Result<(), ProviderError> {
    if (43..=128).contains(&verifier.len())
        && verifier
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~'))
    {
        Ok(())
    } else {
        Err(ProviderError::InvalidRequest)
    }
}

fn validate_redirect_uri(value: &str) -> Result<(), ProviderError> {
    if value.starts_with("https://")
        && value.len() <= 2 * 1024
        && !value.contains('#')
        && !value.chars().any(char::is_control)
    {
        Ok(())
    } else {
        Err(ProviderError::InvalidConfiguration)
    }
}

fn form_encode(parameters: &[(&str, &str)]) -> String {
    parameters
        .iter()
        .map(|(name, value)| format!("{}={}", percent_encode(name), percent_encode(value)))
        .collect::<Vec<_>>()
        .join("&")
}

fn percent_encode(value: &str) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut output = String::with_capacity(value.len());
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            output.push(char::from(byte));
        } else {
            output.push('%');
            output.push(char::from(HEX[usize::from(byte >> 4)]));
            output.push(char::from(HEX[usize::from(byte & 0x0f)]));
        }
    }
    output
}

fn base64url_unpadded(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut output = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let first = chunk[0];
        let second = chunk.get(1).copied().unwrap_or_default();
        let third = chunk.get(2).copied().unwrap_or_default();
        output.push(char::from(ALPHABET[usize::from(first >> 2)]));
        output.push(char::from(
            ALPHABET[usize::from(((first & 0x03) << 4) | (second >> 4))],
        ));
        if chunk.len() > 1 {
            output.push(char::from(
                ALPHABET[usize::from(((second & 0x0f) << 2) | (third >> 6))],
            ));
        }
        if chunk.len() > 2 {
            output.push(char::from(ALPHABET[usize::from(third & 0x3f)]));
        }
    }
    output
}

#[cfg(test)]
mod tests {
    use super::{
        ProviderError, TokenGrantKind, base64url_unpadded, classify_oauth_error, percent_encode,
        validate_pkce_verifier,
    };
    use sha2::{Digest, Sha256};

    #[test]
    fn pkce_s256_matches_rfc_7636_vector() {
        let verifier = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
        assert!(validate_pkce_verifier(verifier).is_ok());
        assert_eq!(
            base64url_unpadded(Sha256::digest(verifier.as_bytes()).as_slice()),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
    }

    #[test]
    fn oauth_form_encoding_is_rfc3986_and_never_uses_plus_for_space() {
        assert_eq!(percent_encode("a b+c"), "a%20b%2Bc");
    }

    #[test]
    fn refresh_invalid_grant_is_the_only_credential_rejection_class() {
        assert_eq!(
            classify_oauth_error(TokenGrantKind::RefreshToken, "invalid_grant"),
            ProviderError::CredentialRejected
        );
        assert_eq!(
            classify_oauth_error(TokenGrantKind::AuthorizationCode, "invalid_grant"),
            ProviderError::ProviderRejected
        );
        assert_eq!(
            classify_oauth_error(TokenGrantKind::RefreshToken, "invalid_client"),
            ProviderError::InvalidConfiguration
        );
        assert_eq!(
            classify_oauth_error(TokenGrantKind::RefreshToken, "invalid_scope"),
            ProviderError::InvalidConfiguration
        );
    }
}
