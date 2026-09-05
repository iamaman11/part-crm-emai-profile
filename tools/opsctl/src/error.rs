use crate::d1::D1Error;
use serde_json::{Value, json};
use std::error::Error;
use std::fmt;

#[derive(Debug, PartialEq, Eq)]
pub struct OpsctlError {
    command: &'static str,
    message: String,
    gate_result: Option<Value>,
}

impl OpsctlError {
    pub(crate) fn new(command: &'static str, message: impl Into<String>) -> Self {
        Self {
            command,
            message: message.into(),
            gate_result: None,
        }
    }

    pub(crate) fn from_d1(error: D1Error) -> Self {
        Self {
            command: "d1",
            message: error.to_string(),
            gate_result: Some(error.gate_result_json()),
        }
    }

    #[must_use]
    pub fn json(&self) -> String {
        let mut output = json!({
            "schema_version": 1,
            "command": self.command,
            "status": "error",
            "mode": "read-only",
            "mutation_executed": false,
            "error": self.message,
        });
        if let Some(gate_result) = &self.gate_result {
            output["gate_result"] = gate_result.clone();
        }
        match serde_json::to_string(&output) {
            Ok(serialized) => serialized + "\n",
            Err(_) => "{\"schema_version\":1,\"command\":\"opsctl\",\"status\":\"error\",\"mode\":\"read-only\",\"mutation_executed\":false,\"error\":\"OPSCTL_ERROR_SERIALIZATION_FAILED\"}\n".to_owned(),
        }
    }
}

impl fmt::Display for OpsctlError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl Error for OpsctlError {}

#[cfg(test)]
mod tests {
    use super::OpsctlError;

    #[test]
    fn ordinary_errors_remain_secret_free_read_only_json() -> Result<(), serde_json::Error> {
        let parsed: serde_json::Value =
            serde_json::from_str(&OpsctlError::new("doctor", "broken").json())?;
        assert_eq!(parsed["command"], "doctor");
        assert_eq!(parsed["status"], "error");
        assert_eq!(parsed["mutation_executed"], false);
        assert_eq!(parsed["error"], "broken");
        assert!(parsed.get("gate_result").is_none());
        Ok(())
    }
}
