pub fn ensure_success_output(content: &str, operation: &str, _verification: &str) -> String {
    let trimmed = content.trim();
    if !trimmed.is_empty() {
        return trimmed.to_string();
    }

    let operation = operation.trim();
    if operation.is_empty() {
        "Done.".to_string()
    } else {
        operation.to_string()
    }
}

pub fn format_error_output(error_detail: &str, operation: &str, verification: &str) -> String {
    let detail = error_detail.trim();
    if !detail.is_empty() {
        return detail.to_string();
    }

    let operation = operation.trim();
    let verification = verification.trim();
    match (operation.is_empty(), verification.is_empty()) {
        (false, false) => format!("Execution failed. {operation} {verification}"),
        (false, true) => format!("Execution failed. {operation}"),
        (true, false) => format!("Execution failed. {verification}"),
        (true, true) => "Execution failed.".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::{ensure_success_output, format_error_output};

    #[test]
    fn ensure_success_output_keeps_plain_text() {
        let rendered = ensure_success_output(
            "Final answer payload",
            "Executed the requested agent workflow.",
            "Checked for execution-time failures.",
        );

        assert_eq!(rendered, "Final answer payload");
    }

    #[test]
    fn ensure_success_output_uses_operation_when_content_empty() {
        let rendered = ensure_success_output("", "Operation fallback", "Verification fallback");
        assert_eq!(rendered, "Operation fallback");
    }

    #[test]
    fn ensure_success_output_uses_default_when_all_empty() {
        let rendered = ensure_success_output("", "", "");
        assert_eq!(rendered, "Done.");
    }

    #[test]
    fn format_error_output_keeps_detail() {
        let rendered = format_error_output(
            "Tool call failed: timeout",
            "Tried to execute the task with configured tools.",
            "Failure confirmed from runtime error path.",
        );

        assert_eq!(rendered, "Tool call failed: timeout");
    }

    #[test]
    fn format_error_output_builds_fallback_message() {
        let rendered = format_error_output(
            "",
            "Attempted to process request.",
            "Please retry after fixing configuration.",
        );

        assert_eq!(
            rendered,
            "Execution failed. Attempted to process request. Please retry after fixing configuration."
        );
    }
}
