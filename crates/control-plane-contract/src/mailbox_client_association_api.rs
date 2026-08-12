use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MailboxClientAssociationProjectionDto {
    pub binding_id: String,
    pub client_id: Option<String>,
    pub relationship_version: u64,
    pub mailbox_executable: bool,
    pub can_manage: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ChangeMailboxClientAssociationRequestDto {
    pub client_id: Option<String>,
    pub expected_relationship_version: u64,
    pub request_digest: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MailboxClientAssociationMutationReceiptDto {
    pub result_code: String,
    pub binding_id: String,
    pub client_id: Option<String>,
    pub relationship_version: u64,
    pub replayed: bool,
}

#[must_use]
pub fn openapi_fragment() -> Value {
    json!({
        "paths": {
            "/api/v1/tenants/{tenantId}/mailboxes/{bindingId}/client-association": {
                "get": {
                    "operationId": "getMailboxClientAssociation",
                    "parameters": path_parameters(),
                    "responses": {
                        "200": json_response("Current mailbox Client association metadata", "MailboxClientAssociationProjectionDto"),
                        "404": problem_response(),
                        "500": problem_response(),
                        "503": problem_response()
                    }
                },
                "post": {
                    "operationId": "changeMailboxClientAssociation",
                    "parameters": path_parameters(),
                    "requestBody": {
                        "required": true,
                        "content": {
                            "application/json": {
                                "schema": schema_ref("ChangeMailboxClientAssociationRequestDto")
                            }
                        }
                    },
                    "responses": {
                        "200": json_response("Accepted bind, rebind or unbind result", "MailboxClientAssociationMutationReceiptDto"),
                        "400": problem_response(),
                        "404": problem_response(),
                        "409": problem_response(),
                        "500": problem_response(),
                        "503": problem_response()
                    }
                }
            }
        },
        "components": {
            "schemas": {
                "MailboxClientAssociationProjectionDto": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["bindingId", "clientId", "relationshipVersion", "mailboxExecutable", "canManage"],
                    "properties": {
                        "bindingId": opaque_id_schema(),
                        "clientId": nullable_opaque_id_schema(),
                        "relationshipVersion": relationship_version_schema(),
                        "mailboxExecutable": {"type": "boolean"},
                        "canManage": {"type": "boolean"}
                    }
                },
                "ChangeMailboxClientAssociationRequestDto": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["clientId", "expectedRelationshipVersion", "requestDigest"],
                    "properties": {
                        "clientId": nullable_opaque_id_schema(),
                        "expectedRelationshipVersion": relationship_version_schema(),
                        "requestDigest": sha256_schema()
                    }
                },
                "MailboxClientAssociationMutationReceiptDto": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["resultCode", "bindingId", "clientId", "relationshipVersion", "replayed"],
                    "properties": {
                        "resultCode": {
                            "type": "string",
                            "enum": ["bound", "rebound", "unbound"]
                        },
                        "bindingId": opaque_id_schema(),
                        "clientId": nullable_opaque_id_schema(),
                        "relationshipVersion": relationship_version_schema(),
                        "replayed": {"type": "boolean"}
                    }
                }
            }
        }
    })
}

fn path_parameters() -> Value {
    json!([
        {
            "name": "tenantId",
            "in": "path",
            "required": true,
            "schema": opaque_id_schema()
        },
        {
            "name": "bindingId",
            "in": "path",
            "required": true,
            "schema": opaque_id_schema()
        }
    ])
}

fn schema_ref(name: &str) -> Value {
    json!({"$ref": format!("#/components/schemas/{name}")})
}

fn opaque_id_schema() -> Value {
    json!({"type": "string", "minLength": 8, "maxLength": 96})
}

fn nullable_opaque_id_schema() -> Value {
    json!({"type": "string", "nullable": true, "minLength": 8, "maxLength": 96})
}

fn relationship_version_schema() -> Value {
    json!({"type": "integer", "minimum": 0})
}

fn sha256_schema() -> Value {
    json!({"type": "string", "pattern": "^[0-9a-f]{64}$"})
}

fn json_response(description: &str, schema: &str) -> Value {
    json!({
        "description": description,
        "content": {"application/json": {"schema": schema_ref(schema)}}
    })
}

fn problem_response() -> Value {
    json!({
        "description": "Problem response",
        "content": {"application/problem+json": {"schema": {"type": "object"}}}
    })
}

#[cfg(test)]
mod tests {
    use super::{
        ChangeMailboxClientAssociationRequestDto, MailboxClientAssociationProjectionDto,
        openapi_fragment,
    };
    use serde_json::Value;

    #[test]
    fn association_change_is_strict_nullable_and_contains_no_credential_surface()
    -> Result<(), Box<dyn std::error::Error>> {
        let digest = "a".repeat(64);
        let bind = format!(
            r#"{{"clientId":"client_01JASSOCIATION","expectedRelationshipVersion":0,"requestDigest":"{digest}"}}"#
        );
        let unbind = format!(
            r#"{{"clientId":null,"expectedRelationshipVersion":2,"requestDigest":"{digest}"}}"#
        );
        assert!(serde_json::from_str::<ChangeMailboxClientAssociationRequestDto>(&bind).is_ok());
        assert!(serde_json::from_str::<ChangeMailboxClientAssociationRequestDto>(&unbind).is_ok());
        for forbidden in [
            "secretHandle",
            "password",
            "accessToken",
            "providerToken",
            "profileId",
        ] {
            let invalid = format!(
                r#"{{"clientId":null,"expectedRelationshipVersion":2,"requestDigest":"{digest}","{forbidden}":"forbidden"}}"#
            );
            assert!(
                serde_json::from_str::<ChangeMailboxClientAssociationRequestDto>(&invalid).is_err()
            );
        }
        Ok(())
    }

    #[test]
    fn projection_preserves_unassigned_version_zero_and_explicit_manage_capability()
    -> Result<(), Box<dyn std::error::Error>> {
        let value = serde_json::to_value(MailboxClientAssociationProjectionDto {
            binding_id: "mailbox_01JASSOCIATION".to_owned(),
            client_id: None,
            relationship_version: 0,
            mailbox_executable: true,
            can_manage: true,
        })?;
        assert_eq!(value["relationshipVersion"], 0);
        assert!(value.get("clientId").is_some_and(Value::is_null));
        assert_eq!(value["canManage"], true);
        Ok(())
    }

    #[test]
    fn public_fragment_is_one_resource_with_get_and_unified_change_command() {
        let document = openapi_fragment();
        let resource = &document["paths"]
            ["/api/v1/tenants/{tenantId}/mailboxes/{bindingId}/client-association"];
        assert_eq!(
            resource["get"]["operationId"],
            "getMailboxClientAssociation"
        );
        assert_eq!(
            resource["post"]["operationId"],
            "changeMailboxClientAssociation"
        );
        assert!(resource.get("put").is_none());
        assert!(resource.get("delete").is_none());
    }
}
