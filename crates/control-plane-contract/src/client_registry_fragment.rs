use crate::{client_registry_api, public_api};
use client_registry_api::ClientRegistryOpenApiError;
use serde_json::{Map, Value, json};

pub fn canonical_fragment() -> Result<Value, ClientRegistryOpenApiError> {
    let base = public_api::openapi_document();
    let mut extended = base.clone();
    client_registry_api::extend_openapi(&mut extended)?;
    diff_fragment(&base, &extended)
}

pub fn compatibility_fragment() -> Result<Value, ClientRegistryOpenApiError> {
    let mut fragment = canonical_fragment()?;
    remap_legacy_refs(&mut fragment);
    inject_client_registry_problem(&mut fragment)?;
    decorate_legacy_transport_contract(&mut fragment);
    Ok(fragment)
}

fn diff_fragment(base: &Value, extended: &Value) -> Result<Value, ClientRegistryOpenApiError> {
    let base_paths = base
        .get("paths")
        .and_then(Value::as_object)
        .ok_or(ClientRegistryOpenApiError::MissingPathsObject)?;
    let extended_paths = extended
        .get("paths")
        .and_then(Value::as_object)
        .ok_or(ClientRegistryOpenApiError::MissingPathsObject)?;
    let mut paths = Map::new();
    for (route, extended_item) in extended_paths {
        let extended_item = extended_item
            .as_object()
            .ok_or(ClientRegistryOpenApiError::InvalidPathItem)?;
        let base_item = match base_paths.get(route) {
            Some(item) => Some(
                item.as_object()
                    .ok_or(ClientRegistryOpenApiError::InvalidPathItem)?,
            ),
            None => None,
        };
        let mut additions = Map::new();
        for (name, value) in extended_item {
            if base_item.is_none_or(|item| !item.contains_key(name)) {
                additions.insert(name.clone(), value.clone());
            }
        }
        if !additions.is_empty() {
            paths.insert(route.clone(), Value::Object(additions));
        }
    }

    let base_schemas = base
        .pointer("/components/schemas")
        .and_then(Value::as_object)
        .ok_or(ClientRegistryOpenApiError::MissingSchemasObject)?;
    let extended_schemas = extended
        .pointer("/components/schemas")
        .and_then(Value::as_object)
        .ok_or(ClientRegistryOpenApiError::MissingSchemasObject)?;
    let mut schemas = Map::new();
    for (name, value) in extended_schemas {
        if !base_schemas.contains_key(name) {
            schemas.insert(name.clone(), value.clone());
        }
    }

    Ok(json!({
        "paths": Value::Object(paths),
        "components": {
            "schemas": Value::Object(schemas)
        }
    }))
}

fn remap_legacy_refs(value: &mut Value) {
    match value {
        Value::Object(map) => {
            if let Some(Value::String(reference)) = map.get_mut("$ref") {
                if reference == "#/components/schemas/ClientProjection" {
                    *reference = "#/components/schemas/ClientView".to_owned();
                } else if reference == "#/components/schemas/ProblemPayload" {
                    *reference = "#/components/schemas/ClientRegistryProblem".to_owned();
                }
            }
            for child in map.values_mut() {
                remap_legacy_refs(child);
            }
        }
        Value::Array(values) => {
            for child in values {
                remap_legacy_refs(child);
            }
        }
        _ => {}
    }
}

fn inject_client_registry_problem(fragment: &mut Value) -> Result<(), ClientRegistryOpenApiError> {
    let schemas = fragment
        .pointer_mut("/components/schemas")
        .and_then(Value::as_object_mut)
        .ok_or(ClientRegistryOpenApiError::MissingSchemasObject)?;
    if schemas.contains_key("ClientRegistryProblem") {
        return Err(ClientRegistryOpenApiError::DuplicateSchema);
    }
    schemas.insert(
        "ClientRegistryProblem".to_owned(),
        json!({
            "type": "object",
            "additionalProperties": false,
            "required": ["type", "title", "status", "code", "correlation_id"],
            "properties": {
                "type": {"type": "string", "format": "uri"},
                "title": {"type": "string"},
                "status": {"type": "integer", "minimum": 400, "maximum": 599},
                "code": {"type": "string", "enum": public_api::PROBLEM_CODES},
                "correlation_id": {"type": "string", "minLength": 1}
            }
        }),
    );
    Ok(())
}

fn decorate_legacy_transport_contract(fragment: &mut Value) {
    let Some(paths) = fragment.get_mut("paths").and_then(Value::as_object_mut) else {
        return;
    };
    for path_item in paths.values_mut().filter_map(Value::as_object_mut) {
        for (method, operation) in path_item.iter_mut() {
            let Some(operation) = operation.as_object_mut() else {
                continue;
            };
            operation.insert("security".to_owned(), json!([{"cloudflareAccessJwt": []}]));
            let parameters = operation
                .entry("parameters")
                .or_insert_with(|| Value::Array(Vec::new()));
            let Some(parameters) = parameters.as_array_mut() else {
                continue;
            };
            parameters.push(json!({"$ref": "#/components/parameters/CorrelationHeader"}));
            if method != "get" {
                parameters.push(json!({"$ref": "#/components/parameters/IdempotencyHeader"}));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{canonical_fragment, compatibility_fragment};

    #[test]
    fn fragment_contains_only_additive_client_registry_surface()
    -> Result<(), Box<dyn std::error::Error>> {
        let fragment = canonical_fragment()?;
        let paths = fragment["paths"]
            .as_object()
            .ok_or("fragment paths must be an object")?;
        assert!(
            paths["/api/v1/tenants/{tenantId}/clients"]
                .get("get")
                .is_some()
        );
        assert!(
            paths["/api/v1/tenants/{tenantId}/clients"]
                .get("post")
                .is_none()
        );
        assert!(
            paths["/api/v1/tenants/{tenantId}/clients/{clientId}"]
                .get("patch")
                .is_some()
        );
        assert!(
            paths["/api/v1/tenants/{tenantId}/clients/{clientId}"]
                .get("get")
                .is_none()
        );
        assert!(
            paths["/api/v1/tenants/{tenantId}/clients/{clientId}/merge"]
                .get("post")
                .is_some()
        );
        Ok(())
    }

    #[test]
    fn compatibility_fragment_uses_legacy_client_view_and_full_problem_schema()
    -> Result<(), Box<dyn std::error::Error>> {
        let fragment = compatibility_fragment()?;
        let rendered = serde_json::to_string(&fragment)?;
        assert!(!rendered.contains("#/components/schemas/ClientProjection"));
        assert!(!rendered.contains("#/components/schemas/ProblemPayload"));
        assert!(rendered.contains("#/components/schemas/ClientView"));
        assert!(rendered.contains("#/components/schemas/ClientRegistryProblem"));
        let codes = fragment["components"]["schemas"]["ClientRegistryProblem"]["properties"]
            ["code"]["enum"]
            .as_array()
            .ok_or("problem code enum must be an array")?;
        assert!(codes.iter().any(|value| value == "version_conflict"));
        assert!(codes.iter().any(|value| value == "integrity_failure"));
        assert!(codes.iter().any(|value| value == "dependency_unavailable"));
        assert!(rendered.contains("#/components/parameters/CorrelationHeader"));
        assert!(rendered.contains("#/components/parameters/IdempotencyHeader"));
        Ok(())
    }
}
