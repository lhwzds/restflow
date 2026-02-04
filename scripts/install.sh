#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

usage() {
  cat <<'EOF'
Usage: install.sh [--systemd|--launchd]

Installs RestFlow daemon service files for the selected platform.
EOF
}

install_systemd() {
  sudo mkdir -p /etc/systemd/system
  sudo cp "${SCRIPT_DIR}/restflow.service" /etc/systemd/system/restflow.service
  sudo systemctl daemon-reload
  sudo systemctl enable restflow
  sudo systemctl start restflow
  echo "RestFlow systemd service installed and started."
}

install_launchd() {
  local target="${HOME}/Library/LaunchAgents/com.restflow.daemon.plist"
  mkdir -p "${HOME}/Library/LaunchAgents"
  cp "${SCRIPT_DIR}/com.restflow.daemon.plist" "${target}"
  launchctl unload "${target}" >/dev/null 2>&1 || true
  launchctl load "${target}"
  echo "RestFlow launchd service installed and loaded."
}

if [[ $# -eq 0 ]]; then
  usage
  exit 1
fi

case "$1" in
  --systemd)
    install_systemd
    ;;
  --launchd)
    install_launchd
    ;;
  -h|--help)
    usage
    ;;
  *)
    echo "Unknown option: $1" >&2
    usage
    exit 1
    ;;
esac
