# Desktop interface

The first MVP intentionally uses the shared Agent CLI as the Windows interface:

```powershell
lan-save-sync.exe --config agent.json plan --peer steam-deck --folder eden
lan-save-sync.exe --config agent.json sync --peer steam-deck --folder eden --action auto
```

A native desktop/tray UI will be added here after the Agent protocol has been
validated on a real Steam Deck. It must call the Agent's authenticated local API
instead of duplicating scanning, version, or transfer logic.

The Windows install and portable scripts live in `install/windows`.
