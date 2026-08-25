use serde_json::{Value, json};
use std::fmt;

pub const REQUEST_DIGEST_EXTENSION: &str = "x-part-crm-request-digest";
pub const REQUIRED_RESPONSE_HEADERS_EXTENSION: &str = "x-part-crm-required-response-headers";
pub const BROWSER_TRANSPORT_EXTENSION: &str = "x-part-crm-browser-transport";

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum FrontendTransportContractError {
    InvalidRoot,
    UnsupportedDialect(String),
    UnsupportedNullable,
    UnsupportedNumber,
    MissingProblemPayload,
    NetworkReference(String),
    UnresolvedReference(String),
    ExtensionConflict(&'static str),
    Serialization(String),
}

impl fmt::Display for FrontendTransportContractError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidRoot => write!(formatter, "frontend OpenAPI root must be an object"),
            Self::UnsupportedDialect(value) => {
                write!(
                    formatter,
                    "frontend OpenAPI dialect must be exactly 3.1.0, got {value:?}"
                )
            }
            Self::UnsupportedNullable => write!(
                formatter,
                "OpenAPI 3.0 nullable is forbidden in the frontend compiler input"
            ),
            Self::UnsupportedNumber => write!(
                formatter,
                "part-crm-json-v1 request digests support integers only; floating-point JSON numbers are forbidden"
            ),
            Self::MissingProblemPayload => write!(
                formatter,
                "application/problem+json closure requires components.schemas.ProblemPayload"
            ),
            Self::NetworkReference(reference) => {
                write!(
                    formatter,
                    "network OpenAPI reference is forbidden: {reference}"
                )
            }
            Self::UnresolvedReference(reference) => {
                write!(formatter, "unresolved local OpenAPI reference: {reference}")
            }
            Self::ExtensionConflict(extension) => {
                write!(
                    formatter,
                    "conflicting generated OpenAPI extension: {extension}"
                )
            }
            Self::Serialization(message) => {
                write!(formatter, "canonical JSON serialization failed: {message}")
            }
        }
    }
}

impl std::error::Error for FrontendTransportContractError {}

#[must_use]
pub fn request_digest_extension() -> Value {
    json!({
        "algorithm": "sha-256",
        "canonicalization": "part-crm-json-v1",
        "encoding": "lowercase-hex",
        "source": "request-body-without-requestDigest",
        "rules": {
            "arrays": "preserve-order",
            "numbers": "integers-only",
            "objectKeys": "unicode-codepoint-ascending",
            "textEncoding": "utf-8",
            "whitespace": "none"
        }
    })
}

#[must_use]
pub fn browser_transport_extension() -> Value {
    json!({
        "allowedPathPrefix": "/api/v1/",
        "absoluteUrls": "forbidden",
        "credentials": "same-origin",
        "openapiServers": "documentation-only",
        "redirect": "error"
    })
}

pub fn canonical_digest_material(value: &Value) -> Result<Vec<u8>, FrontendTransportContractError> {
    let mut bytes = Vec::new();
    write_canonical_json(value, &mut bytes, true)?;
    Ok(bytes)
}

fn write_canonical_json(
    value: &Value,
    output: &mut Vec<u8>,
    omit_root_request_digest: bool,
) -> Result<(), FrontendTransportContractError> {
    match value {
        Value::Null | Value::Bool(_) | Value::String(_) => {
            output.extend_from_slice(
                serde_json::to_string(value)
                    .map_err(|error| {
                        FrontendTransportContractError::Serialization(error.to_string())
                    })?
                    .as_bytes(),
            );
        }
        Value::Number(number) => {
            if !(number.is_i64() || number.is_u64()) {
                return Err(FrontendTransportContractError::UnsupportedNumber);
            }
            output.extend_from_slice(number.to_string().as_bytes());
        }
        Value::Array(values) => {
            output.push(b'[');
            for (index, child) in values.iter().enumerate() {
                if index != 0 {
                    output.push(b',');
                }
                write_canonical_json(child, output, false)?;
            }
            output.push(b']');
        }
        Value::Object(map) => {
            output.push(b'{');
            let mut keys: Vec<&str> = map
                .keys()
                .map(String::as_str)
                .filter(|key| !(omit_root_request_digest && *key == "requestDigest"))
                .collect();
            keys.sort_unstable();
            for (index, key) in keys.into_iter().enumerate() {
                if index != 0 {
                    output.push(b',');
                }
                output.extend_from_slice(
                    serde_json::to_string(key)
                        .map_err(|error| {
                            FrontendTransportContractError::Serialization(error.to_string())
                        })?
                        .as_bytes(),
                );
                output.push(b':');
                let child = map.get(key).ok_or_else(|| {
                    FrontendTransportContractError::Serialization(
                        "missing canonical JSON object member".to_owned(),
                    )
                })?;
                write_canonical_json(child, output, false)?;
            }
            output.push(b'}');
        }
    }
    Ok(())
}

pub fn close_compiler_input(mut document: Value) -> Result<Value, FrontendTransportContractError> {
    let dialect = document
        .get("openapi")
        .and_then(Value::as_str)
        .ok_or(FrontendTransportContractError::InvalidRoot)?;
    if dialect != "3.1.0" {
        return Err(FrontendTransportContractError::UnsupportedDialect(
            dialect.to_owned(),
        ));
    }
    let has_problem_payload = document
        .pointer("/components/schemas/ProblemPayload")
        .is_some();
    close_value(&mut document, has_problem_payload)?;
    let root = document
        .as_object_mut()
        .ok_or(FrontendTransportContractError::InvalidRoot)?;
    insert_generated_extension(
        root,
        BROWSER_TRANSPORT_EXTENSION,
        browser_transport_extension(),
    )?;
    validate_local_references(&document, &document)?;
    Ok(document)
}

fn close_value(
    value: &mut Value,
    has_problem_payload: bool,
) -> Result<(), FrontendTransportContractError> {
    match value {
        Value::Object(map) => {
            if map.contains_key("nullable") {
                return Err(FrontendTransportContractError::UnsupportedNullable);
            }
            if let Some(Value::String(reference)) = map.get("$ref") {
                if !reference.starts_with("#/") {
                    return Err(FrontendTransportContractError::NetworkReference(
                        reference.clone(),
                    ));
                }
            }

            if let Some(Value::Object(properties)) = map.get_mut("properties") {
                if let Some(Value::Object(request_digest)) = properties.get_mut("requestDigest") {
                    insert_generated_extension(
                        request_digest,
                        REQUEST_DIGEST_EXTENSION,
                        request_digest_extension(),
                    )?;
                }
            }

            if let Some(Value::Object(headers)) = map.get("headers") {
                if !headers.is_empty() {
                    let mut names: Vec<String> = headers.keys().cloned().collect();
                    names.sort_unstable();
                    insert_generated_extension(
                        map,
                        REQUIRED_RESPONSE_HEADERS_EXTENSION,
                        json!(names),
                    )?;
                }
            }

            if let Some(Value::Object(content)) = map.get_mut("content") {
                if let Some(Value::Object(problem_media)) =
                    content.get_mut("application/problem+json")
                {
                    if let Some(schema) = problem_media.get_mut("schema") {
                        let is_permissive_object = schema.as_object().is_some_and(|object| {
                            object.len() == 1
                                && object.get("type") == Some(&Value::String("object".to_owned()))
                        });
                        if is_permissive_object {
                            if !has_problem_payload {
                                return Err(FrontendTransportContractError::MissingProblemPayload);
                            }
                            *schema = json!({"$ref": "#/components/schemas/ProblemPayload"});
                        }
                    }
                }
            }

            for child in map.values_mut() {
                close_value(child, has_problem_payload)?;
            }
        }
        Value::Array(values) => {
            for child in values {
                close_value(child, has_problem_payload)?;
            }
        }
        _ => {}
    }
    Ok(())
}

fn insert_generated_extension(
    map: &mut serde_json::Map<String, Value>,
    name: &'static str,
    expected: Value,
) -> Result<(), FrontendTransportContractError> {
    match map.get(name) {
        Some(existing) if existing != &expected => {
            Err(FrontendTransportContractError::ExtensionConflict(name))
        }
        Some(_) => Ok(()),
        None => {
            map.insert(name.to_owned(), expected);
            Ok(())
        }
    }
}

fn validate_local_references(
    document: &Value,
    value: &Value,
) -> Result<(), FrontendTransportContractError> {
    match value {
        Value::Object(map) => {
            if let Some(Value::String(reference)) = map.get("$ref") {
                if !reference.starts_with("#/") {
                    return Err(FrontendTransportContractError::NetworkReference(
                        reference.clone(),
                    ));
                }
                let pointer = reference.strip_prefix('#').ok_or_else(|| {
                    FrontendTransportContractError::NetworkReference(reference.clone())
                })?;
                if document.pointer(pointer).is_none() {
                    return Err(FrontendTransportContractError::UnresolvedReference(
                        reference.clone(),
                    ));
                }
            }
            for child in map.values() {
                validate_local_references(document, child)?;
            }
        }
        Value::Array(values) => {
            for child in values {
                validate_local_references(document, child)?;
            }
        }
        _ => {}
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        BROWSER_TRANSPORT_EXTENSION, FrontendTransportContractError, REQUEST_DIGEST_EXTENSION,
        REQUIRED_RESPONSE_HEADERS_EXTENSION, canonical_digest_material, close_compiler_input,
    };
    use serde_json::json;

    #[test]
    fn digest_material_is_order_independent_and_excludes_digest_field()
    -> Result<(), Box<dyn std::error::Error>> {
        let first = json!({
            "displayName": "Alice",
            "requestDigest": "ignored",
            "kind": "PERSON",
            "clientId": "client_01"
        });
        let second = json!({
            "clientId": "client_01",
            "kind": "PERSON",
            "displayName": "Alice"
        });
        let expected = br#"{"clientId":"client_01","displayName":"Alice","kind":"PERSON"}"#;
        assert_eq!(canonical_digest_material(&first)?, expected);
        assert_eq!(canonical_digest_material(&second)?, expected);
        Ok(())
    }

    #[test]
    fn digest_material_rejects_floating_point_numbers() {
        assert_eq!(
            canonical_digest_material(&json!({"value": 1.5})),
            Err(FrontendTransportContractError::UnsupportedNumber)
        );
    }

    #[test]
    fn compiler_input_closure_emits_bounded_transport_extensions_and_strict_problem()
    -> Result<(), Box<dyn std::error::Error>> {
        let document = json!({
            "openapi": "3.1.0",
            "info": {"title": "fixture", "version": "1.0.0"},
            "paths": {
                "/api/v1/fixture": {
                    "post": {
                        "operationId": "fixtureMutation",
                        "requestBody": {
                            "required": true,
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "type": "object",
                                        "properties": {
                                            "requestDigest": {"type": "string"}
                                        }
                                    }
                                }
                            }
                        },
                        "responses": {
                            "200": {
                                "description": "ok",
                                "headers": {"X-Correlation-Id": {"schema": {"type": "string"}}},
                                "content": {"application/json": {"schema": {"type": "object"}}}
                            },
                            "400": {
                                "description": "problem",
                                "content": {"application/problem+json": {"schema": {"type": "object"}}}
                            }
                        }
                    }
                }
            },
            "components": {
                "schemas": {
                    "ProblemPayload": {
                        "type": "object",
                        "additionalProperties": false
                    }
                }
            }
        });
        let closed = close_compiler_input(document)?;
        assert!(closed.get(BROWSER_TRANSPORT_EXTENSION).is_some());
        assert!(
            closed["paths"]["/api/v1/fixture"]["post"]["requestBody"]["content"]
                ["application/json"]["schema"]["properties"]["requestDigest"]
                .get(REQUEST_DIGEST_EXTENSION)
                .is_some()
        );
        assert_eq!(
            closed["paths"]["/api/v1/fixture"]["post"]["responses"]["200"]
                [REQUIRED_RESPONSE_HEADERS_EXTENSION],
            json!(["X-Correlation-Id"])
        );
        assert_eq!(
            closed["paths"]["/api/v1/fixture"]["post"]["responses"]["400"]["content"]["application/problem+json"]
                ["schema"]["$ref"],
            "#/components/schemas/ProblemPayload"
        );
        Ok(())
    }

    #[test]
    fn compiler_input_rejects_mixed_dialect_and_network_refs() {
        let mixed = json!({"openapi": "3.0.3"});
        assert!(matches!(
            close_compiler_input(mixed),
            Err(FrontendTransportContractError::UnsupportedDialect(_))
        ));
        let network = json!({
            "openapi": "3.1.0",
            "info": {"title": "fixture", "version": "1.0.0"},
            "paths": {},
            "components": {"schemas": {"Remote": {"$ref": "https://example.invalid/schema.json"}}}
        });
        assert!(matches!(
            close_compiler_input(network),
            Err(FrontendTransportContractError::NetworkReference(_))
        ));
    }
}
