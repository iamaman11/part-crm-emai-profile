use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProfileLaunchProjection {
    pub launch_uri: String,
    pub expires_at_ms: u64,
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
                            "minLength": 46,
                            "maxLength": 118,
                            "pattern": "^profilebridge://claim/[A-Za-z0-9_-]{24,96}$"
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
        ] {
            assert!(serde_json::from_str::<ProfileLaunchProjection>(invalid).is_err());
        }
    }

    #[test]
    fn fragment_owns_one_authenticated_additive_launch_operation() {
        let fragment = openapi_fragment();
        let operation = &fragment["paths"]
            ["/api/v1/tenants/{tenantId}/profiles/{profileId}/launch"]["post"];
        assert_eq!(operation["operationId"], "launchProfile");
        assert_eq!(operation["security"][0]["cloudflareAccessJwt"], serde_json::json!([]));
        assert!(operation.get("requestBody").is_none());
        let parameters = operation["parameters"].as_array().expect("parameters");
        assert!(parameters.iter().all(|parameter| {
            parameter["$ref"] != "#/components/parameters/DevicePath"
                && parameter["$ref"] != "#/components/parameters/DeviceHeader"
        }));
    }
}
