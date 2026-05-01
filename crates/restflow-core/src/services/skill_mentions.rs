/// Parse explicit `@skill-id` mentions from user input.
pub fn parse_skill_mentions(input: &str) -> Vec<String> {
    let mut mentions = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let chars = input.char_indices().collect::<Vec<_>>();
    let mut index = 0usize;

    while index < chars.len() {
        let (byte_index, ch) = chars[index];
        if ch != '@' {
            index += 1;
            continue;
        }

        if byte_index > 0 {
            let previous = input[..byte_index].chars().next_back();
            if previous.is_some_and(|value| !value.is_whitespace()) {
                index += 1;
                continue;
            }
        }

        let mut end = byte_index + ch.len_utf8();
        let mut next_index = index + 1;
        while next_index < chars.len() {
            let (next_byte, next_ch) = chars[next_index];
            if !(next_ch.is_ascii_alphanumeric() || next_ch == '-' || next_ch == '_') {
                break;
            }
            end = next_byte + next_ch.len_utf8();
            next_index += 1;
        }

        if end > byte_index + 1 {
            let id = input[byte_index + 1..end].to_string();
            if seen.insert(id.clone()) {
                mentions.push(id);
            }
        }
        index = next_index.max(index + 1);
    }

    mentions
}

#[cfg(test)]
mod tests {
    use super::parse_skill_mentions;

    #[test]
    fn parses_single_skill_mention() {
        assert_eq!(parse_skill_mentions("@team review this"), vec!["team"]);
    }

    #[test]
    fn parses_multiple_unique_mentions() {
        assert_eq!(
            parse_skill_mentions("@team use @code-review and @team"),
            vec!["team", "code-review"]
        );
    }

    #[test]
    fn ignores_email_like_at_signs() {
        assert!(parse_skill_mentions("mail me at a@example.com").is_empty());
    }

    #[test]
    fn supports_chinese_text_around_mentions() {
        assert_eq!(parse_skill_mentions("请用 @team 并行处理"), vec!["team"]);
    }
}
