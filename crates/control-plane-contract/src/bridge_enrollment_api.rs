use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

pub const BRIDGE_ENROLLMENT_ISSUE_PATH_TEMPLATE: &str =
    "/api/v1/tenants/{tenantId}/bridge-enrollment/authorities";
pub const BRIDGE_ENROLLMENT_REDEEM_PATH_TEMPLATE: &str =
    "/api/v1/tenants/{tenantId}/bridge-enrollment/redemptions";
pub const MAX_BRIDGE_ENROLLMENT_CSR_DER_HEX_LENGTH: usize = 32 * 1024;

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BridgeEnrollmentIssueRequest {}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BridgeEnrollmentIssueProjection {
    pub claim_code: String,
    pub expires_at_ms: u64,
    pub device_id: String,
}

#[derive(Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BridgeEnrollmentRedemptionRequest {
    claim_code: String,
    csr_der_hex: String,
}

impl BridgeEnrollmentRedemptionRequest {
    #[must_use]
    pub fn claim_code(&self) -> &str {
        &self.claim_code
    }

    #[must_use]
    pub fn csr_der_hex(&self) -> &str {
        &self.csr_der_hex
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BridgeEnrollmentRedemptionProjection {
    pub device_id: String,
    pub certificate_sha256: String,
    pub leaf_certificate_der_hex: String,
    pub certificate_chain_der_hex: Vec<String>,
}

#[must_use]
pub fn openapi_fragment() -> Value {
    json!({
        "paths": {
            "/api/v1/tenants/{tenantId}/bridge-enrollment/authorities": {
                "post": {
                    "operationId": "issueBridgeEnrollmentAuthority",
                    "security": [{"cloudflareAccessJwt": []}],
                    "parameters": [
                        {"$ref": "#/components/parameters/TenantPath"},
                        {"$ref": "#/components/parameters/CorrelationHeader"},
                        {"$ref": "#/components/parameters/IdempotencyHeader"}
                    ],
                    "requestBody": {
                        "required": true,
                        "content": {"application/json": {"schema": {"$ref": "#/components/schemas/BridgeEnrollmentIssueRequest"}}}
                    },
                    "responses": {
                        "200": {"description": "Idempotent enrollment authority replay"},
                        "201": {"description": "Enrollment authority issued"},
                        "400": {"$ref": "#/components/responses/InvalidRequest"},
                        "404": {"$ref": "#/components/responses/NeutralNotFound"},
                        "409": {"$ref": "#/components/responses/Conflict"},
                        "500": {"$ref": "#/components/responses/InternalFailure"},
                        "503": {"$ref": "#/components/responses/DependencyUnavailable"}
                    }
                }
            },
            "/api/v1/tenants/{tenantId}/bridge-enrollment/redemptions": {
                "post": {
                    "operationId": "redeemBridgeEnrollmentAuthority",
                    "security": [{"cloudflareAccessJwt": []}],
                    "parameters": [
                        {"$ref": "#/components/parameters/TenantPath"},
                        {"$ref": "#/components/parameters/CorrelationHeader"}
                    ],
                    "requestBody": {
                        "required": true,
                        "content": {"application/json": {"schema": {"$ref": "#/components/schemas/BridgeEnrollmentRedemptionRequest"}}}
                    },
                    "responses": {
                        "200": {"description": "Public Bridge client certificate material"},
                        "400": {"$ref": "#/components/responses/InvalidRequest"},
                        "404": {"$ref": "#/components/responses/NeutralNotFound"},
                        "409": {"$ref": "#/components/responses/Conflict"},
                        "500": {"$ref": "#/components/responses/InternalFailure"},
                        "503": {"$ref": "#/components/responses/DependencyUnavailable"}
                    }
                }
            }
        },
        "components": {
            "schemas": {
                "BridgeEnrollmentIssueRequest": {
                    "type": "object", "additionalProperties": false, "properties": {}
                },
                "BridgeEnrollmentIssueProjection": {
                    "type": "object", "additionalProperties": false,
                    "required": ["claimCode", "expiresAtMs", "deviceId"],
                    "properties": {
                        "claimCode": {"type": "string", "minLength": 64, "maxLength": 64, "pattern": "^[0-9a-f]{64}$"},
                        "expiresAtMs": {"type": "integer", "minimum": 1},
                        "deviceId": {"type": "string"}
                    }
                },
                "BridgeEnrollmentRedemptionRequest": {
                    "type": "object", "additionalProperties": false,
                    "required": ["claimCode", "csrDerHex"],
                    "properties": {
                        "claimCode": {"type": "string", "minLength": 64, "maxLength": 64, "pattern": "^[0-9a-f]{64}$"},
                        "csrDerHex": {"type": "string", "minLength": 4, "maxLength": MAX_BRIDGE_ENROLLMENT_CSR_DER_HEX_LENGTH, "pattern": "^(?:[0-9a-f]{2})+$"}
                    }
                },
                "BridgeEnrollmentRedemptionProjection": {
                    "type": "object", "additionalProperties": false,
                    "required": ["deviceId", "certificateSha256", "leafCertificateDerHex", "certificateChainDerHex"],
                    "properties": {
                        "deviceId": {"type": "string"},
                        "certificateSha256": {"type": "string", "minLength": 64, "maxLength": 64, "pattern": "^[0-9a-f]{64}$"},
                        "leafCertificateDerHex": {"type": "string"},
                        "certificateChainDerHex": {"type": "array", "maxItems": 8, "items": {"type": "string"}}
                    }
                }
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::{
        BRIDGE_ENROLLMENT_ISSUE_PATH_TEMPLATE, BRIDGE_ENROLLMENT_REDEEM_PATH_TEMPLATE,
        BridgeEnrollmentIssueRequest, BridgeEnrollmentRedemptionRequest, openapi_fragment,
    };
    use crate::{RouteClass, classify_route};

    #[test]
    fn exact_human_enrollment_routes_are_authenticated_and_versioned() {
        for template in [
            BRIDGE_ENROLLMENT_ISSUE_PATH_TEMPLATE,
            BRIDGE_ENROLLMENT_REDEEM_PATH_TEMPLATE,
        ] {
            let path = template.replace("{tenantId}", "tenant_01JENROLL");
            assert_eq!(
                classify_route("POST", &path),
                RouteClass::BridgeEnrollmentApi
            );
            for method in ["GET", "PUT", "DELETE"] {
                assert_eq!(
                    classify_route(method, &path),
                    RouteClass::DynamicRouteNotFound
                );
            }
        }
    }

    #[test]
    fn issue_request_accepts_no_caller_owned_identity() {
        assert!(serde_json::from_str::<BridgeEnrollmentIssueRequest>("{}").is_ok());
        for forbidden in ["tenantId", "actorId", "deviceId", "csrSha256"] {
            let body = format!(r#"{{"{forbidden}":"caller-owned"}}"#);
            assert!(serde_json::from_str::<BridgeEnrollmentIssueRequest>(&body).is_err());
        }
    }

    #[test]
    fn redemption_request_contains_only_claim_and_exact_csr_transport()
    -> Result<(), Box<dyn std::error::Error>> {
        let valid = format!(
            r#"{{"claimCode":"{}","csrDerHex":"3000"}}"#,
            "a".repeat(64)
        );
        let request = serde_json::from_str::<BridgeEnrollmentRedemptionRequest>(&valid)?;
        assert_eq!(request.claim_code().len(), 64);
        assert_eq!(request.csr_der_hex(), "3000");
        for forbidden in ["tenantId", "actorId", "deviceId", "csrSha256"] {
            let invalid = valid.replace(
                "}",
                &format!(r#","{forbidden}":"caller-owned"}}"#),
            );
            assert!(serde_json::from_str::<BridgeEnrollmentRedemptionRequest>(&invalid).is_err());
        }
        Ok(())
    }

    #[test]
    fn fragment_has_no_caller_identity_or_digest_fields() {
        let fragment = openapi_fragment();
        let request = &fragment["components"]["schemas"]["BridgeEnrollmentRedemptionRequest"]
            ["properties"];
        assert!(request.get("claimCode").is_some());
        assert!(request.get("csrDerHex").is_some());
        for forbidden in ["tenantId", "actorId", "deviceId", "csrSha256"] {
            assert!(request.get(forbidden).is_none());
        }
    }
}
