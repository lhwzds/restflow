use anyhow::{bail, Result};
use chrono::{DateTime, Local, TimeZone};
use restflow_core::models::AIModel;
use std::io::{self, Read};

pub fn format_timestamp(timestamp: Option<i64>) -> String {
    let Some(ts) = timestamp else {
        return "-".to_string();
    };

    let datetime: DateTime<Local> = match Local.timestamp_millis_opt(ts).single() {
        Some(dt) => dt,
        None => return "-".to_string(),
    };

    datetime.format("%Y-%m-%d %H:%M:%S").to_string()
}

pub fn read_stdin_to_string() -> Result<String> {
    let mut buffer = String::new();
    io::stdin().read_to_string(&mut buffer)?;
    Ok(buffer.trim().to_string())
}

pub fn parse_model(input: &str) -> Result<AIModel> {
    let normalized = input.trim().to_lowercase();
    let model = match normalized.as_str() {
        "gpt-5" => AIModel::Gpt5,
        "gpt-5-mini" => AIModel::Gpt5Mini,
        "gpt-5-nano" => AIModel::Gpt5Nano,
        "gpt-5-pro" => AIModel::Gpt5Pro,
        "o4-mini" => AIModel::O4Mini,
        "o3" => AIModel::O3,
        "o3-mini" => AIModel::O3Mini,
        "claude-opus-4-1" => AIModel::ClaudeOpus4_1,
        "claude-sonnet-4-5" => AIModel::ClaudeSonnet4_5,
        "claude-haiku-4-5" => AIModel::ClaudeHaiku4_5,
        "deepseek-chat" => AIModel::DeepseekChat,
        "deepseek-reasoner" => AIModel::DeepseekReasoner,
        _ => {
            bail!("Unknown model: {input}")
        }
    };

    Ok(model)
}

pub fn slugify(input: &str) -> String {
    let mut output = String::new();
    let mut last_dash = false;

    for ch in input.chars() {
        if ch.is_ascii_alphanumeric() {
            output.push(ch.to_ascii_lowercase());
            last_dash = false;
        } else if (ch.is_whitespace() || ch == '-' || ch == '_') && !last_dash && !output.is_empty() {
            output.push('-');
            last_dash = true;
        }
    }

    if output.ends_with('-') {
        output.pop();
    }

    if output.is_empty() {
        "skill".to_string()
    } else {
        output
    }
}

pub fn preview_text(input: &str, max_len: usize) -> String {
    if input.len() <= max_len {
        return input.to_string();
    }

    let mut preview = input.chars().take(max_len).collect::<String>();
    preview.push('…');
    preview
}
