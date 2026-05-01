use colored::Colorize;

const SUGGESTION_HEADER: &str = "Suggestion:";

fn suggestions_for_message(msg: &str) -> Vec<Vec<String>> {
    let lower = msg.to_lowercase();
    let mut blocks = Vec::new();

    if lower.contains("api key not found")
        || lower.contains("missing api key")
        || lower.contains("no api key configured")
    {
        blocks.push(vec![
            "Set your API key with:".to_string(),
            format!(
                "{} restflow secret set ANTHROPIC_API_KEY <value>",
                "$".dimmed()
            ),
        ]);
    }

    if lower.contains("agent not found") {
        blocks.push(vec![
            "List available agents with:".to_string(),
            format!("{} restflow agent list", "$".dimmed()),
        ]);
    }

    if lower.contains("task not found") {
        blocks.push(vec![
            "Check daemon status and active runners with:".to_string(),
            format!("{} restflow daemon status", "$".dimmed()),
        ]);
    }

    if lower.contains("connection refused") || lower.contains("network") {
        blocks.push(vec![
            "Check your internet connection and try again.".to_string(),
        ]);
    }

    blocks
}

pub fn handle_error(err: anyhow::Error) -> ! {
    eprintln!("{} {}", "Error:".red().bold(), err);

    for lines in suggestions_for_message(&err.to_string()) {
        eprintln!("\n{}", SUGGESTION_HEADER.yellow().bold());
        for line in lines {
            eprintln!("  {}", line);
        }
    }

    std::process::exit(1);
}

#[cfg(test)]
mod tests {
    use super::suggestions_for_message;

    #[test]
    fn suggests_api_key_fix() {
        let suggestions = suggestions_for_message("No API key configured");
        let joined = suggestions
            .iter()
            .flat_map(|block| block.iter())
            .cloned()
            .collect::<Vec<String>>()
            .join("\n");
        assert!(joined.contains("restflow secret set ANTHROPIC_API_KEY"));
    }

    #[test]
    fn suggests_agent_list() {
        let suggestions = suggestions_for_message("agent not found: abc");
        let joined = suggestions
            .iter()
            .flat_map(|block| block.iter())
            .cloned()
            .collect::<Vec<String>>()
            .join("\n");
        assert!(joined.contains("restflow agent list"));
    }

    #[test]
    fn suggests_task_list() {
        let suggestions = suggestions_for_message("Task not found: my-task");
        let joined = suggestions
            .iter()
            .flat_map(|block| block.iter())
            .cloned()
            .collect::<Vec<String>>()
            .join("\n");
        assert!(joined.contains("restflow daemon status"));
    }

    #[test]
    fn suggests_network_hint() {
        let suggestions = suggestions_for_message("connection refused");
        let joined = suggestions
            .iter()
            .flat_map(|block| block.iter())
            .cloned()
            .collect::<Vec<String>>()
            .join("\n");
        assert!(joined.contains("internet connection"));
    }

    #[test]
    fn no_suggestion_for_unrelated_error() {
        let suggestions = suggestions_for_message("unexpected parse error");
        assert!(suggestions.is_empty());
    }
}
