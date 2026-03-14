use super::*;
use tempfile::TempDir;

#[test]
fn test_file_tool_new() {
    let tool = FileTool::new();
    assert!(tool.base_dir.is_none());
    assert_eq!(tool.max_read_bytes, DEFAULT_MAX_READ_BYTES);
}

#[test]
fn test_file_tool_with_base_dir() {
    let tool = FileTool::new().with_base_dir("/tmp");
    assert_eq!(tool.base_dir, Some(PathBuf::from("/tmp")));
}

#[test]
fn test_file_tool_with_max_read() {
    let tool = FileTool::new().with_max_read(50_000);
    assert_eq!(tool.max_read_bytes, 50_000);
}

#[test]
fn test_file_tool_name() {
    let tool = FileTool::new();
    assert_eq!(tool.name(), "file");
}

#[test]
fn test_file_tool_description() {
    let tool = FileTool::new();
    assert!(tool.description().contains("file and directory operations"));
    assert!(tool.description().contains("use bash"));
}

#[test]
fn test_file_tool_schema() {
    let tool = FileTool::new();
    let schema = tool.parameters_schema();
    assert_eq!(schema["type"], "object");
    assert!(schema["properties"]["action"].is_object());
    assert!(schema["properties"]["path"].is_object());
}

#[test]
fn test_file_error_classification() {
    assert_eq!(
        classify_file_error_message("File not found: foo.txt"),
        (ToolErrorCategory::NotFound, false, None)
    );
    assert_eq!(
        classify_file_error_message("Cannot open file: Permission denied"),
        (ToolErrorCategory::Auth, false, None)
    );
}

#[test]
fn test_glob_match_exact() {
    assert!(glob_match("hello", "hello"));
    assert!(!glob_match("hello", "world"));
}

#[test]
fn test_glob_match_wildcard() {
    assert!(glob_match("*.rs", "main.rs"));
    assert!(glob_match("*.rs", "test.rs"));
    assert!(!glob_match("*.rs", "main.txt"));
}

#[test]
fn test_glob_match_question() {
    assert!(glob_match("test?.rs", "test1.rs"));
    assert!(glob_match("test?.rs", "testa.rs"));
    assert!(!glob_match("test?.rs", "test12.rs"));
}

#[test]
fn test_glob_match_complex() {
    assert!(glob_match("src/*.rs", "src/main.rs"));
    assert!(glob_match("**/test.rs", "src/test.rs"));
    assert!(glob_match("*.?s", "file.rs"));
}

#[test]
fn test_is_likely_binary() {
    assert!(is_likely_binary("image.png"));
    assert!(is_likely_binary("archive.zip"));
    assert!(is_likely_binary("video.MP4"));
    assert!(!is_likely_binary("code.rs"));
    assert!(!is_likely_binary("readme.md"));
}

#[test]
fn test_file_action_read_deserialization() {
    let action: FileAction = serde_json::from_value(serde_json::json!({
        "action": "read",
        "path": "/tmp/test.txt"
    }))
    .unwrap();

    match action {
        FileAction::Read {
            path,
            offset,
            limit,
        } => {
            assert_eq!(path, "/tmp/test.txt");
            assert_eq!(offset, 0);
            assert!(limit.is_none());
        }
        _ => panic!("Expected Read action"),
    }
}

#[test]
fn test_file_action_write_deserialization() {
    let action: FileAction = serde_json::from_value(serde_json::json!({
        "action": "write",
        "path": "/tmp/test.txt",
        "content": "hello world"
    }))
    .unwrap();

    match action {
        FileAction::Write {
            path,
            content,
            append,
        } => {
            assert_eq!(path, "/tmp/test.txt");
            assert_eq!(content, "hello world");
            assert!(!append);
        }
        _ => panic!("Expected Write action"),
    }
}

#[test]
fn test_file_action_list_deserialization() {
    let action: FileAction = serde_json::from_value(serde_json::json!({
        "action": "list",
        "path": "/tmp",
        "recursive": true,
        "pattern": "*.rs"
    }))
    .unwrap();

    match action {
        FileAction::List {
            path,
            recursive,
            pattern,
        } => {
            assert_eq!(path, "/tmp");
            assert!(recursive);
            assert_eq!(pattern, Some("*.rs".to_string()));
        }
        _ => panic!("Expected List action"),
    }
}

#[tokio::test]
async fn test_write_and_read_file() {
    let temp_dir = TempDir::new().unwrap();
    let tool = FileTool::new();

    let file_path = temp_dir.path().join("test.txt").display().to_string();

    // Write file
    let output = tool
        .execute(serde_json::json!({
            "action": "write",
            "path": &file_path,
            "content": "line 1\nline 2\nline 3"
        }))
        .await
        .unwrap();

    assert!(output.success);

    // Read file
    let output = tool
        .execute(serde_json::json!({
            "action": "read",
            "path": &file_path
        }))
        .await
        .unwrap();

    assert!(output.success);
    assert!(output.result["total_lines"].as_u64().unwrap() == 3);
}

#[tokio::test]
async fn test_write_append() {
    let temp_dir = TempDir::new().unwrap();
    let tool = FileTool::new();

    let file_path = temp_dir.path().join("append.txt").display().to_string();

    // Write initial content
    tool.execute(serde_json::json!({
        "action": "write",
        "path": &file_path,
        "content": "first\n"
    }))
    .await
    .unwrap();

    tool.execute(serde_json::json!({
        "action": "read",
        "path": &file_path
    }))
    .await
    .unwrap();

    // Append more content
    tool.execute(serde_json::json!({
        "action": "write",
        "path": &file_path,
        "content": "second\n",
        "append": true
    }))
    .await
    .unwrap();

    // Read and verify
    let output = tool
        .execute(serde_json::json!({
            "action": "read",
            "path": &file_path
        }))
        .await
        .unwrap();

    let content = output.result["content"].as_str().unwrap();
    assert!(content.contains("first"));
    assert!(content.contains("second"));
}

#[tokio::test]
async fn test_write_existing_file_requires_read_first() {
    let temp_dir = TempDir::new().unwrap();
    let file_path = temp_dir.path().join("existing.txt");
    fs::write(&file_path, "initial").await.unwrap();

    let tool = FileTool::new();
    let output = tool
        .execute(serde_json::json!({
            "action": "write",
            "path": file_path.display().to_string(),
            "content": "updated"
        }))
        .await
        .unwrap();

    assert!(!output.success);
    assert!(
        output
            .error
            .as_deref()
            .unwrap_or_default()
            .contains("You must read")
    );
}

#[tokio::test]
async fn test_write_new_file_without_read_succeeds() {
    let temp_dir = TempDir::new().unwrap();
    let file_path = temp_dir.path().join("new.txt");
    let tool = FileTool::new();

    let output = tool
        .execute(serde_json::json!({
            "action": "write",
            "path": file_path.display().to_string(),
            "content": "created"
        }))
        .await
        .unwrap();

    assert!(output.success);
    assert!(file_path.exists());
}

#[tokio::test]
async fn test_write_does_not_count_as_read_for_existing_file() {
    let temp_dir = TempDir::new().unwrap();
    let file_path = temp_dir.path().join("new.txt");
    let tool = FileTool::new();

    // First write creates a new file and is allowed without prior read.
    let first_write = tool
        .execute(serde_json::json!({
            "action": "write",
            "path": file_path.display().to_string(),
            "content": "v1"
        }))
        .await
        .unwrap();
    assert!(first_write.success);

    // Second write targets an existing file and must still require a read.
    let second_write = tool
        .execute(serde_json::json!({
            "action": "write",
            "path": file_path.display().to_string(),
            "content": "v2"
        }))
        .await
        .unwrap();
    assert!(!second_write.success);
    assert!(
        second_write
            .error
            .as_deref()
            .unwrap_or_default()
            .contains("You must read")
    );
}

#[tokio::test]
async fn test_list_directory() {
    let temp_dir = TempDir::new().unwrap();
    let tool = FileTool::new();

    // Create some files
    fs::write(temp_dir.path().join("file1.txt"), "content")
        .await
        .unwrap();
    fs::write(temp_dir.path().join("file2.rs"), "content")
        .await
        .unwrap();
    fs::create_dir(temp_dir.path().join("subdir"))
        .await
        .unwrap();

    let output = tool
        .execute(serde_json::json!({
            "action": "list",
            "path": temp_dir.path().display().to_string()
        }))
        .await
        .unwrap();

    assert!(output.success);
    assert!(output.result["count"].as_u64().unwrap() >= 3);
}

#[tokio::test]
async fn test_list_with_pattern() {
    let temp_dir = TempDir::new().unwrap();
    let tool = FileTool::new();

    // Create files
    fs::write(temp_dir.path().join("file1.txt"), "content")
        .await
        .unwrap();
    fs::write(temp_dir.path().join("file2.rs"), "content")
        .await
        .unwrap();
    fs::write(temp_dir.path().join("file3.txt"), "content")
        .await
        .unwrap();

    let output = tool
        .execute(serde_json::json!({
            "action": "list",
            "path": temp_dir.path().display().to_string(),
            "pattern": "*.txt"
        }))
        .await
        .unwrap();

    assert!(output.success);
    assert_eq!(output.result["count"].as_u64().unwrap(), 2);
}

#[tokio::test]
async fn test_search_files() {
    let temp_dir = TempDir::new().unwrap();
    let tool = FileTool::new();

    // Create files with content
    fs::write(
        temp_dir.path().join("file1.txt"),
        "hello world\ngoodbye world",
    )
    .await
    .unwrap();
    fs::write(temp_dir.path().join("file2.txt"), "no match here")
        .await
        .unwrap();

    let output = tool
        .execute(serde_json::json!({
            "action": "search",
            "path": temp_dir.path().display().to_string(),
            "pattern": "world"
        }))
        .await
        .unwrap();

    assert!(output.success);
    assert!(output.result["match_count"].as_u64().unwrap() >= 2);
}

#[tokio::test]
async fn test_exists() {
    let temp_dir = TempDir::new().unwrap();
    let tool = FileTool::new();

    let file_path = temp_dir.path().join("exists.txt");
    fs::write(&file_path, "content").await.unwrap();

    // Check existing file
    let output = tool
        .execute(serde_json::json!({
            "action": "exists",
            "path": file_path.display().to_string()
        }))
        .await
        .unwrap();

    assert!(output.success);
    assert!(output.result["exists"].as_bool().unwrap());
    assert_eq!(output.result["type"].as_str().unwrap(), "file");

    // Check non-existing file
    let output = tool
        .execute(serde_json::json!({
            "action": "exists",
            "path": temp_dir.path().join("nonexistent.txt").display().to_string()
        }))
        .await
        .unwrap();

    assert!(output.success);
    assert!(!output.result["exists"].as_bool().unwrap());
}

#[tokio::test]
async fn test_delete_file() {
    let temp_dir = TempDir::new().unwrap();
    let tool = FileTool::new();

    let file_path = temp_dir.path().join("delete_me.txt");
    fs::write(&file_path, "content").await.unwrap();
    assert!(file_path.exists());

    // Read first to satisfy read-before-delete guard
    let read_output = tool
        .execute(serde_json::json!({
            "action": "read",
            "path": file_path.display().to_string()
        }))
        .await
        .unwrap();
    assert!(read_output.success);

    let output = tool
        .execute(serde_json::json!({
            "action": "delete",
            "path": file_path.display().to_string()
        }))
        .await
        .unwrap();

    assert!(output.success);
    assert!(!file_path.exists());
}

#[tokio::test]
async fn test_delete_file_requires_read_first() {
    let temp_dir = TempDir::new().unwrap();
    let tool = FileTool::new();

    let file_path = temp_dir.path().join("delete_requires_read.txt");
    fs::write(&file_path, "content").await.unwrap();

    let output = tool
        .execute(serde_json::json!({
            "action": "delete",
            "path": file_path.display().to_string()
        }))
        .await
        .unwrap();

    assert!(!output.success);
    assert!(
        output
            .error
            .as_deref()
            .unwrap_or_default()
            .contains("must read")
    );
    assert!(file_path.exists());
}

#[tokio::test]
async fn test_read_with_offset_and_limit() {
    let temp_dir = TempDir::new().unwrap();
    let tool = FileTool::new();

    let file_path = temp_dir.path().join("lines.txt");
    fs::write(&file_path, "line 0\nline 1\nline 2\nline 3\nline 4")
        .await
        .unwrap();

    let output = tool
        .execute(serde_json::json!({
            "action": "read",
            "path": file_path.display().to_string(),
            "offset": 1,
            "limit": 2
        }))
        .await
        .unwrap();

    assert!(output.success);
    let content = output.result["content"].as_str().unwrap();
    assert!(content.contains("line 1"));
    assert!(content.contains("line 2"));
    assert!(!content.contains("line 0"));
    assert!(!content.contains("line 3"));
}

#[tokio::test]
async fn test_base_dir_restriction() {
    let temp_dir = TempDir::new().unwrap();
    let tool = FileTool::new().with_base_dir(temp_dir.path());

    // Try to escape base directory
    let output = tool
        .execute(serde_json::json!({
            "action": "read",
            "path": "../../../etc/passwd"
        }))
        .await
        .unwrap();

    assert!(!output.success);
    assert!(
        output
            .error
            .as_ref()
            .unwrap()
            .contains("escapes allowed base directory")
    );
}

#[tokio::test]
#[cfg(unix)]
async fn test_base_dir_symlink_escape_blocked() {
    use std::os::unix::fs::symlink;

    let base_dir = TempDir::new().unwrap();
    let outside_dir = TempDir::new().unwrap();
    let tool = FileTool::new().with_base_dir(base_dir.path());

    let link_path = base_dir.path().join("link");
    symlink(outside_dir.path(), &link_path).unwrap();

    let output = tool
        .execute(serde_json::json!({
            "action": "write",
            "path": "link/newfile.txt",
            "content": "nope"
        }))
        .await
        .unwrap();

    assert!(!output.success);
    assert!(
        output
            .error
            .as_ref()
            .unwrap()
            .contains("escapes allowed base directory")
    );
}

#[tokio::test]
async fn test_read_nonexistent_file() {
    let tool = FileTool::new();

    let output = tool
        .execute(serde_json::json!({
            "action": "read",
            "path": "/nonexistent/path/file.txt"
        }))
        .await
        .unwrap();

    assert!(!output.success);
    assert!(output.error.as_ref().unwrap().contains("not found"));
}

#[tokio::test]
async fn test_write_creates_parent_dirs() {
    let temp_dir = TempDir::new().unwrap();
    let tool = FileTool::new();

    let deep_path = temp_dir.path().join("a/b/c/file.txt");

    let output = tool
        .execute(serde_json::json!({
            "action": "write",
            "path": deep_path.display().to_string(),
            "content": "nested content"
        }))
        .await
        .unwrap();

    assert!(output.success);
    assert!(deep_path.exists());
}

#[tokio::test]
async fn test_batch_read_multiple_files() {
    let temp_dir = TempDir::new().unwrap();
    let tool = FileTool::new();

    // Create test files
    fs::write(temp_dir.path().join("file1.txt"), "content 1")
        .await
        .unwrap();
    fs::write(temp_dir.path().join("file2.txt"), "content 2")
        .await
        .unwrap();
    fs::write(temp_dir.path().join("file3.txt"), "content 3")
        .await
        .unwrap();

    let output = tool
        .execute(serde_json::json!({
            "action": "batch_read",
            "paths": [
                temp_dir.path().join("file1.txt").display().to_string(),
                temp_dir.path().join("file2.txt").display().to_string(),
                temp_dir.path().join("file3.txt").display().to_string()
            ]
        }))
        .await
        .unwrap();

    assert!(output.success);
    assert_eq!(output.result["total"].as_u64().unwrap(), 3);
    assert_eq!(output.result["successful"].as_u64().unwrap(), 3);
    assert_eq!(output.result["failed"].as_u64().unwrap(), 0);
}

#[tokio::test]
async fn test_batch_read_partial_failure() {
    let temp_dir = TempDir::new().unwrap();
    let tool = FileTool::new();

    // Create one file, leave others missing
    fs::write(temp_dir.path().join("exists.txt"), "content")
        .await
        .unwrap();

    let output = tool
        .execute(serde_json::json!({
            "action": "batch_read",
            "paths": [
                temp_dir.path().join("exists.txt").display().to_string(),
                temp_dir.path().join("missing.txt").display().to_string()
            ],
            "continue_on_error": true
        }))
        .await
        .unwrap();

    assert!(output.success); // continue_on_error = true
    assert_eq!(output.result["total"].as_u64().unwrap(), 2);
    assert_eq!(output.result["successful"].as_u64().unwrap(), 1);
    assert_eq!(output.result["failed"].as_u64().unwrap(), 1);
}

#[tokio::test]
async fn test_batch_read_missing_file_error_includes_path() {
    let temp_dir = TempDir::new().unwrap();
    let tool = FileTool::new();
    let missing_path = temp_dir.path().join("missing.txt");

    let output = tool
        .execute(serde_json::json!({
            "action": "batch_read",
            "paths": [missing_path.display().to_string()],
            "continue_on_error": true
        }))
        .await
        .unwrap();

    assert!(output.success);
    let error = output.result["results"][0]["error"].as_str().unwrap();
    assert!(error.contains("File not found:"));
    assert!(error.contains(missing_path.display().to_string().as_str()));
}

#[tokio::test]
async fn test_batch_read_large_file_error_has_partial_read_hint() {
    let temp_dir = TempDir::new().unwrap();
    let tool = FileTool::new();
    let large_file = temp_dir.path().join("large.txt");
    fs::write(&large_file, "0123456789").await.unwrap();

    let output = tool
        .execute(serde_json::json!({
            "action": "batch_read",
            "paths": [large_file.display().to_string()],
            "max_file_size": 5,
            "continue_on_error": true
        }))
        .await
        .unwrap();

    assert!(output.success);
    let error = output.result["results"][0]["error"].as_str().unwrap();
    assert!(error.contains("Use offset and limit parameters for partial reads."));
}

#[tokio::test]
async fn test_batch_read_exceeds_limit() {
    let tool = FileTool::new();

    // Try to read more files than allowed
    let paths: Vec<String> = (0..25).map(|i| format!("/tmp/file{}.txt", i)).collect();

    let output = tool
        .execute(serde_json::json!({
            "action": "batch_read",
            "paths": paths
        }))
        .await
        .unwrap();

    assert!(!output.success);
    assert!(output.error.as_ref().unwrap().contains("exceeds maximum"));
}

#[tokio::test]
async fn test_batch_exists_mixed() {
    let temp_dir = TempDir::new().unwrap();
    let tool = FileTool::new();

    // Create some paths
    fs::write(temp_dir.path().join("file.txt"), "content")
        .await
        .unwrap();
    fs::create_dir(temp_dir.path().join("subdir"))
        .await
        .unwrap();

    let output = tool
        .execute(serde_json::json!({
            "action": "batch_exists",
            "paths": [
                temp_dir.path().join("file.txt").display().to_string(),
                temp_dir.path().join("subdir").display().to_string(),
                temp_dir.path().join("missing.txt").display().to_string()
            ]
        }))
        .await
        .unwrap();

    assert!(output.success);
    assert_eq!(output.result["total"].as_u64().unwrap(), 3);
    assert_eq!(output.result["existing"].as_u64().unwrap(), 2);

    let results = output.result["results"].as_array().unwrap();
    assert!(results[0]["exists"].as_bool().unwrap());
    assert!(results[0]["is_file"].as_bool().unwrap());
    assert!(results[1]["exists"].as_bool().unwrap());
    assert!(results[1]["is_dir"].as_bool().unwrap());
    assert!(!results[2]["exists"].as_bool().unwrap());
}

#[tokio::test]
async fn test_batch_search_single_location() {
    let temp_dir = TempDir::new().unwrap();
    let tool = FileTool::new();

    // Create files with searchable content
    fs::write(temp_dir.path().join("file1.txt"), "hello world\ntest line")
        .await
        .unwrap();
    fs::write(temp_dir.path().join("file2.txt"), "no match here")
        .await
        .unwrap();
    fs::write(temp_dir.path().join("file3.txt"), "another hello")
        .await
        .unwrap();

    let output = tool
        .execute(serde_json::json!({
            "action": "batch_search",
            "pattern": "hello",
            "locations": [temp_dir.path().display().to_string()]
        }))
        .await
        .unwrap();

    assert!(output.success);
    assert_eq!(output.result["total_matches"].as_u64().unwrap(), 2);
}

#[tokio::test]
async fn test_batch_search_with_context() {
    let temp_dir = TempDir::new().unwrap();
    let tool = FileTool::new();

    fs::write(
        temp_dir.path().join("test.txt"),
        "line 1\nline 2\nTARGET\nline 4\nline 5",
    )
    .await
    .unwrap();

    let output = tool
        .execute(serde_json::json!({
            "action": "batch_search",
            "pattern": "TARGET",
            "locations": [temp_dir.path().display().to_string()],
            "context_lines": 2
        }))
        .await
        .unwrap();

    assert!(output.success);
    let results = output.result["results"].as_array().unwrap();
    let matches = results[0]["matches"].as_array().unwrap();
    assert_eq!(matches.len(), 1);

    let m = &matches[0];
    assert_eq!(m["line_number"].as_u64().unwrap(), 3);
    assert_eq!(m["content"].as_str().unwrap(), "TARGET");
    assert_eq!(m["context_before"].as_array().unwrap().len(), 2);
    assert_eq!(m["context_after"].as_array().unwrap().len(), 2);
}
