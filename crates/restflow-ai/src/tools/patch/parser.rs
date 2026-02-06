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
        if line.starts_with('-') {
            if finished_changes {
                return Err(anyhow!("Change lines must be contiguous"));
            }
            in_changes = true;
            removals.push(line[1..].to_string());
            continue;
        }

        if line.starts_with('+') {
            if finished_changes {
                return Err(anyhow!("Change lines must be contiguous"));
            }
            in_changes = true;
            additions.push(line[1..].to_string());
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

fn parse_added_content(lines: &[String]) -> String {
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

#[cfg(test)]
mod tests {
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
}
