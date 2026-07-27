# Security

## Supported status

LAN Save Sync is currently an alpha/MVP. The peer API authenticates requests
with a bearer token, but transport encryption is not implemented yet.

Use it only on a trusted private LAN. Do not expose port 48123 to the internet
and do not use it on public or otherwise untrusted networks.

## Secrets

Never commit a real `agent.json`. It contains the local API token and the
tokens used to authenticate to peers. The repository `.gitignore` excludes the
standard local configuration and data paths.

## File safety

The Agent rejects:

- archives with absolute paths, parent traversal, links, or duplicate paths;
- uploads whose content hash does not match the advertised version;
- writes when the destination changed after the sync plan;
- automatic synchronization when both peers changed after the common baseline.

Incoming replacement creates a local history version before swapping the
directory. This is a recovery feature, not a substitute for an independent
backup.
