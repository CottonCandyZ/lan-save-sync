#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
binary="${script_dir}/lan-save-sync"
config="${script_dir}/agent.json"

if [[ ! -x "${binary}" ]]; then
  echo "lan-save-sync must be executable and placed next to this script" >&2
  exit 1
fi
if [[ ! -f "${config}" ]]; then
  "${binary}" init \
    --device-id portable-deck \
    --name "Portable Steam Deck" \
    --output "${config}"
  echo "Created ${config}. Add peers and folders, then run this script again."
  exit 0
fi
exec "${binary}" --config "${config}" serve
