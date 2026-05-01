use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct ShellAnalysis {
    pub segments: Vec<String>,
    pub has_pipe: bool,
    pub has_redirect: bool,
    pub has_chain: bool,
    pub has_subshell: bool,
    pub danger_reason: Option<String>,
}

pub fn analyze_command(command: &str) -> Result<ShellAnalysis, String> {
    if command.contains('\n') || command.contains('\r') {
        return Err("Command contains newlines".to_string());
    }

    // Tokenize first (shell_words handles quoting)
    let tokens = shell_words::split(command).map_err(|e| format!("Invalid shell syntax: {e}"))?;

    // Check for unquoted subshell patterns in the raw command
    if has_unquoted_subshell(command) {
        return Err("Command contains subshell".to_string());
    }

    let mut analysis = ShellAnalysis::default();
    for token in &tokens {
        match token.as_str() {
            "|" => analysis.has_pipe = true,
            ">" | ">>" | "<" | "<<" => analysis.has_redirect = true,
            "&&" | "||" | ";" => analysis.has_chain = true,
            _ => {
                analysis.segments.push(token.clone());
            }
        }
    }

    if analysis.has_pipe || analysis.has_redirect || analysis.has_chain {
        analysis.danger_reason = Some("Contains shell operators".to_string());
    }

    Ok(analysis)
}

/// Check if a command contains unquoted subshell patterns ($(...) or backticks).
fn has_unquoted_subshell(command: &str) -> bool {
    let mut in_single_quote = false;
    let mut in_double_quote = false;
    let mut prev_char = '\0';
    let chars: Vec<char> = command.chars().collect();

    for i in 0..chars.len() {
        let c = chars[i];
        match c {
            '\'' if !in_double_quote && prev_char != '\\' => in_single_quote = !in_single_quote,
            '"' if !in_single_quote && prev_char != '\\' => in_double_quote = !in_double_quote,
            '$' if !in_single_quote && i + 1 < chars.len() && chars[i + 1] == '(' => return true,
            // Backticks expand even inside double quotes, only single quotes suppress them
            '`' if !in_single_quote => return true,
            _ => {}
        }
        prev_char = c;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detects_pipe_operator() {
        let analysis = analyze_command("ls | grep foo").unwrap();
        assert!(analysis.has_pipe);
        assert!(analysis.danger_reason.is_some());
    }

    #[test]
    fn test_allows_pipe_in_quotes() {
        let analysis = analyze_command("echo 'a|b'").unwrap();
        assert!(!analysis.has_pipe);
        assert!(analysis.danger_reason.is_none());
    }

    #[test]
    fn test_detects_redirect_operator() {
        let analysis = analyze_command("echo hello > file.txt").unwrap();
        assert!(analysis.has_redirect);
        assert!(analysis.danger_reason.is_some());
    }

    #[test]
    fn test_detects_chain_operator() {
        let analysis = analyze_command("echo one && echo two").unwrap();
        assert!(analysis.has_chain);
        assert!(analysis.danger_reason.is_some());
    }

    #[test]
    fn test_blocks_subshell() {
        let result = analyze_command("$(whoami)");
        assert!(result.is_err());
    }

    #[test]
    fn test_subshell_in_single_quotes_allowed() {
        // Single-quoted $(...) is literal, not a subshell
        let result = analyze_command("echo '$(date)'");
        assert!(result.is_ok());
    }

    #[test]
    fn test_unquoted_subshell_blocked() {
        let result = analyze_command("echo $(date)");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("subshell"));
    }

    #[test]
    fn test_backtick_in_double_quotes_blocked() {
        // Backticks expand inside double quotes
        let result = analyze_command("echo \"`date`\"");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("subshell"));
    }

    #[test]
    fn test_backtick_in_single_quotes_allowed() {
        // Backticks in single quotes are literal
        let result = analyze_command("echo '`date`'");
        assert!(result.is_ok());
    }
}
