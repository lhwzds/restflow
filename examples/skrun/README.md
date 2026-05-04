# RestFlow External Tool Examples for skrun

RestFlow core keeps only the agent runtime, file/edit/search/bash tools,
`load_skill`, and `run_skill`. Tools that reach outside the workspace should be
installed and executed through `skrun`.

Each example directory contains:

- `skill.json`: illustrative metadata for an executable skill.
- `run.py`: a JSON-in/JSON-out reference implementation.

The expected invocation shape is:

```bash
python run.py '{"key":"value"}'
```

`run_skill` calls the `skrun` Rust crate embedded in RestFlow. A concrete
standalone `skrun` CLI may use the same artifact format for local installation,
but RestFlow runtime execution does not require a separate `skrun` executable.

## Example Skills

| Skill | Purpose |
| --- | --- |
| `python-exec` | Run short Python snippets outside RestFlow core. |
| `http-request` | Make arbitrary HTTP requests. |
| `web-fetch` | Fetch a web page and return text. |
| `web-search` | Search through a provider API such as Brave Search. |
| `email-send` | Send email with SMTP credentials. |
| `telegram-send` | Send Telegram bot messages. |
| `discord-send` | Send Discord webhook messages. |
| `slack-send` | Send Slack webhook messages. |
| `browser-automation` | Run a minimal Playwright browser action. |
| `transcribe` | Call an external transcription command. |
| `vision` | Call an external image-analysis command. |
| `memory-file` | Store and search local JSONL notes outside core storage. |

All examples fail with structured JSON errors when required credentials,
commands, or Python packages are missing.
