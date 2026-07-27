#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
binary_source="${script_dir}/lan-save-sync"
service_source="${script_dir}/lan-save-sync.service"
binary_target="${HOME}/.local/bin/lan-save-sync"
config_target="${HOME}/.config/lan-save-sync/agent.json"
service_target="${HOME}/.config/systemd/user/lan-save-sync.service"
plugin_source="${script_dir}/decky-plugin/LanSaveSync"
plugin_target="${HOME}/homebrew/plugins/LanSaveSync"

if [[ ! -f "${binary_source}" ]]; then
  echo "lan-save-sync must be placed next to install.sh" >&2
  exit 1
fi

install -Dm755 "${binary_source}" "${binary_target}"
install -Dm644 "${service_source}" "${service_target}"

if [[ ! -f "${config_target}" ]]; then
  "${binary_target}" init \
    --device-id steam-deck \
    --name "Steam Deck" \
    --output "${config_target}"
  echo "Created ${config_target}; add peers and folders before syncing."
fi

systemctl --user daemon-reload
systemctl --user enable lan-save-sync.service

if [[ -d "${plugin_source}" ]]; then
  mkdir -p "$(dirname -- "${plugin_target}")"
  rm -rf -- "${plugin_target}"
  cp -a -- "${plugin_source}" "${plugin_target}"
  echo "Decky UI installed at ${plugin_target}."
  echo "Reload LAN Save Sync from Decky settings or restart Gaming Mode."
else
  echo "Decky UI bundle not present; Agent-only installation completed."
fi

echo "After editing the configuration, start the Agent with:"
echo "  systemctl --user start lan-save-sync.service"
