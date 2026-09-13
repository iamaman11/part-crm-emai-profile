use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

pub const DEVICE_PAIRING_COLLECTION_PATH_TEMPLATE: &str =
    "/api/v1/tenants/{tenantId}/device-pairings";
pub const DEVICE_PAIRING_AUTHORIZATION_PATH_TEMPLATE: &str =
    "/api/v1/tenants/{tenantId}/device-pairings/authorizations";
pub const DEVICE_PAIRING_COMPLETION_PATH_TEMPLATE: &str =
    "/api/v1/tenants/{tenantId}/device-pairings/completions";
pub const DEVICE_SESSION_CHALLENGE_PATH_TEMPLATE: &str =
    "/api/v1/tenants/{tenantId}/devices/{deviceId}/session-challenges";
pub const DEVICE_SESSION_COLLECTION_PATH_TEMPLATE: &str =
    "/api/v1/tenants/{tenantId}/devices/{deviceId}/sessions";

pub const OPAQUE_TOKEN_HEX_LENGTH: usize = 64;
pub const P256_SPKI_DER_HEX_LENGTH: usize = 182;
pub const P256_SIGNATURE_P1363_HEX_LENGTH: usize = 128;
pub const DEVICE_PROOF_NONCE_HEX_LENGTH: usize = 64;

#[derive(Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DevicePairingCreateRequest {
    device_id: String,
    public_key_spki_der_hex: String,
}

impl DevicePairingCreateRequest {
    #[must_use]
    pub fn device_id(&self) -> &str {
        &self.device_id
    }

    #[must_use]
    pub fn public_key_spki_der_hex(&self) -> &str {
        &self.public_key_spki_der_hex
    }
}

#[derive(Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DevicePairingCreateProjection {
    pub pairing_token: String,
    pub expires_at_ms: u64,
}

#[derive(Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DevicePairingAuthorizeRequest {
    pairing_token: String,
}

impl DevicePairingAuthorizeRequest {
    #[must_use]
    pub fn pairing_token(&self) -> &str {
        &self.pairing_token
    }
}

#[derive(Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DeviceProofChallengeProjection {
    pub challenge_token: String,
    pub device_id: String,
    pub nonce_hex: String,
    pub expires_at_ms: u64,
}

#[derive(Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DevicePairingCompleteRequest {
    pairing_token: String,
    challenge_token: String,
    signature_p1363_hex: String,
}

impl DevicePairingCompleteRequest {
    #[must_use]
    pub fn pairing_token(&self) -> &str {
        &self.pairing_token
    }

    #[must_use]
    pub fn challenge_token(&self) -> &str {
        &self.challenge_token
    }

    #[must_use]
    pub fn signature_p1363_hex(&self) -> &str {
        &self.signature_p1363_hex
    }
}

#[derive(Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DeviceSessionChallengeRequest {}

#[derive(Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DeviceSessionRenewRequest {
    challenge_token: String,
    signature_p1363_hex: String,
}

impl DeviceSessionRenewRequest {
    #[must_use]
    pub fn challenge_token(&self) -> &str {
        &self.challenge_token
    }

    #[must_use]
    pub fn signature_p1363_hex(&self) -> &str {
        &self.signature_p1363_hex
    }
}

#[derive(Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DeviceApplicationSessionProjection {
    pub session_token: String,
    pub device_id: String,
    pub expires_at_ms: u64,
}

#[must_use]
pub fn openapi_fragment() -> Value {
    json!({
        "paths": {
            DEVICE_PAIRING_COLLECTION_PATH_TEMPLATE: {
                "post": {
                    "operationId": "createDevicePairing",
                    "security": [],
                    "parameters": [tenant_path_parameter(), correlation_header()],
                    "requestBody": json_request("DevicePairingCreateRequest"),
                    "responses": {
                        "201": json_response("DevicePairingCreateProjection"),
                        "400": problem_response(), "409": problem_response(),
                        "500": problem_response(), "503": problem_response()
                    }
                }
            },
            DEVICE_PAIRING_AUTHORIZATION_PATH_TEMPLATE: {
                "post": {
                    "operationId": "authorizeDevicePairing",
                    "security": [{"cloudflareAccessJwt": []}],
                    "parameters": [tenant_path_parameter(), correlation_header()],
                    "requestBody": json_request("DevicePairingAuthorizeRequest"),
                    "responses": {
                        "200": json_response("DeviceProofChallengeProjection"),
                        "400": problem_response(), "404": problem_response(), "409": problem_response(),
                        "500": problem_response(), "503": problem_response()
                    }
                }
            },
            DEVICE_PAIRING_COMPLETION_PATH_TEMPLATE: {
                "post": {
                    "operationId": "completeDevicePairing",
                    "security": [],
                    "parameters": [tenant_path_parameter(), correlation_header()],
                    "requestBody": json_request("DevicePairingCompleteRequest"),
                    "responses": {
                        "201": json_response("DeviceApplicationSessionProjection"),
                        "400": problem_response(), "404": problem_response(), "409": problem_response(),
                        "500": problem_response(), "503": problem_response()
                    }
                }
            },
            DEVICE_SESSION_CHALLENGE_PATH_TEMPLATE: {
                "post": {
                    "operationId": "createDeviceSessionChallenge",
                    "security": [],
                    "parameters": [tenant_path_parameter(), device_path_parameter(), correlation_header()],
                    "requestBody": json_request("DeviceSessionChallengeRequest"),
                    "responses": {
                        "201": json_response("DeviceProofChallengeProjection"),
                        "400": problem_response(), "404": problem_response(),
                        "500": problem_response(), "503": problem_response()
                    }
                }
            },
            DEVICE_SESSION_COLLECTION_PATH_TEMPLATE: {
                "post": {
                    "operationId": "renewDeviceApplicationSession",
                    "security": [],
                    "parameters": [tenant_path_parameter(), device_path_parameter(), correlation_header()],
                    "requestBody": json_request("DeviceSessionRenewRequest"),
                    "responses": {
                        "201": json_response("DeviceApplicationSessionProjection"),
                        "400": problem_response(), "404": problem_response(), "409": problem_response(),
                        "500": problem_response(), "503": problem_response()
                    }
                }
            }
        },
        "components": {
            "securitySchemes": {
                "cloudflareAccessJwt": {
                    "type": "apiKey", "in": "header", "name": "Cf-Access-Jwt-Assertion"
                }
            },
            "schemas": {
                "DevicePairingCreateRequest": {
                    "type": "object", "additionalProperties": false,
                    "required": ["deviceId", "publicKeySpkiDerHex"],
                    "properties": {
                        "deviceId": {"type": "string", "minLength": 8, "maxLength": 96, "pattern": "^[A-Za-z0-9_-]+$"},
                        "publicKeySpkiDerHex": {"type": "string", "minLength": P256_SPKI_DER_HEX_LENGTH, "maxLength": P256_SPKI_DER_HEX_LENGTH, "pattern": "^[0-9a-f]+$"}
                    }
                },
                "DevicePairingCreateProjection": {
                    "type": "object", "additionalProperties": false,
                    "required": ["pairingToken", "expiresAtMs"],
                    "properties": {
                        "pairingToken": token_schema(),
                        "expiresAtMs": {"type": "integer", "minimum": 1}
                    }
                },
                "DevicePairingAuthorizeRequest": {
                    "type": "object", "additionalProperties": false,
                    "required": ["pairingToken"],
                    "properties": {"pairingToken": token_schema()}
                },
                "DeviceProofChallengeProjection": {
                    "type": "object", "additionalProperties": false,
                    "required": ["challengeToken", "deviceId", "nonceHex", "expiresAtMs"],
                    "properties": {
                        "challengeToken": token_schema(),
                        "deviceId": {"type": "string"},
                        "nonceHex": {"type": "string", "minLength": DEVICE_PROOF_NONCE_HEX_LENGTH, "maxLength": DEVICE_PROOF_NONCE_HEX_LENGTH, "pattern": "^[0-9a-f]+$"},
                        "expiresAtMs": {"type": "integer", "minimum": 1}
                    }
                },
                "DevicePairingCompleteRequest": {
                    "type": "object", "additionalProperties": false,
                    "required": ["pairingToken", "challengeToken", "signatureP1363Hex"],
                    "properties": {
                        "pairingToken": token_schema(),
                        "challengeToken": token_schema(),
                        "signatureP1363Hex": signature_schema()
                    }
                },
                "DeviceSessionChallengeRequest": {
                    "type": "object", "additionalProperties": false, "properties": {}
                },
                "DeviceSessionRenewRequest": {
                    "type": "object", "additionalProperties": false,
                    "required": ["challengeToken", "signatureP1363Hex"],
                    "properties": {
                        "challengeToken": token_schema(),
                        "signatureP1363Hex": signature_schema()
                    }
                },
                "DeviceApplicationSessionProjection": {
                    "type": "object", "additionalProperties": false,
                    "required": ["sessionToken", "deviceId", "expiresAtMs"],
                    "properties": {
                        "sessionToken": token_schema(),
                        "deviceId": {"type": "string"},
                        "expiresAtMs": {"type": "integer", "minimum": 1}
                    }
                }
            }
        }
    })
}

fn tenant_path_parameter() -> Value {
    path_parameter("tenantId")
}

fn device_path_parameter() -> Value {
    path_parameter("deviceId")
}

fn path_parameter(name: &str) -> Value {
    json!({"name": name, "in": "path", "required": true, "schema": {"type": "string"}})
}

fn correlation_header() -> Value {
    json!({
        "name": "X-Correlation-Id", "in": "header", "required": true,
        "schema": {"type": "string"}
    })
}

fn token_schema() -> Value {
    json!({
        "type": "string", "minLength": OPAQUE_TOKEN_HEX_LENGTH,
        "maxLength": OPAQUE_TOKEN_HEX_LENGTH, "pattern": "^[0-9a-f]{64}$"
    })
}

fn signature_schema() -> Value {
    json!({
        "type": "string", "minLength": P256_SIGNATURE_P1363_HEX_LENGTH,
        "maxLength": P256_SIGNATURE_P1363_HEX_LENGTH, "pattern": "^[0-9a-f]{128}$"
    })
}

fn schema_ref(name: &str) -> Value {
    json!({"$ref": format!("#/components/schemas/{name}")})
}

fn json_request(schema: &str) -> Value {
    json!({"required": true, "content": {"application/json": {"schema": schema_ref(schema)}}})
}

fn json_response(schema: &str) -> Value {
    json!({"description": "Successful response", "content": {"application/json": {"schema": schema_ref(schema)}}})
}

fn problem_response() -> Value {
    json!({
        "description": "Problem response",
        "content": {"application/problem+json": {"schema": schema_ref("ProblemPayload")}}
    })
}

#[cfg(test)]
mod tests {
    use super::{
        DevicePairingAuthorizeRequest, DevicePairingCompleteRequest, DevicePairingCreateRequest,
        DeviceSessionChallengeRequest, DeviceSessionRenewRequest, OPAQUE_TOKEN_HEX_LENGTH,
        P256_SIGNATURE_P1363_HEX_LENGTH, P256_SPKI_DER_HEX_LENGTH,
    };

    #[test]
    fn transport_is_strict_and_keeps_raw_secrets_out_of_paths() {
        let token = "a".repeat(OPAQUE_TOKEN_HEX_LENGTH);
        let public_key = "b".repeat(P256_SPKI_DER_HEX_LENGTH);
        let signature = "c".repeat(P256_SIGNATURE_P1363_HEX_LENGTH);

        let create =
            format!(r#"{{"deviceId":"device_01JPAIR","publicKeySpkiDerHex":"{public_key}"}}"#);
        assert!(serde_json::from_str::<DevicePairingCreateRequest>(&create).is_ok());
        assert!(serde_json::from_str::<DeviceSessionChallengeRequest>("{}").is_ok());
        assert!(
            serde_json::from_str::<DeviceSessionChallengeRequest>(r#"{"deviceId":"x"}"#).is_err()
        );

        let authorize = format!(r#"{{"pairingToken":"{token}"}}"#);
        assert!(serde_json::from_str::<DevicePairingAuthorizeRequest>(&authorize).is_ok());
        let complete = format!(
            r#"{{"pairingToken":"{token}","challengeToken":"{token}","signatureP1363Hex":"{signature}"}}"#
        );
        assert!(serde_json::from_str::<DevicePairingCompleteRequest>(&complete).is_ok());
        let renew = format!(r#"{{"challengeToken":"{token}","signatureP1363Hex":"{signature}"}}"#);
        assert!(serde_json::from_str::<DeviceSessionRenewRequest>(&renew).is_ok());

        for secret in ["pairingToken", "challengeToken", "sessionToken"] {
            for path in [
                super::DEVICE_PAIRING_COLLECTION_PATH_TEMPLATE,
                super::DEVICE_PAIRING_AUTHORIZATION_PATH_TEMPLATE,
                super::DEVICE_PAIRING_COMPLETION_PATH_TEMPLATE,
                super::DEVICE_SESSION_CHALLENGE_PATH_TEMPLATE,
                super::DEVICE_SESSION_COLLECTION_PATH_TEMPLATE,
            ] {
                assert!(!path.contains(secret));
            }
        }
    }

    #[test]
    fn requests_reject_caller_owned_identity_and_unknown_fields() {
        let public_key = "b".repeat(P256_SPKI_DER_HEX_LENGTH);
        for forbidden in ["tenantId", "actorId", "authEpoch", "bindingVersion"] {
            let body = format!(
                r#"{{"deviceId":"device_01JPAIR","publicKeySpkiDerHex":"{public_key}","{forbidden}":"caller-owned"}}"#
            );
            assert!(serde_json::from_str::<DevicePairingCreateRequest>(&body).is_err());
        }
    }
}
