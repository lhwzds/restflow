use serde::Deserialize;
use serde_json::Value;

pub(crate) fn default_batch_line_limit() -> usize {
    super::DEFAULT_BATCH_LINE_LIMIT
}

pub(crate) fn default_batch_max_size() -> usize {
    super::DEFAULT_BATCH_MAX_FILE_SIZE
}

pub(crate) fn default_batch_max_matches() -> usize {
    super::DEFAULT_BATCH_MAX_MATCHES
}

pub(crate) fn default_context_lines() -> usize {
    super::DEFAULT_BATCH_CONTEXT_LINES
}

pub(crate) fn default_continue_on_error() -> bool {
    true
}

pub(super) fn file_parameters_schema() -> Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "action": {
                "type": "string",
                "enum": ["read", "write", "list", "search", "delete", "exists", "batch_read", "batch_exists", "batch_search"],
                "description": "The file operation to perform"
            },
            "path": {
                "type": "string",
                "description": "File or directory path (for single-file operations)"
            },
            "paths": {
                "type": "array",
                "items": { "type": "string" },
                "description": "List of file paths (for batch_read, batch_exists)"
            },
            "locations": {
                "type": "array",
                "items": { "type": "string" },
                "description": "List of directories or globs to search (for batch_search)"
            },
            "content": {
                "type": "string",
                "description": "Content to write (for write action)"
            },
            "pattern": {
                "type": "string",
                "description": "Search pattern (regex for search/batch_search, glob for list)"
            },
            "file_pattern": {
                "type": "string",
                "description": "Filter files by glob pattern (for search action)"
            },
            "append": {
                "type": "boolean",
                "description": "Append to file instead of overwrite"
            },
            "recursive": {
                "type": "boolean",
                "description": "List directories recursively"
            },
            "offset": {
                "type": "integer",
                "description": "Start reading from this line number (0-indexed)"
            },
            "limit": {
                "type": "integer",
                "description": "Maximum lines to read"
            },
            "line_limit": {
                "type": "integer",
                "description": "Max lines per file in batch_read (default: 500)"
            },
            "max_file_size": {
                "type": "integer",
                "description": "Skip files larger than this in batch_read (default: 500KB)"
            },
            "max_matches": {
                "type": "integer",
                "description": "Max total matches in batch_search (default: 100)"
            },
            "context_lines": {
                "type": "integer",
                "description": "Context lines before/after matches in batch_search (default: 2)"
            },
            "continue_on_error": {
                "type": "boolean",
                "description": "Continue batch on individual errors (default: true)"
            }
        },
        "required": ["action"]
    })
}

#[derive(Debug, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum FileAction {
    Read {
        path: String,
        #[serde(default)]
        offset: usize,
        #[serde(default)]
        limit: Option<usize>,
    },
    Write {
        path: String,
        content: String,
        #[serde(default)]
        append: bool,
    },
    List {
        path: String,
        #[serde(default)]
        recursive: bool,
        #[serde(default)]
        pattern: Option<String>,
    },
    Search {
        path: String,
        pattern: String,
        #[serde(default)]
        file_pattern: Option<String>,
    },
    Delete {
        path: String,
    },
    Exists {
        path: String,
    },
    BatchRead {
        paths: Vec<String>,
        #[serde(default = "default_batch_line_limit")]
        line_limit: usize,
        #[serde(default = "default_batch_max_size")]
        max_file_size: usize,
        #[serde(default = "default_continue_on_error")]
        continue_on_error: bool,
    },
    BatchExists {
        paths: Vec<String>,
    },
    BatchSearch {
        pattern: String,
        locations: Vec<String>,
        #[serde(default = "default_batch_max_matches")]
        max_matches: usize,
        #[serde(default = "default_context_lines")]
        context_lines: usize,
    },
}
