---
name: Address PR Review Feedback
description: Triage and resolve unresolved PR review threads with minimal, test-verified fixes.
tags:
  - pr
  - review
  - automation
  - github
suggested_tools:
  - bash
  - file
---

# Address PR Review Feedback

Use this skill when you need to process unresolved GitHub PR review comments end-to-end.

## Input

- PR number or PR URL.
- If missing, resolve the PR from the current branch with `gh pr view`.

## Workflow

1. Gather unresolved review threads.
- Query review threads with GraphQL and collect: `thread_id`, `path`, `line`, and comment text.
- Skip resolved threads.

2. Triage each thread.
- `FIX`: Clear and actionable feedback. Implement a minimal targeted change.
- `SKIP`: Subjective preference, duplicate, or already addressed. Reply with a short reason.
- `ASK`: Ambiguous request or missing context. Ask a precise clarification question.

3. Implement fixes for `FIX` items first.
- Read the target file and surrounding context before editing.
- Keep scope narrow: implement only what is required by the comment.
- Do not bundle unrelated refactors.

4. Verify before replying.
- If Rust changed, run `cargo clippy -- -D warnings`.
- If web TypeScript/Vue changed, run `cd web && npm run format` and targeted tests.
- If no code changed, verification can be skipped with a stated reason.

5. Reply and resolve.
- For fixed items: reply with `Fixed: [what changed]`, then resolve the thread.
- For skipped items: reply with `Skipped: [reason]`, keep unresolved if reviewer decision is needed.
- For asked items: post concise clarifying question and keep unresolved.

6. Publish final summary.
- Total unresolved threads.
- `Fixed: N`, include touched files.
- `Skipped: N`, include reasons.
- `Asked: N`, include open questions.

## Command Snippets

```bash
# Resolve PR number from current branch
PR_NUMBER=$(gh pr view --json number -q '.number')

# List unresolved review threads
gh api graphql -f query='
query($owner: String!, $repo: String!, $pr: Int!) {
  repository(owner: $owner, name: $repo) {
    pullRequest(number: $pr) {
      reviewThreads(first: 100) {
        nodes {
          id
          isResolved
          comments(first: 10) {
            nodes {
              body
              path
              line
              startLine
            }
          }
        }
      }
    }
  }
}' -f owner='{owner}' -f repo='{repo}' -F pr="$PR_NUMBER"
```

```bash
# Reply to a review thread
gh api graphql -f query='
mutation($threadId: ID!, $body: String!) {
  addPullRequestReviewThreadReply(input: {
    pullRequestReviewThreadId: $threadId,
    body: $body
  }) {
    comment { id }
  }
}' -f threadId="$THREAD_ID" -f body="$REPLY"

# Resolve a review thread
gh api graphql -f query='
mutation($threadId: ID!) {
  resolveReviewThread(input: { threadId: $threadId }) {
    thread { isResolved }
  }
}' -f threadId="$THREAD_ID"
```

## Rules

- Treat bot and human comments with equal priority.
- Never resolve a thread that is not actually addressed.
- Keep replies concise and evidence-based.
- Prefer minimal diff with clear verification evidence.
