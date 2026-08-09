# Screenshots

Media assets used by the repo's top-level [`README.md`](../../README.md).

| File | Used for | Notes |
|------|----------|-------|
| `tailr-demo.gif` | Hero demo animation (README top) | ~6 MB, keep < 8 MB so it loads smoothly on mobile |

## Adding new media

1. Drop the file into this directory (`docs/screenshots/`).
2. Use a **lowercase** filename (e.g. `multi-file-tabs.png`) — case-sensitive
   on Linux/CI even though macOS is not.
3. Reference it from README with a root-relative path:
   ```markdown
   ![description](./docs/screenshots/multi-file-tabs.png)
   ```
4. Prefer lossy formats: **GIF/MP4** for animations, **WebP/PNG** for stills.
   Aim for < 8 MB per file (compress larger clips before committing).

## Regenerating `tailr-demo.gif`

The demo is a screen recording, ~15-20s, showing:

1. `tailr -l ./logs` in the terminal
2. Browser opens `localhost:7700`, multi-file tabs visible
3. Live log streaming
4. Multi-keyword filter narrows the view
5. Switch a log-level preset (e.g. General → Java)

Re-record with [Kap](https://getkap.co/) (macOS) or `ffmpeg`, then compress at
ezgif.com/optimize (drop frame rate to ~10fps, width ~900px) to stay under 8 MB.
