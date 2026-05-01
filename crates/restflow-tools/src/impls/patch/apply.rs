use anyhow::{Result, anyhow};

use super::parser::Hunk;

pub fn apply_hunks(original: &str, hunks: &[Hunk]) -> Result<String> {
    let mut lines: Vec<String> = original.lines().map(|line| line.to_string()).collect();

    for hunk in hunks {
        let position = find_hunk_position(&lines, hunk)?;
        let remove_count =
            hunk.context_before.len() + hunk.removals.len() + hunk.context_after.len();
        let mut new_lines = Vec::new();
        new_lines.extend(hunk.context_before.iter().cloned());
        new_lines.extend(hunk.additions.iter().cloned());
        new_lines.extend(hunk.context_after.iter().cloned());

        lines.splice(position..position + remove_count, new_lines);
    }

    Ok(lines.join("\n"))
}

fn find_hunk_position(lines: &[String], hunk: &Hunk) -> Result<usize> {
    let mut search_lines: Vec<&str> = Vec::new();
    search_lines.extend(hunk.context_before.iter().map(String::as_str));
    search_lines.extend(hunk.removals.iter().map(String::as_str));
    search_lines.extend(hunk.context_after.iter().map(String::as_str));

    if search_lines.is_empty() {
        return Err(anyhow!("Hunk has no searchable context"));
    }

    let mut first_match: Option<usize> = None;

    for i in 0..=lines.len().saturating_sub(search_lines.len()) {
        let mut matched = true;
        for (offset, expected) in search_lines.iter().enumerate() {
            if lines[i + offset] != *expected {
                matched = false;
                break;
            }
        }
        if matched {
            if first_match.is_some() {
                return Err(anyhow!(
                    "Ambiguous patch: hunk matches at multiple locations. Add more context lines to disambiguate."
                ));
            }
            first_match = Some(i);
        }
    }

    first_match.ok_or_else(|| anyhow!("Could not find matching context for hunk"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::impls::patch::parser::Hunk;

    #[test]
    fn apply_simple_hunk() {
        let original = "line1\nline2\nline3";
        let hunk = Hunk {
            context_before: vec!["line1".to_string()],
            removals: vec!["line2".to_string()],
            additions: vec!["line2b".to_string()],
            context_after: vec!["line3".to_string()],
        };

        let updated = apply_hunks(original, &[hunk]).unwrap();
        assert_eq!(updated, "line1\nline2b\nline3");
    }

    #[test]
    fn apply_hunk_rejects_ambiguous_match() {
        let original = "header\nline1\nline2\nline3\nsep\nline1\nline2\nline3\nfooter";
        let hunk = Hunk {
            context_before: vec!["line1".to_string()],
            removals: vec!["line2".to_string()],
            additions: vec!["line2b".to_string()],
            context_after: vec!["line3".to_string()],
        };

        let result = apply_hunks(original, &[hunk]);
        assert!(result.is_err());
        assert!(
            result
                .err()
                .unwrap()
                .to_string()
                .contains("Ambiguous patch")
        );
    }
}
