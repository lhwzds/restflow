use std::collections::HashMap;

/// Render placeholders in a single pass to prevent second-order substitutions.
///
/// Placeholder keys must include delimiters (for example, `"{{task_id}}"`).
/// Unknown placeholders are kept unchanged.
pub fn render_template_single_pass(template: &str, replacements: &HashMap<&str, &str>) -> String {
    let mut rendered = String::with_capacity(template.len());
    let mut rest = template;

    while let Some(start) = rest.find("{{") {
        rendered.push_str(&rest[..start]);
        if let Some(end_offset) = rest[start..].find("}}") {
            let key = &rest[start..start + end_offset + 2];
            if let Some(value) = replacements.get(key) {
                rendered.push_str(value);
            } else {
                rendered.push_str(key);
            }
            rest = &rest[start + end_offset + 2..];
        } else {
            rendered.push_str(&rest[start..]);
            rest = "";
        }
    }
    rendered.push_str(rest);
    rendered
}

#[cfg(test)]
mod tests {
    use super::render_template_single_pass;
    use std::collections::HashMap;

    #[test]
    fn test_render_template_single_pass_basic_substitution() {
        let replacements = HashMap::from([("{{name}}", "world"), ("{{id}}", "task-1")]);
        let rendered = render_template_single_pass("hello {{name}}: {{id}}", &replacements);
        assert_eq!(rendered, "hello world: task-1");
    }

    #[test]
    fn test_render_template_single_pass_prevents_double_substitution() {
        let replacements = HashMap::from([
            ("{{output}}", "injected {{task_id}}"),
            ("{{task_id}}", "task-123"),
        ]);
        let rendered = render_template_single_pass("value={{output}}", &replacements);
        assert_eq!(rendered, "value=injected {{task_id}}");
    }

    #[test]
    fn test_render_template_single_pass_keeps_unknown_placeholders() {
        let replacements = HashMap::from([("{{known}}", "ok")]);
        let rendered = render_template_single_pass("{{known}} {{unknown}}", &replacements);
        assert_eq!(rendered, "ok {{unknown}}");
    }

    #[test]
    fn test_render_template_single_pass_handles_unclosed_placeholder() {
        let replacements = HashMap::from([("{{known}}", "ok")]);
        let rendered = render_template_single_pass("prefix {{known", &replacements);
        assert_eq!(rendered, "prefix {{known");
    }
}
