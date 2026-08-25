use serde_json::Value;
use std::fmt;

const FORBIDDEN_COMPILER_REPAIR_EXTENSIONS: [&str; 3] = [
    "x-part-crm-request-digest",
    "x-part-crm-required-response-headers",
    "x-part-crm-browser-transport",
];

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum FrontendTransportContractError {
    InvalidRoot,
    UnsupportedDialect(String),
    LegacyNullable,
    NetworkReference(String),
    UnresolvedReference(String),
    PermissiveProblemSchema,
    ForbiddenCompilerRepairExtension(String),
}

impl fmt::Display for FrontendTransportContractError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidRoot => write!(formatter, "frontend OpenAPI root must be an object"),
            Self::UnsupportedDialect(value) => write!(
                formatter,
                "frontend OpenAPI dialect must be exactly 3.1.0, got {value:?}"
            ),
            Self::LegacyNullable => write!(
                formatter,
                "legacy OpenAPI nullable is forbidden; fix the capability-owned producer"
            ),
            Self::NetworkReference(reference) => {
                write!(formatter, "network OpenAPI reference is forbidden: {reference}")
            }
            Self::UnresolvedReference(reference) => {
                write!(formatter, "unresolved local OpenAPI reference: {reference}")
            }
            Self::PermissiveProblemSchema => write!(
                formatter,
                "permissive application/problem+json schema is forbidden; fix the capability-owned producer"
            ),
            Self::ForbiddenCompilerRepairExtension(extension) => write!(
                formatter,
                "compiler repair extension is forbidden in canonical OpenAPI: {extension}"
            ),
        }
    }
}

impl std::error::Error for FrontendTransportContractError {}

/// Validate the canonical frontend compiler input without mutating or repairing it.
///
/// Capability-owned producers are responsible for emitting valid OpenAPI 3.1. The
/// compiler boundary is intentionally fail-closed: legacy dialect constructs,
/// permissive problem schemas, network/unresolved references, and predecessor
/// repair extensions are rejected rather than normalized into a different contract.
pub fn validate_compiler_input(document: &Value) -> Result<(), FrontendTransportContractError> {
    let root = document
        .as_object()
        .ok_or(FrontendTransportContractError::InvalidRoot)?;
    let dialect = root
        .get("openapi")
        .and_then(Value::as_str)
        .ok_or(FrontendTransportContractError::InvalidRoot)?;
    if dialect != "3.1.0" {
        return Err(FrontendTransportContractError::UnsupportedDialect(
            dialect.to_owned(),
        ));
    }
    validate_value(document, document)
}

fn validate_value(
    document: &Value,
    value: &Value,
) -> Result<(), FrontendTransportContractError> {
    match value {
        Value::Object(map) => {
            if map.contains_key("nullable") {
                return Err(FrontendTransportContractError::LegacyNullable);
            }
            for extension in FORBIDDEN_COMPILER_REPAIR_EXTENSIONS {
                if map.contains_key(extension) {
                    return Err(
                        FrontendTransportContractError::ForbiddenCompilerRepairExtension(
                            extension.to_owned(),
                        ),
                    );
                }
            }

            if let Some(Value::String(reference)) = map.get("$ref") {
                validate_reference(document, reference)?;
            }

            if let Some(Value::Object(content)) = map.get("content") {
                if let Some(Value::Object(problem_media)) = content.get("application/problem+json") {
                    let schema = problem_media
                        .get("schema")
                        .ok_or(FrontendTransportContractError::PermissiveProblemSchema)?;
                    let resolved = resolve_schema(document, schema)?;
                    let permissive = resolved.as_object().is_some_and(|object| {
                        object.is_empty()
                            || (object.len() == 1
                                && object.get("type")
                                    == Some(&Value::String("object".to_owned())))
                    });
                    if permissive {
                        return Err(FrontendTransportContractError::PermissiveProblemSchema);
                    }
                }
            }

            for child in map.values() {
                validate_value(document, child)?;
            }
        }
        Value::Array(values) => {
            for child in values {
                validate_value(document, child)?;
            }
        }
        _ => {}
    }
    Ok(())
}

fn validate_reference(
    document: &Value,
    reference: &str,
) -> Result<(), FrontendTransportContractError> {
    if !reference.starts_with("#/") {
        return Err(FrontendTransportContractError::NetworkReference(
            reference.to_owned(),
        ));
    }
    let pointer = reference
        .strip_prefix('#')
        .ok_or_else(|| FrontendTransportContractError::NetworkReference(reference.to_owned()))?;
    if document.pointer(pointer).is_none() {
        return Err(FrontendTransportContractError::UnresolvedReference(
            reference.to_owned(),
        ));
    }
    Ok(())
}

fn resolve_schema<'a>(
    document: &'a Value,
    schema: &'a Value,
) -> Result<&'a Value, FrontendTransportContractError> {
    let Some(reference) = schema.get("$ref").and_then(Value::as_str) else {
        return Ok(schema);
    };
    validate_reference(document, reference)?;
    let pointer = reference
        .strip_prefix('#')
        .ok_or_else(|| FrontendTransportContractError::NetworkReference(reference.to_owned()))?;
    document.pointer(pointer).ok_or_else(|| {
        FrontendTransportContractError::UnresolvedReference(reference.to_owned())
    })
}

#[cfg(test)]
mod tests {
    use super::{FrontendTransportContractError, validate_compiler_input};
    use serde_json::json;

    fn strict_document() -> serde_json::Value {
        json!({
            "openapi": "3.1.0",
            "info": {"title": "fixture", "version": "1.0.0"},
            "paths": {
                "/api/v1/fixture": {
                    "get": {
                        "operationId": "getFixture",
                        "responses": {
                            "400": {
                                "description": "problem",
                                "content": {
                                    "application/problem+json": {
                                        "schema": {"$ref": "#/components/schemas/Problem"}
                                    }
                                }
                            }
                        }
                    }
                }
            },
            "components": {
                "schemas": {
                    "Problem": {
                        "type": "object",
                        "additionalProperties": false,
                        "required": ["type"],
                        "properties": {"type": {"type": "string"}}
                    }
                }
            }
        })
    }

    #[test]
    fn strict_openapi_is_validated_without_repair() {
        let document = strict_document();
        let before = document.clone();
        assert_eq!(validate_compiler_input(&document), Ok(()));
        assert_eq!(document, before);
    }

    #[test]
    fn legacy_nullable_fails_closed() {
        let mut document = strict_document();
        document["components"]["schemas"]["Legacy"] = json!({
            "type": "string",
            "nullable": true
        });
        assert_eq!(
            validate_compiler_input(&document),
            Err(FrontendTransportContractError::LegacyNullable)
        );
    }

    #[test]
    fn permissive_problem_schema_fails_closed() {
        let mut document = strict_document();
        document["paths"]["/api/v1/fixture"]["get"]["responses"]["400"]["content"]
            ["application/problem+json"]["schema"] = json!({"type": "object"});
        assert_eq!(
            validate_compiler_input(&document),
            Err(FrontendTransportContractError::PermissiveProblemSchema)
        );
    }

    #[test]
    fn predecessor_repair_extensions_fail_closed() {
        let mut document = strict_document();
        document["x-part-crm-request-digest"] = json!({"canonicalization": "part-crm-json-v1"});
        assert_eq!(
            validate_compiler_input(&document),
            Err(FrontendTransportContractError::ForbiddenCompilerRepairExtension(
                "x-part-crm-request-digest".to_owned()
            ))
        );
    }

    #[test]
    fn network_reference_fails_closed() {
        let mut document = strict_document();
        document["components"]["schemas"]["Remote"] =
            json!({"$ref": "https://example.invalid/schema.json"});
        assert!(matches!(
            validate_compiler_input(&document),
            Err(FrontendTransportContractError::NetworkReference(_))
        ));
    }
}
