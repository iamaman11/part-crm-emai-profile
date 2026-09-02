/// Canonicalize a browser numeric value by its JavaScript/IEEE-754 binary64 semantics.
///
/// Browser-visible numeric capability values cross JSON/IPC boundaries where equivalent values can
/// have different textual spellings (`1`, `1.0`, `1e0`, `-0`). The semantic domain owns their
/// normalized representation so config and observation adapters cannot create identity drift by
/// choosing a serializer spelling. Finite values are represented by exact binary64 bits; negative
/// zero is normalized to positive zero because JavaScript numeric equality does not distinguish it
/// for the capability surfaces governed by this contract.
#[must_use]
pub fn canonical_browser_number(value: f64) -> Option<String> {
    if !value.is_finite() {
        return None;
    }
    let normalized = if value == 0.0 { 0.0 } else { value };
    Some(format!("{:016x}", normalized.to_bits()))
}

#[cfg(test)]
mod tests {
    use super::canonical_browser_number;

    #[test]
    fn equivalent_javascript_numbers_have_one_canonical_identity() {
        assert_eq!(
            canonical_browser_number(1.0).as_deref(),
            Some("3ff0000000000000")
        );
        assert_eq!(canonical_browser_number(1e0), canonical_browser_number(1.0));
        assert_eq!(
            canonical_browser_number(-0.0),
            canonical_browser_number(0.0)
        );
        assert_eq!(
            canonical_browser_number(0.0).as_deref(),
            Some("0000000000000000")
        );
    }

    #[test]
    fn distinct_binary64_values_remain_distinct() {
        assert_ne!(canonical_browser_number(1.0), canonical_browser_number(1.5));
        assert_ne!(
            canonical_browser_number(f64::MIN_POSITIVE),
            canonical_browser_number(0.0)
        );
    }

    #[test]
    fn non_finite_numbers_are_not_identity_values() {
        assert_eq!(canonical_browser_number(f64::NAN), None);
        assert_eq!(canonical_browser_number(f64::INFINITY), None);
        assert_eq!(canonical_browser_number(f64::NEG_INFINITY), None);
    }
}
