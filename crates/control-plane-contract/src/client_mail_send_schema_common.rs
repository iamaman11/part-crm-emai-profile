use serde_json::{Value, json};

pub(crate) fn empty_schema() -> Value {
    json!({})
}
