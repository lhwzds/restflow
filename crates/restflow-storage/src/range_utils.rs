//! Range query helpers for prefix scans.

/// Calculate the exclusive end bound for a prefix range query.
///
/// Given prefix "agent-001:", returns "agent-001;" (next ASCII char after ':').
/// This allows efficient range scans: range(prefix..end_prefix)
pub fn prefix_end_bound(prefix: &str) -> String {
    if prefix.is_empty() {
        return String::new();
    }

    let mut bytes = prefix.as_bytes().to_vec();
    if let Some(last) = bytes.last_mut() {
        *last = last.saturating_add(1);
    }

    String::from_utf8(bytes).unwrap_or_else(|_| format!("{}\x7F", prefix))
}

/// Create a prefix range for redb queries.
pub fn prefix_range(prefix: &str) -> (String, String) {
    (prefix.to_string(), prefix_end_bound(prefix))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_prefix_end_bound() {
        assert_eq!(prefix_end_bound("agent:"), "agent;");
        assert_eq!(prefix_end_bound("tag:rust:"), "tag:rust;");
        assert_eq!(prefix_end_bound(""), "");
    }
}
