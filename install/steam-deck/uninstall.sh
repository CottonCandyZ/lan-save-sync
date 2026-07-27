#!/usr/bin/env bash
set -euo pipefail

purge=false
if [[ "${1:-}" == "--purge" ]]; then
  purge=true
elif [[ $# -gt 0 ]]; then
  echo "Usage: $0 [--purge]" >&2
  exit 2
fi

systemctl --user disable --now lan-save-sync.service 2>/dev/null || true
rm -f -- "${HOME}/.config/systemd/user/lan-save-sync.service"
systemctl --user daemon-reload
rm -f -- "${HOME}/.local/bin/lan-save-sync"
rm -rf -- "${HOME}/homebrew/plugins/LanSaveSync"

if [[ "${purge}" == true ]]; then
  rm -rf -- "${HOME}/.config/lan-save-sync"
  echo "Program, Decky UI, configuration, and local history removed."
else
  echo "Program and Decky UI removed."
  echo "Configuration and local history kept at ${HOME}/.config/lan-save-sync"
  echo "Run '$0 --purge' to remove them too."
fi
