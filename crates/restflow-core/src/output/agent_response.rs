const EVIDENCE_HEADER: &str = "### Evidence";
const OPERATION_HEADER: &str = "### Operation";
const VERIFICATION_HEADER: &str = "### Verification";

fn normalize_section_content(content: &str, fallback: &str) -> String {
    let trimmed = content.trim();
    if trimmed.is_empty() {
        fallback.to_string()
    } else {
        trimmed.to_string()
    }
}

fn has_contract_sections(content: &str) -> bool {
    let lower = content.to_ascii_lowercase();
    lower.contains("evidence") && lower.contains("operation") && lower.contains("verification")
}

fn format_contract(evidence: &str, operation: &str, verification: &str) -> String {
    format!(
        "{EVIDENCE_HEADER}\n{}\n\n{OPERATION_HEADER}\n{}\n\n{VERIFICATION_HEADER}\n{}",
        normalize_section_content(evidence, "No concrete evidence available."),
        normalize_section_content(operation, "Operation details are unavailable."),
        normalize_section_content(verification, "Verification details are unavailable.")
    )
}

pub fn ensure_success_output(content: &str, operation: &str, verification: &str) -> String {
    let trimmed = content.trim();
    if has_contract_sections(trimmed) {
        return trimmed.to_string();
    }

    format_contract(trimmed, operation, verification)
}

pub fn format_error_output(error_detail: &str, operation: &str, verification: &str) -> String {
    format_contract(error_detail, operation, verification)
}

#[cfg(test)]
mod tests {
    use super::{ensure_success_output, format_error_output};

    #[test]
    fn ensure_success_output_wraps_plain_text() {
        let rendered = ensure_success_output(
            "Final answer payload",
            "Executed the requested agent workflow.",
            "Checked for execution-time failures.",
        );

        assert!(rendered.contains("### Evidence"));
        assert!(rendered.contains("### Operation"));
        assert!(rendered.contains("### Verification"));
        assert!(rendered.contains("Final answer payload"));
    }

    #[test]
    fn ensure_success_output_keeps_existing_contract() {
        let existing = "### Evidence\nA\n\n### Operation\nB\n\n### Verification\nC";
        let rendered =
            ensure_success_output(existing, "Operation fallback", "Verification fallback");

        assert_eq!(rendered, existing);
    }

    #[test]
    fn format_error_output_renders_contract_sections() {
        let rendered = format_error_output(
            "Tool call failed: timeout",
            "Tried to execute the task with configured tools.",
            "Failure confirmed from runtime error path.",
        );

        assert!(rendered.contains("Tool call failed: timeout"));
        assert!(rendered.contains("### Evidence"));
        assert!(rendered.contains("### Operation"));
        assert!(rendered.contains("### Verification"));
    }
}
