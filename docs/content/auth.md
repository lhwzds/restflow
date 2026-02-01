# Authentication

RestFlow supports multiple authentication providers for AI services. This guide covers how to configure and manage authentication profiles.

## Providers

| Provider | Description | Token Format | Use Case |
|----------|-------------|--------------|----------|
| `claude-code` | Claude Code CLI OAuth | `sk-ant-oat01-...` | `restflow claude` command |
| `anthropic` | Anthropic API | `sk-ant-api03-...` | Direct API calls |
| `openai` | OpenAI API | `sk-...` | OpenAI models |
| `google` | Google Gemini API | API key | Gemini models |

## Commands

### List Profiles

```bash
restflow auth list
```

Example output:
```
+----------+-------------------+------------+-------------+---------+-----------+
| ID       | Name              | Provider   | Source      | Health  | Available |
+===============================================================================+
| 207a9150 | Claude Code OAuth | ClaudeCode | Manual      | Healthy | yes       |
| ff2717f1 | $OPENAI_API_KEY   | OpenAI     | Environment | Unknown | yes       |
+----------+-------------------+------------+-------------+---------+-----------+
```

### Add Profile

```bash
restflow auth add --provider <PROVIDER> --key <KEY> [--name <NAME>]
```

**Options:**

| Option | Description | Required |
|--------|-------------|----------|
| `--provider` | Provider type (see table above) | Yes |
| `--key` | API key or OAuth token | Yes |
| `--name` | Display name for the profile | No |

### Show Profile

```bash
restflow auth show <ID>
```

### Remove Profile

```bash
restflow auth remove <ID>
```

### Check Status

```bash
restflow auth status
```

### Discover Credentials

Automatically discover credentials from environment variables and other sources:

```bash
restflow auth discover
```

## Setting Up Claude Code

### Step 1: Get OAuth Token

Run the Claude Code setup command to generate an OAuth token:

```bash
claude setup-token
```

This will:
1. Open your browser for authentication
2. Generate a long-lived OAuth token (valid for 1 year)
3. Display the token: `sk-ant-oat01-...`

!!! warning "Save Your Token"
    Store this token securely. You won't be able to see it again after closing the terminal.

### Step 2: Add to RestFlow

```bash
restflow auth add \
  --provider claude-code \
  --key "sk-ant-oat01-YOUR_TOKEN_HERE" \
  --name "My Claude Code"
```

### Step 3: Verify

```bash
# Check profile is available
restflow auth list

# Test with claude command
restflow claude -p "Say hello"
```

## Setting Up Anthropic API

For direct API access (not Claude Code CLI):

```bash
restflow auth add \
  --provider anthropic \
  --key "sk-ant-api03-YOUR_API_KEY" \
  --name "My Anthropic API"
```

## Setting Up OpenAI

```bash
restflow auth add \
  --provider openai \
  --key "sk-YOUR_OPENAI_KEY" \
  --name "My OpenAI"
```

Or set via environment variable (auto-discovered):

```bash
export OPENAI_API_KEY="sk-YOUR_OPENAI_KEY"
restflow auth discover
```

## Environment Variables

RestFlow automatically discovers credentials from these environment variables:

| Variable | Provider |
|----------|----------|
| `OPENAI_API_KEY` | OpenAI |
| `ANTHROPIC_API_KEY` | Anthropic |
| `GOOGLE_API_KEY` | Google |

## Profile Health

Profiles track health status for automatic failover:

| Status | Description |
|--------|-------------|
| `Healthy` | Working correctly |
| `Unknown` | Not yet tested |
| `Cooldown` | Temporarily unavailable (rate limited) |
| `Disabled` | Permanently disabled |

## Troubleshooting

### "No available ClaudeCode auth profile found"

You need to add a Claude Code profile:

```bash
# First, get a token
claude setup-token

# Then add it to RestFlow
restflow auth add --provider claude-code --key "sk-ant-oat01-..." --name "Claude Code"
```

### "Invalid API key"

Make sure you're using the correct token type:

- **Claude Code CLI**: Use `sk-ant-oat01-...` (OAuth token) with `--provider claude-code`
- **Anthropic API**: Use `sk-ant-api03-...` (API key) with `--provider anthropic`

### Profile Not Available

Check profile status:

```bash
restflow auth show <ID>
```

If in cooldown, wait for the cooldown period to expire, or manually enable:

```bash
# The profile may need to be re-added if disabled
restflow auth remove <ID>
restflow auth add --provider ... --key ...
```
