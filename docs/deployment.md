# Deployment

This guide covers common ways to run RestFlow in the background or as a desktop app.

## Quick Start (Local)

```bash
restflow start
```

Options:

- `--no-browser` to skip opening the UI.
- `--mcp-port <PORT>` to override the default MCP HTTP port (8787).

## Daemon Management

```bash
restflow daemon start
restflow daemon stop
restflow daemon status
```

For foreground mode:

```bash
restflow daemon start --foreground
```

## systemd (Linux)

```bash
sudo cp scripts/restflow.service /etc/systemd/system/restflow.service
sudo systemctl daemon-reload
sudo systemctl enable restflow
sudo systemctl start restflow
```

Logs:

```bash
journalctl -u restflow -f
```

## launchd (macOS)

```bash
mkdir -p ~/Library/LaunchAgents
cp scripts/com.restflow.daemon.plist ~/Library/LaunchAgents/com.restflow.daemon.plist
launchctl unload ~/Library/LaunchAgents/com.restflow.daemon.plist || true
launchctl load ~/Library/LaunchAgents/com.restflow.daemon.plist
```

Logs:

```bash
tail -f /usr/local/var/log/restflow/daemon.log
```

## Installer Script

```bash
scripts/install.sh --systemd
scripts/install.sh --launchd
```

## MCP HTTP Server

The daemon exposes an MCP (Model Context Protocol) HTTP server on port 8787 by default. This provides full API access via JSON-RPC over Streamable HTTP.

```bash
# Test with curl
curl -X POST http://localhost:8787/mcp \
  -H "Content-Type: application/json" \
  -H "Accept: application/json, text/event-stream" \
  -d '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-03-26","capabilities":{},"clientInfo":{"name":"test","version":"0.1.0"}}}'
```
