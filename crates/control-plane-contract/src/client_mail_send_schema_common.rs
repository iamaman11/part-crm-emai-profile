use serde_json::{Value, json};

pub(crate) fn schema_ref(name: &str) -> Value {
    json!({"$ref": format!("#/components/schemas/{name}")})
}

pub(crate) fn path_parameter(name: &str) -> Value {
    json!({
        "name": name,
        "in": "path",
        "required": true,
        "schema": {"type": "string", "minLength": 8, "maxLength": 96}
    })
}

pub(crate) fn address_array() -> Value {
    json!({
        "type": "array",
        "maxItems": 100,
        "items": {"type": "string", "minLength": 3, "maxLength": 320}
    })
}
