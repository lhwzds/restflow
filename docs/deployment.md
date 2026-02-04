# Deployment

This guide covers common ways to run RestFlow in the background or as a desktop app.

## Quick Start (Local)

```bash
restflow start
```

Options:

- `--no-browser` to skip opening the UI.
- `--port <PORT>` to override the default HTTP port (3000).
- `--http/--no-http` to enable or disable the HTTP API.

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

## Health Check

If the HTTP API is enabled, the health endpoint is:

```text
http://localhost:3000/health
```
