# Manual smoke checklist — Browser view (igv.js)

Date: 2026-05-11. Spec: `docs/superpowers/specs/2026-05-11-browser-serve-design.md`.
The automated test suite covers the HTTP routes and the controller
plumbing, but neither runs a real browser nor exercises the
TUI ↔ browser sync end-to-end. Run this checklist by hand before
shipping a release that touches the browser-serve code.

## Setup

```bash
cargo run --release -- \
    reference.fa \
    -b sample.bam \
    -g genes.gtf \
    -l loops.bedpe
```

(Substitute any small fixture set; `crates/igv-core/tests/data/` has
ready-made BAM/GFF/BEDPE samples.)

## Steps

1. **Press `B`.** Default browser opens a tab at
   `http://127.0.0.1:<port>/`. Footer briefly flashes
   `browser → http://127.0.0.1:<port>/`.
2. **Verify igv.js boots.** The page shows the igv.js track list:
   reference, BAM alignment, annotation, BEDPE interact track. The
   initial locus matches the TUI's current region.
3. **Pan the TUI with `d`.** The browser viewport advances by one
   window. Repeat with `s` (zoom out) and confirm the browser
   follows.
4. **Type `:BRCA1` (or any gene present in your GTF) in the TUI.** Both
   the TUI and the browser jump to BRCA1's span.
5. **Press `B` a second time.** No new server is started; the
   existing URL is opened in the browser again (focus may shift to
   the existing tab depending on OS browser behaviour).
6. **Open a second browser tab to the same URL.** Both tabs receive
   subsequent SSE pushes from the TUI.
7. **Quit the TUI with `q`.** The server shuts down cleanly. The
   browser tab(s) show network errors on the next navigation; the
   process exits with code 0.
8. **Re-launch with `--no-browser`.** Press `B`; no browser opens.
   Footer shows the URL anyway so users on SSH can copy it.
9. **Re-launch with `--serve-port 9001`.** Browser opens at
   `http://127.0.0.1:9001/` (or the next free port — if 9001 is
   busy, the launch fails with an error in the footer).

## Out-of-scope manual checks

- Remote access (deliberately disabled — `bind = 127.0.0.1` is hard-coded).
- Browser → TUI reverse sync (not implemented).
- Authentication tokens (not implemented).
