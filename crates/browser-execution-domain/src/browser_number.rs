/// Canonicalize a browser numeric value according to JavaScript-number semantics.
///
/// Browser-visible numeric capability values cross JSON/IPC boundaries where equivalent values can
/// have different textual spellings (`1`, `1.0`, `1e0`, `-0`). The semantic domain owns their
/// normalized representation so config and observation adapters cannot create identity drift by
/// choosing a serializer spelling.
#[must_use]
pub fn canonical_browser_number(value: f64) -> Option<String> {
    if !value.is_finite() {
        return None;
    }
    let normalized = if value == 0.0 { 0.0 } else { value };
    Some(normalized.to_string())
}

#[cfg(test)]
mod tests {
    use super::canonical_browser_number;

    #[test]
    fn equivalent_javascript_numbers_have_one_canonical_spelling() {
        assert_eq!(canonical_browser_number(1.0).as_deref(), Some("1"));
        assert_eq!(canonical_browser_number(1e0).as_deref(), Some("1"));
        assert_eq!(canonical_browser_number(-0.0).as_deref(), Some("0"));
        assert_eq!(canonical_browser_number(0.0).as_deref(), Some("0"));
    }

    #[test]
    fn non_finite_numbers_are_not_identity_values() {
        assert_eq!(canonical_browser_number(f64::NAN), None);
        assert_eq!(canonical_browser_number(f64::INFINITY), None);
        assert_eq!(canonical_browser_number(f64::NEG_INFINITY), None);
    }
}
