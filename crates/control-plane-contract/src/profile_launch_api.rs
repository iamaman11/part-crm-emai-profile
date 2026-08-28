use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::{Value, json};

const CLAIM_URI_PREFIX: &str = "profilebridge://claim/";
const CLAIM_CODE_MIN_LENGTH: usize = 24;
const CLAIM_CODE_MAX_LENGTH: usize = 96;
const CLAIM_URI_PATTERN: &str = "^profilebridge://claim/[A-Za-z0-9_-]{24,96}$";

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileLaunchProjection {
    pub launch_uri: String,
    pub expires_at_ms: u64,
}

impl<'de> Deserialize<'de> for ProfileLaunchProjection {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase", deny_unknown_fields)]
        struct WireProjection {
            launch_uri: String,
            expires_at_ms: u64,
        }

        let wire = WireProjection::deserialize(deserializer)?;
        if !valid_launch_uri(&wire.launch_uri) {
            return Err(D::Error::custom("invalid Profile Bridge launch URI"));
        }
        if wire.expires_at_ms == 0 {
            return Err(D::Error::custom("launch authority expiry must be positive"));
        }
        Ok(Self {
            launch_uri: wire.launch_uri,
            expires_at_ms: wire.expires_at_ms,
        })
    }
}

fn valid_launch_uri(value: &str) -> bool {
    let Some(code) = value.strip_prefix(CLAIM_URI_PREFIX) else {
        return false;
    };
    (CLAIM_CODE_MIN_LENGTH..=CLAIM_CODE_MAX_LENGTH).contains(&code.len())
        && code
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-')
}

#[must_use]
pub fn openapi_fragment() -> Value {
    json!({
        "paths": {
            "/api/v1/tenants/{tenantId}/profiles/{profileId}/launch": {
                "post": {
                    "operationId": "launchProfile",
                    "security": [{"cloudflareAccessJwt": []}],
                    "parameters": [
                        {"$ref": "#/components/parameters/TenantPath"},
                        {"$ref": "#/components/parameters/ProfilePath"},
                        {"$ref": "#/components/parameters/CorrelationHeader"},
                        {"$ref": "#/components/parameters/IdempotencyHeader"}
                    ],
                    "responses": {
                        "200": {
                            "description": "Bounded single-use Profile Bridge launch authority",
                            "content": {
                                "application/json": {
                                    "schema": {"$ref": "#/components/schemas/ProfileLaunchProjection"}
                                }
                            }
                        },
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
                "ProfileLaunchProjection": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["launchUri", "expiresAtMs"],
                    "properties": {
                        "launchUri": {
                            "type": "string",
                            "minLength": CLAIM_URI_PREFIX.len() + CLAIM_CODE_MIN_LENGTH,
                            "maxLength": CLAIM_URI_PREFIX.len() + CLAIM_CODE_MAX_LENGTH,
                            "pattern": CLAIM_URI_PATTERN
                        },
                        "expiresAtMs": {
                            "type": "integer",
                            "minimum": 1
                        }
                    }
                }
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::{ProfileLaunchProjection, openapi_fragment};

    #[test]
    fn launch_projection_is_strict_and_contains_no_caller_selected_device() {
        let valid = r#"{"launchUri":"profilebridge://claim/claim_01JBRIDGE_FEASIBILITY","expiresAtMs":1000}"#;
        assert!(serde_json::from_str::<ProfileLaunchProjection>(valid).is_ok());
        for invalid in [
            r#"{"launchUri":"profilebridge://claim/claim_01JBRIDGE_FEASIBILITY","expiresAtMs":1000,"deviceId":"device_01JTEST"}"#,
            r#"{"launchUri":"profilebridge://claim/short","expiresAtMs":1000}"#,
            r#"{"launchUri":"profilebridge://claim/claim_01JBRIDGE_FEASIBILITY?copy=true","expiresAtMs":1000}"#,
            r#"{"launchUri":"profilebridge://claim/claim_01JBRIDGE_FEASIBILITY","expiresAtMs":0}"#,
        ] {
            assert!(serde_json::from_str::<ProfileLaunchProjection>(invalid).is_err());
        }
    }

    #[test]
    fn fragment_owns_one_authenticated_additive_launch_operation() {
        let fragment = openapi_fragment();
        let operation =
            &fragment["paths"]["/api/v1/tenants/{tenantId}/profiles/{profileId}/launch"]["post"];
        assert_eq!(operation["operationId"], "launchProfile");
        assert_eq!(
            operation["security"][0]["cloudflareAccessJwt"],
            serde_json::json!([])
        );
        assert!(operation.get("requestBody").is_none());
        let parameters = operation["parameters"].as_array().expect("parameters");
        assert!(parameters.iter().all(|parameter| {
            parameter["$ref"] != "#/components/parameters/DevicePath"
                && parameter["$ref"] != "#/components/parameters/DeviceHeader"
        }));
    }
}
