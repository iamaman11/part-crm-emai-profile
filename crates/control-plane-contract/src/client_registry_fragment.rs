use crate::{client_registry_api, public_api};
use serde_json::{Map, Value, json};

pub fn canonical_fragment() -> Value {
    let base = public_api::openapi_document();
    let mut extended = base.clone();
    client_registry_api::extend_openapi(&mut extended);
    diff_fragment(&base, &extended)
}

pub fn compatibility_fragment() -> Value {
    let mut fragment = canonical_fragment();
    remap_legacy_refs(&mut fragment);
    decorate_legacy_transport_contract(&mut fragment);
    fragment
}

fn diff_fragment(base: &Value, extended: &Value) -> Value {
    let base_paths = base
        .get("paths")
        .and_then(Value::as_object)
        .expect("canonical base paths");
    let extended_paths = extended
        .get("paths")
        .and_then(Value::as_object)
        .expect("canonical extended paths");
    let mut paths = Map::new();
    for (route, extended_item) in extended_paths {
        let Some(extended_item) = extended_item.as_object() else {
            continue;
        };
        let base_item = base_paths.get(route).and_then(Value::as_object);
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
        .expect("canonical base schemas");
    let extended_schemas = extended
        .pointer("/components/schemas")
        .and_then(Value::as_object)
        .expect("canonical extended schemas");
    let mut schemas = Map::new();
    for (name, value) in extended_schemas {
        if !base_schemas.contains_key(name) {
            schemas.insert(name.clone(), value.clone());
        }
    }

    json!({
        "paths": Value::Object(paths),
        "components": {
            "schemas": Value::Object(schemas)
        }
    })
}

fn remap_legacy_refs(value: &mut Value) {
    match value {
        Value::Object(map) => {
            if let Some(Value::String(reference)) = map.get_mut("$ref") {
                if reference == "#/components/schemas/ClientProjection" {
                    *reference = "#/components/schemas/ClientView".to_owned();
                } else if reference == "#/components/schemas/ProblemPayload" {
                    *reference = "#/components/schemas/Problem".to_owned();
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

fn decorate_legacy_transport_contract(fragment: &mut Value) {
    let Some(paths) = fragment.get_mut("paths").and_then(Value::as_object_mut) else {
        return;
    };
    for path_item in paths.values_mut().filter_map(Value::as_object_mut) {
        for (method, operation) in path_item.iter_mut() {
            let Some(operation) = operation.as_object_mut() else {
                continue;
            };
            operation.insert(
                "security".to_owned(),
                json!([{"cloudflareAccessJwt": []}]),
            );
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
    fn fragment_contains_only_additive_client_registry_surface() {
        let fragment = canonical_fragment();
        let paths = fragment["paths"].as_object().expect("fragment paths");
        assert!(paths["/api/v1/tenants/{tenantId}/clients"].get("get").is_some());
        assert!(paths["/api/v1/tenants/{tenantId}/clients"].get("post").is_none());
        assert!(paths["/api/v1/tenants/{tenantId}/clients/{clientId}"].get("patch").is_some());
        assert!(paths["/api/v1/tenants/{tenantId}/clients/{clientId}"].get("get").is_none());
        assert!(paths["/api/v1/tenants/{tenantId}/clients/{clientId}/merge"].get("post").is_some());
    }

    #[test]
    fn compatibility_fragment_uses_legacy_root_refs_and_transport_headers() {
        let fragment = compatibility_fragment();
        let rendered = serde_json::to_string(&fragment).expect("serialize fragment");
        assert!(!rendered.contains("#/components/schemas/ClientProjection"));
        assert!(!rendered.contains("#/components/schemas/ProblemPayload"));
        assert!(rendered.contains("#/components/schemas/ClientView"));
        assert!(rendered.contains("#/components/schemas/Problem"));
        assert!(rendered.contains("#/components/parameters/CorrelationHeader"));
        assert!(rendered.contains("#/components/parameters/IdempotencyHeader"));
    }
}
