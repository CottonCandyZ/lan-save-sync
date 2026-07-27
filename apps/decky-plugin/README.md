# LAN Save Sync Decky interface

This directory contains only the Decky UI and its Python bridge. The shared
`lan-save-sync` Agent must be installed and running separately.

Build with pnpm 9:

```bash
pnpm install
pnpm build
```

The distributable plugin requires `dist/index.js`, `main.py`, `plugin.json`,
`package.json`, `README.md`, and the repository license.
