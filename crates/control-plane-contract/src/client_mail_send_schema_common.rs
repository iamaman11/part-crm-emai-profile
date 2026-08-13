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
