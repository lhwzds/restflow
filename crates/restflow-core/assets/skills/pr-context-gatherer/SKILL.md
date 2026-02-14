---
name: PR Context Gatherer
description: Pre-fetch pull request context into a structured workspace for downstream review agents.
tags:
  - default
  - pr
  - review
  - context
suggested_tools:
  - bash
  - file
---

# PR Context Gatherer

Use this skill to collect pull request context once and reuse it across review runs.

## Inputs

- Pull request number, URL, or branch name (optional).
- If no input is provided, detect the PR from the current branch.

## Output

Create `.restflow/pr-context/{pr_number}/` with the following files:

1. `pr-meta.json`
- Command:
```bash
gh pr view {PR} --json number,title,body,author,baseRefName,headRefName,state,labels,reviewDecision,mergeable,additions,deletions,changedFiles
```

2. `comments.json`
- Command:
```bash
gh api repos/{owner}/{repo}/pulls/{PR}/comments --paginate
```

3. `review-threads.json`
- Command:
```bash
gh api graphql -f query='\
query($owner: String!, $repo: String!, $pr: Int!) {\
  repository(owner: $owner, name: $repo) {\
    pullRequest(number: $pr) {\
      reviewThreads(first: 100) {\
        nodes {\
          id\
          isResolved\
          isOutdated\
          path\
          line\
          startLine\
          comments(first: 20) {\
            nodes {\
              author { login }\
              body\
              createdAt\
            }\
          }\
        }\
      }\
    }\
  }\
}'
```

4. `changed-files.txt`
- Command:
```bash
gh pr diff {PR} --name-only
```

5. `diffs/{sanitized_filename}.diff`
- For each changed file, collect a focused diff:
```bash
gh pr diff {PR} -- {file} > diffs/{sanitized_filename}.diff
```
- Optionally include function-level context for code files:
```bash
git diff -W origin/{base_branch}...HEAD -- {file}
```

6. `related-issues.json`
- Parse PR body references like `#123`, `fixes #456`, `closes #789`.
- Fetch each related issue with:
```bash
gh issue view {ISSUE} --json number,title,state,author,labels,body
```

7. `context/`
- Copy project conventions when available:
  - `CLAUDE.md`
  - `AGENTS.md`
  - nearest directory-level `AGENTS.md` for changed files

8. `CONTEXT_SUMMARY.md`
- Include:
  - PR number and title
  - changed file count and list
  - resolved/total review-thread counts
  - inventory of generated context files

## Implementation Procedure

1. Resolve PR identity.
- If input contains a URL, extract number.
- If input contains a plain integer, use it directly.
- Otherwise run:
```bash
gh pr view --json number
```

2. Create target directory.
```bash
mkdir -p .restflow/pr-context/{PR}/diffs .restflow/pr-context/{PR}/context
```

3. Gather metadata, comments, review threads, and changed files.
- Save each command output to the corresponding JSON or text file.

4. Generate per-file diffs.
- Iterate through `changed-files.txt`.
- Sanitize file path separators when writing diff filenames.

5. Collect related issues.
- Extract issue numbers from PR body with regex.
- Deduplicate IDs before fetching.

6. Copy context docs.
- Copy only files that exist.
- Do not fail the workflow if optional docs are missing.

7. Write `CONTEXT_SUMMARY.md`.
- Record what was fetched and what was unavailable.

## Hook Integration

Use this hook pattern to auto-run context gathering for review tasks.

```json
{
  "operation": "create",
  "name": "pr-context-pre-gather",
  "event": "task_started",
  "filter": "review*",
  "action": "run_task",
  "agent": "pr-context-gatherer-agent",
  "input": "Gather context for PR {{task_name}}"
}
```

CLI equivalent:

```bash
restflow hook create \
  --name pr-context-pre-gather \
  --event task_started \
  --action run_task \
  --agent pr-context-gatherer-agent \
  --input "Gather context for PR {{task_name}}"
```

## Consumer Guidance

Review agents should:

1. Check for `.restflow/pr-context/{PR}/CONTEXT_SUMMARY.md` first.
2. Load all context files in one batch using the file tool.
3. Fall back to direct `gh` calls only if required files are missing.

## Success Criteria

- Context directory exists with expected files.
- A downstream review task can complete initial analysis without repeated metadata fetch calls.
- Missing optional files are reported in summary but do not fail the run.
