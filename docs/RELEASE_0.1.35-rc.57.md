# Agbrio 0.1.35-rc.57 — Read files inside conversations

Source: main. Includes the Boards, native conversation trees, decisions, chronology/mobile recovery and application-control capabilities from rc53, plus rc54 update discovery fallback when GitHub anonymous API quota is exhausted.

## File preview

Click local Markdown file links (including inline code paths) in supported watched/Bridge messages to open a read-only viewer. Desktop uses a wide reading dialog; mobile uses a full-screen reader with Back/close. The original conversation stays mounted. Closing a preview also removes its transient Back entry; nested readers return to their parent document.

- Markdown: headings, tables, code blocks, internal anchors and exact-source relative document/image links.
- Raster image links: view, zoom and fit to screen. Existing inline-image viewer retained.
- UTF-8 text and source files: numbered lines and supported path line/column navigation.
- JSON: formatted when valid, otherwise original text.
- CSV: quoted/multiline cells and read-only table; malformed or oversized grids show original text.
- Unsupported documents, including PDF and Excel: explicit download. No office renderer or online processing service is added.

Only files referenced by the exact message/document chain are readable. Network shares remain blocked. Existing 8MiB transfer cap remains; text rendering is capped at 1MiB and 20,000 lines, with download fallback. Remote HTTPS document links retain normal browser behavior. File contents, code, raw HTML and formulas never execute.

## Reader selection correction

A delayed passive load from an older committed reader can no longer override a newer explicit conversation selection. A deterministic scheduling regression reproduces the original failure and passes with the selection-intent guard. No provider binding or session authority changes.

## Updating

Settings → About → Check for updates. Download/install remain explicit and signature checked. Older clients may need to retry after GitHub API quota resets, or download this signed installer once to acquire the discovery fallback. Preserve existing data, pairing and native sessions; no uncertain sends are replayed.

## Validation boundary

Automated frontend/native/browser and exact signed Windows installer results are attached to the release. Physical-phone UAT remains separate. Native provider sessions and decisions were not modified for validation.
