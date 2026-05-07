use std::iter::Peekable;
use std::str::Lines;

use anyhow::{Result, anyhow};

#[derive(Debug, Clone)]
pub enum PatchOperation {
    Update { path: String, hunks: Vec<Hunk> },
    Add { path: String, content: String },
    Delete { path: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hunk {
    pub context_before: Vec<String>,
    pub removals: Vec<String>,
    pub additions: Vec<String>,
    pub context_after: Vec<String>,
}

pub fn parse_patch(text: &str) -> Result<Vec<PatchOperation>> {
    let mut operations = Vec::new();
    let mut lines = text.lines().peekable();

    while let Some(line) = lines.next() {
        if let Some(path) = line.strip_prefix("*** Update File: ") {
            let block = collect_block(&mut lines);
            let hunks = parse_hunks(&block)?;
            operations.push(PatchOperation::Update {
                path: path.trim().to_string(),
                hunks,
            });
            continue;
        }

        if let Some(path) = line.strip_prefix("*** Add File: ") {
            let block = collect_block(&mut lines);
            let content = parse_added_content(&block);
            operations.push(PatchOperation::Add {
                path: path.trim().to_string(),
                content,
            });
            continue;
        }

        if let Some(path) = line.strip_prefix("*** Delete File: ") {
            operations.push(PatchOperation::Delete {
                path: path.trim().to_string(),
            });
            continue;
        }
    }

    if operations.is_empty() {
        return Err(anyhow!("No valid patch operations found"));
    }

    Ok(operations)
}

fn collect_block(lines: &mut Peekable<Lines<'_>>) -> Vec<String> {
    let mut block = Vec::new();
    while let Some(line) = lines.peek() {
        if line.starts_with("*** ") {
            break;
        }
        block.push(lines.next().unwrap_or_default().to_string());
    }
    block
}

fn parse_hunks(block: &[String]) -> Result<Vec<Hunk>> {
    if block.is_empty() {
        return Err(anyhow!("Update block is empty"));
    }

    if block.iter().any(|line| line.starts_with("@@")) {
        return parse_unified_hunks(block);
    }

    let mut hunks = Vec::new();
    let mut start = 0;

    for (idx, line) in block.iter().enumerate() {
        if line.trim() == "---" {
            if start < idx {
                hunks.push(parse_hunk_lines(&block[start..idx])?);
            }
            start = idx + 1;
        }
    }

    if start < block.len() {
        hunks.push(parse_hunk_lines(&block[start..])?);
    }

    if hunks.is_empty() {
        return Err(anyhow!("No hunks found in update block"));
    }

    Ok(hunks)
}

fn parse_hunk_lines(lines: &[String]) -> Result<Hunk> {
    let mut context_before = Vec::new();
    let mut removals = Vec::new();
    let mut additions = Vec::new();
    let mut context_after = Vec::new();

    let mut in_changes = false;
    let mut finished_changes = false;

    for line in lines {
        if let Some(stripped) = line.strip_prefix('-') {
            if finished_changes {
                return Err(anyhow!("Change lines must be contiguous"));
            }
            in_changes = true;
            removals.push(stripped.to_string());
            continue;
        }

        if let Some(stripped) = line.strip_prefix('+') {
            if finished_changes {
                return Err(anyhow!("Change lines must be contiguous"));
            }
            in_changes = true;
            additions.push(stripped.to_string());
            continue;
        }

        if !in_changes {
            context_before.push(line.to_string());
        } else {
            finished_changes = true;
            context_after.push(line.to_string());
        }
    }

    if removals.is_empty() && additions.is_empty() {
        return Err(anyhow!("Hunk has no changes"));
    }

    Ok(Hunk {
        context_before,
        removals,
        additions,
        context_after,
    })
}

fn parse_unified_hunks(block: &[String]) -> Result<Vec<Hunk>> {
    let mut hunks = Vec::new();
    let mut current = Vec::new();
    let mut saw_header = false;

    for line in block {
        if line.starts_with("--- ") || line.starts_with("+++ ") {
            continue;
        }
        if line.starts_with("@@") {
            if !current.is_empty() {
                hunks.push(parse_unified_hunk_lines(&current)?);
                current.clear();
            }
            saw_header = true;
            continue;
        }
        if saw_header {
            current.push(line.clone());
        }
    }

    if !current.is_empty() {
        hunks.push(parse_unified_hunk_lines(&current)?);
    }

    if hunks.is_empty() {
        return Err(anyhow!("No hunks found in update block"));
    }

    Ok(hunks)
}

fn parse_unified_hunk_lines(lines: &[String]) -> Result<Hunk> {
    let mut lines = lines;
    while let Some((last, rest)) = lines.split_last()
        && last.is_empty()
    {
        lines = rest;
    }
    let normalized = lines
        .iter()
        .map(|line| {
            line.strip_prefix(' ')
                .map(ToOwned::to_owned)
                .unwrap_or_else(|| line.to_string())
        })
        .collect::<Vec<_>>();
    parse_hunk_lines(&normalized)
}

fn parse_added_content(lines: &[String]) -> String {
    if lines.iter().any(|line| line.starts_with("@@")) {
        return parse_unified_added_content(lines);
    }
    let mut content_lines = Vec::new();
    for line in lines {
        if let Some(stripped) = line.strip_prefix('+') {
            content_lines.push(stripped.to_string());
        } else {
            content_lines.push(line.to_string());
        }
    }
    content_lines.join("\n")
}

fn parse_unified_added_content(lines: &[String]) -> String {
    let mut content_lines = Vec::new();
    let mut saw_header = false;

    for line in lines {
        if line.starts_with("--- ") || line.starts_with("+++ ") {
            continue;
        }
        if line.starts_with("@@") {
            saw_header = true;
            continue;
        }
        if !saw_header {
            continue;
        }
        if let Some(stripped) = line.strip_prefix('+') {
            content_lines.push(stripped.to_string());
        } else if let Some(stripped) = line.strip_prefix(' ') {
            content_lines.push(stripped.to_string());
        }
    }

    content_lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::super::apply::apply_hunks;
    use super::*;

    #[test]
    fn parse_patch_update_add_delete() {
        let text = "*** Update File: foo.txt\ncontext\n-old\n+new\ncontext\n*** Add File: bar.txt\n+hello\n+world\n*** Delete File: baz.txt";
        let ops = parse_patch(text).unwrap();
        assert_eq!(ops.len(), 3);
        match &ops[0] {
            PatchOperation::Update { path, hunks } => {
                assert_eq!(path, "foo.txt");
                assert_eq!(hunks.len(), 1);
            }
            _ => panic!("expected update"),
        }
        match &ops[1] {
            PatchOperation::Add { path, content } => {
                assert_eq!(path, "bar.txt");
                assert_eq!(content, "hello\nworld");
            }
            _ => panic!("expected add"),
        }
        match &ops[2] {
            PatchOperation::Delete { path } => {
                assert_eq!(path, "baz.txt");
            }
            _ => panic!("expected delete"),
        }
    }

    #[test]
    fn parse_patch_accepts_unified_diff_hunks() {
        let text = "*** Update File: README.md\n--- \n+++ \n@@ -1,3 +1,3 @@\n # Runtime Panel Smoke\n \n-status=pending\n+status=active_panel_checked\n*** Add File: RESULT.md\nACTIVITY_PANEL_DONE";
        let ops = parse_patch(text).unwrap();
        assert_eq!(ops.len(), 2);
        match &ops[0] {
            PatchOperation::Update { path, hunks } => {
                assert_eq!(path, "README.md");
                assert_eq!(hunks.len(), 1);
                assert_eq!(hunks[0].context_before, ["# Runtime Panel Smoke", ""]);
                assert_eq!(hunks[0].removals, ["status=pending"]);
                assert_eq!(hunks[0].additions, ["status=active_panel_checked"]);
            }
            _ => panic!("expected update"),
        }
        match &ops[1] {
            PatchOperation::Add { path, content } => {
                assert_eq!(path, "RESULT.md");
                assert_eq!(content, "ACTIVITY_PANEL_DONE");
            }
            _ => panic!("expected add"),
        }
    }

    #[test]
    fn parse_patch_accepts_unified_update_and_add_file() {
        let text = "*** Update File: README.md\n--- a/README.md\n+++ b/README.md\n@@ -1,3 +1,3 @@\n # Unified Patch Smoke\n \n-status=pending\n+status=unified_patch_checked\n\n*** Add File: RESULT.md\n--- /dev/null\n+++ b/RESULT.md\n@@ -0,0 +1 @@\n+UNIFIED_PATCH_DONE\n";
        let ops = parse_patch(text).unwrap();
        assert_eq!(ops.len(), 2);
        match &ops[0] {
            PatchOperation::Update { hunks, .. } => {
                let updated = apply_hunks("# Unified Patch Smoke\n\nstatus=pending", hunks)
                    .expect("unified update should apply");
                assert_eq!(
                    updated,
                    "# Unified Patch Smoke\n\nstatus=unified_patch_checked"
                );
            }
            _ => panic!("expected update"),
        }
        match &ops[1] {
            PatchOperation::Add { content, .. } => {
                assert_eq!(content, "UNIFIED_PATCH_DONE");
            }
            _ => panic!("expected add"),
        }
    }
}
