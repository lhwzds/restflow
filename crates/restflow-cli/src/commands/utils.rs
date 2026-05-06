use chrono::{DateTime, Local, TimeZone};

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

pub fn short_id(value: &str) -> String {
    value.chars().take(8).collect()
}

pub fn preview_text(input: &str, max_len: usize) -> String {
    if input.len() <= max_len {
        return input.to_string();
    }

    let mut preview = input.chars().take(max_len).collect::<String>();
    preview.push('…');
    preview
}
