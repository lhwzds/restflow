use syntect::{
    easy::HighlightLines,
    highlighting::{Theme, ThemeSet},
    parsing::SyntaxSet,
    util::as_24_bit_terminal_escaped,
};

pub struct SyntaxHighlighter {
    syntax_set: SyntaxSet,
    theme: Theme,
}

impl SyntaxHighlighter {
    pub fn new(theme_name: &str) -> Self {
        let syntax_set = SyntaxSet::load_defaults_newlines();
        let theme_set = ThemeSet::load_defaults();
        let theme = theme_set
            .themes
            .get(theme_name)
            .cloned()
            .unwrap_or_else(|| theme_set.themes["base16-ocean.dark"].clone());

        Self { syntax_set, theme }
    }

    pub fn highlight_block(&self, code: &str, language: &str) -> Vec<String> {
        let syntax = if language.trim().is_empty() {
            self.syntax_set.find_syntax_plain_text()
        } else {
            self.syntax_set
                .find_syntax_by_token(language)
                .unwrap_or_else(|| self.syntax_set.find_syntax_plain_text())
        };

        let mut highlighter = HighlightLines::new(syntax, &self.theme);
        let mut lines = Vec::new();

        for line in syntect::util::LinesWithEndings::from(code) {
            match highlighter.highlight_line(line, &self.syntax_set) {
                Ok(ranges) => {
                    let escaped = as_24_bit_terminal_escaped(&ranges, false);
                    lines.push(escaped.trim_end_matches('\n').to_string());
                }
                Err(_) => lines.push(line.trim_end_matches('\n').to_string()),
            }
        }

        if lines.is_empty() {
            lines.push(String::new());
        }

        lines
    }
}

pub fn theme_for_config(theme: &str) -> &str {
    match theme.to_lowercase().as_str() {
        "light" => "base16-ocean.light",
        _ => "base16-ocean.dark",
    }
}
