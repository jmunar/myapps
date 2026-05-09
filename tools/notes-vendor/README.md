# notes-vendor

Builds `static/notes-vendor.bundle.js` — a single IIFE bundle containing Yjs + Tiptap (and the extensions the Notes editor uses), exposed on `window.NotesYjsTiptap`.

## Rebuild

```
./build.sh
```

Runs `npm install` if `node_modules/` is missing and re-bundles. Commit the resulting `static/notes-vendor.bundle.js`.

## When to rebuild

- Bumping any version in `package.json`
- Adding a new dependency the bootstrap (`static/notes-tiptap-bootstrap.js`) needs

## Bundle surface

After loading the bundle, `window.NotesYjsTiptap` exposes:

- `Y` — Yjs core (`Doc`, `encodeStateVector`, `applyUpdate`, …)
- `IndexeddbPersistence` — local-first persistence
- `syncProtocol` — y-protocols/sync (`writeSyncStep1`, `readSyncMessage`, …)
- `encoding`, `decoding` — lib0 binary encoders/decoders
- `Editor`, `StarterKit`, `TaskList`, `TaskItem`, `Link`, `Collaboration`, `Markdown` — Tiptap pieces

The actual editor wiring lives in `static/notes-tiptap-bootstrap.js`.
