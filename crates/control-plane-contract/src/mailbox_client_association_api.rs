use serde::{Deserialize, Deserializer, Serialize};

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
    #[serde(deserialize_with = "required_nullable_string")]
    pub client_id: Option<String>,
    pub expected_relationship_version: u64,
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

fn required_nullable_string<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: Deserializer<'de>,
{
    Option::<String>::deserialize(deserializer)
}

#[cfg(test)]
mod tests {
    use super::{ChangeMailboxClientAssociationRequestDto, MailboxClientAssociationProjectionDto};
    use serde_json::Value;

    #[test]
    fn association_change_is_strict_required_nullable_and_rejects_legacy_digest()
    -> Result<(), Box<dyn std::error::Error>> {
        let bind = r#"{"clientId":"client_01JASSOCIATION","expectedRelationshipVersion":0}"#;
        let unbind = r#"{"clientId":null,"expectedRelationshipVersion":2}"#;
        let missing = r#"{"expectedRelationshipVersion":2}"#;
        assert!(serde_json::from_str::<ChangeMailboxClientAssociationRequestDto>(bind).is_ok());
        assert!(serde_json::from_str::<ChangeMailboxClientAssociationRequestDto>(unbind).is_ok());
        assert!(serde_json::from_str::<ChangeMailboxClientAssociationRequestDto>(missing).is_err());

        let legacy_digest = r#"{"clientId":null,"expectedRelationshipVersion":2,"requestDigest":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}"#;
        assert!(
            serde_json::from_str::<ChangeMailboxClientAssociationRequestDto>(legacy_digest)
                .is_err()
        );
        for forbidden in [
            "secretHandle",
            "password",
            "accessToken",
            "providerToken",
            "profileId",
        ] {
            let invalid = format!(
                r#"{{"clientId":null,"expectedRelationshipVersion":2,"{forbidden}":"forbidden"}}"#
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
}
