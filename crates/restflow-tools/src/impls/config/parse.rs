use serde_json::Value;

use crate::{Result, ToolError};

pub(crate) fn parse_u64(value: &Value, key: &str) -> Result<u64> {
    value
        .as_u64()
        .ok_or_else(|| ToolError::Tool(format!("{key} must be a number")))
}

pub(crate) fn parse_u32(value: &Value, key: &str) -> Result<u32> {
    Ok(parse_u64(value, key)? as u32)
}

pub(crate) fn parse_usize(value: &Value, key: &str) -> Result<usize> {
    Ok(parse_u64(value, key)? as usize)
}

#[allow(dead_code)]
pub(crate) fn parse_bool(value: &Value, key: &str) -> Result<bool> {
    value
        .as_bool()
        .ok_or_else(|| ToolError::Tool(format!("{key} must be a boolean")))
}

pub(crate) fn parse_optional_timeout(value: &Value, key: &str) -> Result<Option<u64>> {
    if value.is_null() {
        return Ok(None);
    }
    value
        .as_u64()
        .map(Some)
        .ok_or_else(|| ToolError::Tool(format!("{key} must be a number or null")))
}

pub(crate) fn parse_optional_string_list(value: &Value, key: &str) -> Result<Option<Vec<String>>> {
    if value.is_null() {
        return Ok(None);
    }

    let entries = value
        .as_array()
        .ok_or_else(|| ToolError::Tool(format!("{key} must be an array of strings or null")))?;

    let mut result = Vec::with_capacity(entries.len());
    for entry in entries {
        let text = entry
            .as_str()
            .ok_or_else(|| ToolError::Tool(format!("{key} must be an array of strings or null")))?;
        result.push(text.to_string());
    }

    Ok(Some(result))
}

#[allow(dead_code)]
pub(crate) fn parse_optional_string(value: &Value, key: &str) -> Result<Option<String>> {
    if value.is_null() {
        return Ok(None);
    }
    value
        .as_str()
        .map(|text| Some(text.to_string()))
        .ok_or_else(|| ToolError::Tool(format!("{key} must be a string or null")))
}

pub(crate) fn parse_string_list(value: &Value, key: &str) -> Result<Vec<String>> {
    let values = value
        .as_array()
        .ok_or_else(|| ToolError::Tool(format!("{key} must be an array of strings")))?;
    let mut result = Vec::with_capacity(values.len());
    for entry in values {
        let text = entry
            .as_str()
            .ok_or_else(|| ToolError::Tool(format!("{key} must be an array of strings")))?;
        result.push(text.to_string());
    }
    Ok(result)
}
