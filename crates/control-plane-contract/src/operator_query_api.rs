use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

pub const PROFILE_STATUSES: [&str; 9] = [
    "DRAFT",
    "QUARANTINED",
    "READY",
    "IN_USE",
    "DIRTY_LOCAL",
    "SYNCING",
    "SUSPENDED",
    "DELETING",
    "DELETED",
];
pub const MEMBERSHIP_ROLES: [&str; 2] = ["TENANT_OWNER", "MEMBER"];
pub const MEMBERSHIP_STATUSES: [&str; 3] = ["ACTIVE", "SUSPENDED", "REVOKED"];
pub const MAILBOX_PROVIDERS: [&str; 3] = ["GMAIL_API", "IMAP", "BROWSER_FALLBACK"];
pub const MAILBOX_STATUSES: [&str; 4] = ["ACTIVE", "AUTH_REQUIRED", "SUSPENDED", "REVOKED"];

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProfileListItemDto {
    pub profile_id: String,
    pub status: String,
    pub version: u64,
    pub linked_client_id: Option<String>,
    pub active_generation_id: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProfileListPageDto {
    pub profiles: Vec<ProfileListItemDto>,
    pub next_cursor: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MemberListItemDto {
    pub actor_id: String,
    pub role: String,
    pub status: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MemberListPageDto {
    pub members: Vec<MemberListItemDto>,
    pub next_cursor: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MailboxListItemDto {
    pub binding_id: String,
    pub provider: String,
    pub status: String,
    pub version: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MailboxListPageDto {
    pub mailboxes: Vec<MailboxListItemDto>,
    pub next_cursor: Option<String>,
}

#[must_use]
pub fn openapi_fragment() -> Value {
    json!({
        "paths": {
            "/api/v1/tenants/{tenantId}/profiles": {
                "get": list_operation("listProfiles", "ProfileListPageDto")
            },
            "/api/v1/tenants/{tenantId}/members": {
                "get": list_operation("listMembers", "MemberListPageDto")
            },
            "/api/v1/tenants/{tenantId}/mailboxes": {
                "get": list_operation("listMailboxes", "MailboxListPageDto")
            }
        },
        "components": {
            "schemas": {
                "ProfileStatus": string_enum(&PROFILE_STATUSES),
                "MembershipRole": string_enum(&MEMBERSHIP_ROLES),
                "MembershipStatus": string_enum(&MEMBERSHIP_STATUSES),
                "MailboxProvider": string_enum(&MAILBOX_PROVIDERS),
                "MailboxStatus": string_enum(&MAILBOX_STATUSES),
                "ProfileListItemDto": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["profileId", "status", "version", "linkedClientId", "activeGenerationId"],
                    "properties": {
                        "profileId": {"type": "string", "minLength": 8, "maxLength": 96},
                        "status": schema_ref("ProfileStatus"),
                        "version": {"type": "integer", "minimum": 1},
                        "linkedClientId": {"type": "string", "nullable": true, "minLength": 8, "maxLength": 96},
                        "activeGenerationId": {"type": "string", "nullable": true, "minLength": 8, "maxLength": 96}
                    }
                },
                "ProfileListPageDto": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["profiles", "nextCursor"],
                    "properties": {
                        "profiles": {
                            "type": "array",
                            "maxItems": 100,
                            "items": schema_ref("ProfileListItemDto")
                        },
                        "nextCursor": nullable_cursor_schema()
                    }
                },
                "MemberListItemDto": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["actorId", "role", "status"],
                    "properties": {
                        "actorId": {"type": "string", "minLength": 8, "maxLength": 96},
                        "role": schema_ref("MembershipRole"),
                        "status": schema_ref("MembershipStatus")
                    }
                },
                "MemberListPageDto": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["members", "nextCursor"],
                    "properties": {
                        "members": {
                            "type": "array",
                            "maxItems": 100,
                            "items": schema_ref("MemberListItemDto")
                        },
                        "nextCursor": nullable_cursor_schema()
                    }
                },
                "MailboxListItemDto": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["bindingId", "provider", "status", "version"],
                    "properties": {
                        "bindingId": {"type": "string", "minLength": 8, "maxLength": 96},
                        "provider": schema_ref("MailboxProvider"),
                        "status": schema_ref("MailboxStatus"),
                        "version": {"type": "integer", "minimum": 1}
                    }
                },
                "MailboxListPageDto": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["mailboxes", "nextCursor"],
                    "properties": {
                        "mailboxes": {
                            "type": "array",
                            "maxItems": 100,
                            "items": schema_ref("MailboxListItemDto")
                        },
                        "nextCursor": nullable_cursor_schema()
                    }
                }
            }
        }
    })
}

fn list_operation(operation_id: &str, response_schema: &str) -> Value {
    json!({
        "operationId": operation_id,
        "parameters": [
            {
                "name": "tenantId",
                "in": "path",
                "required": true,
                "schema": {"type": "string"}
            },
            {
                "name": "limit",
                "in": "query",
                "required": false,
                "schema": {"type": "integer", "minimum": 1, "maximum": 100, "default": 50}
            },
            {
                "name": "cursor",
                "in": "query",
                "required": false,
                "schema": {"type": "string", "minLength": 1, "maxLength": 512}
            }
        ],
        "responses": {
            "200": {
                "description": "Authorized bounded query page",
                "content": {
                    "application/json": {
                        "schema": schema_ref(response_schema)
                    }
                }
            },
            "400": problem_response(),
            "404": problem_response(),
            "500": problem_response(),
            "503": problem_response()
        }
    })
}

fn schema_ref(name: &str) -> Value {
    json!({"$ref": format!("#/components/schemas/{name}")})
}

fn string_enum(values: &[&str]) -> Value {
    json!({"type": "string", "enum": values})
}

fn nullable_cursor_schema() -> Value {
    json!({"type": "string", "nullable": true, "maxLength": 512})
}

fn problem_response() -> Value {
    json!({
        "description": "Problem response",
        "content": {"application/problem+json": {"schema": {"type": "object"}}}
    })
}

#[cfg(test)]
mod tests {
    use super::{MailboxListPageDto, MemberListPageDto, ProfileListPageDto, openapi_fragment};

    #[test]
    fn operator_query_contract_has_bounded_list_paths() {
        let document = openapi_fragment();
        for path in [
            "/api/v1/tenants/{tenantId}/profiles",
            "/api/v1/tenants/{tenantId}/members",
            "/api/v1/tenants/{tenantId}/mailboxes",
        ] {
            assert!(document["paths"][path]["get"].is_object());
        }
    }

    #[test]
    fn list_pages_keep_nullable_cursor_on_wire() -> Result<(), Box<dyn std::error::Error>> {
        let profiles = serde_json::to_value(ProfileListPageDto {
            profiles: Vec::new(),
            next_cursor: None,
        })?;
        let members = serde_json::to_value(MemberListPageDto {
            members: Vec::new(),
            next_cursor: None,
        })?;
        let mailboxes = serde_json::to_value(MailboxListPageDto {
            mailboxes: Vec::new(),
            next_cursor: None,
        })?;
        assert!(profiles.get("nextCursor").is_some());
        assert!(members.get("nextCursor").is_some());
        assert!(mailboxes.get("nextCursor").is_some());
        Ok(())
    }
}
