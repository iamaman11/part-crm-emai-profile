use std::error::Error;
use std::fmt;

#[derive(Debug, PartialEq, Eq)]
pub struct OpsctlError {
    command: &'static str,
    message: String,
}

impl OpsctlError {
    pub(crate) fn new(command: &'static str, message: impl Into<String>) -> Self {
        Self {
            command,
            message: message.into(),
        }
    }

    #[must_use]
    pub fn json(&self) -> String {
        format!(
            "{{\"schema_version\":1,\"command\":\"{}\",\"status\":\"error\",\"mode\":\"read-only\",\"mutation_executed\":false,\"error\":\"{}\"}}\n",
            json_escape(self.command),
            json_escape(&self.message)
        )
    }
}

impl fmt::Display for OpsctlError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl Error for OpsctlError {}

fn json_escape(value: &str) -> String {
    let mut output = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '"' => output.push_str("\\\""),
            '\\' => output.push_str("\\\\"),
            '\n' => output.push_str("\\n"),
            '\r' => output.push_str("\\r"),
            '\t' => output.push_str("\\t"),
            c if c.is_control() => output.push_str(&format!("\\u{:04x}", c as u32)),
            c => output.push(c),
        }
    }
    output
}

#[cfg(test)]
mod tests {
    use super::json_escape;

    #[test]
    fn json_errors_are_escaped() {
        assert_eq!(json_escape("a\n\"b\\c"), "a\\n\\\"b\\\\c");
    }
}
