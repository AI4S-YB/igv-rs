# igv-rs GitHub Pages Documentation Site — Design Spec

**Date:** 2026-05-12  
**Status:** Approved

---

## Overview

Build a single-page documentation website for igv-rs, hosted on GitHub Pages via the `docs/` folder on the `main` branch. The site is bilingual (English + Chinese), uses hand-crafted HTML/CSS with no build tooling, and follows a GitHub-inspired visual style.

---

## Approach

**Hand-crafted HTML/CSS** — no static site generator, no build step.

- `docs/index.html` — English version
- `docs/index.zh.html` — Chinese version
- `docs/style.css` — shared stylesheet
- Language toggle in the nav links between the two files

Rationale: Single-page requirement + GitHub aesthetic + zero deploy complexity. `docs/` on `main` → Settings → Pages → done.

---

## Visual Design

### Color Palette (GitHub-inspired)

| Token         | Value     | Usage                          |
|---------------|-----------|--------------------------------|
| Background    | `#ffffff` | Page background                |
| Surface       | `#f6f8fa` | Nav, hero, code blocks, cards  |
| Border        | `#d0d7de` | All dividers and borders       |
| Text primary  | `#1f2328` | Headings, body copy            |
| Text muted    | `#636c76` | Subtitles, labels, meta        |
| Accent        | `#0969da` | Links, section labels, buttons |
| Code green    | `#57ab5a` | Shell prompt `$`, comments     |
| Code blue     | `#0969da` | CLI flags (`-b`, `-v`, etc.)   |

### Typography

- Body: `-apple-system, BlinkMacSystemFont, "Segoe UI", Helvetica, Arial, sans-serif`
- Code: `"SFMono-Regular", Consolas, "Liberation Mono", monospace`
- Base size: 14px, line-height 1.6

### Navigation

Fixed top bar (56px), white background, 1px `#d0d7de` bottom border:
- Left: GitHub Octocat SVG icon + "igv-rs" wordmark
- Center: anchor links — Features · Install · Usage · Keybindings · Config · Snapshot · Browser · GitHub
- Right: `EN / 中文` toggle (bordered pill) + `GitHub` button (dark filled)

---

## Page Structure

### Hero Section

Background `#f6f8fa`, centered, full-width:
- Version badge (e.g. `v0.7.0 · Rust`) in a blue pill
- `<h1>igv-rs</h1>`
- One-line subtitle listing supported formats
- Inline install box: `$ cargo install igv-rs` (bordered, white bg, monospace)
- Two CTA buttons: **Get Started** (blue filled) + **GitHub ↗** (outlined)

### Content Sections

Max-width 860px, centered, `2.5rem 2rem` padding. Each section:
- Small uppercase section label in `#0969da` (e.g. "Overview", "CLI")
- `<h2>` heading with `1px #d0d7de` bottom border
- Body copy + code blocks and/or tables as needed

**Section order:**

1. **Features** — 2×3 card grid, each card: icon + title + description
2. **Install** — cargo install, pre-built binaries link, build from source
3. **Usage / CLI** — `igv-rs` command examples with `-b`, `-v`, `-g`, `-s`, `-l` flags syntax-highlighted
4. **Wide-zoom Behavior** — responsive table (view width vs data loaded)
5. **Snapshot Export** — interactive (`S` key) + headless batch (`--snapshot-bed`, `--snapshot-genes`) with code blocks
6. **Browser View** — press `B` in TUI, local server details, code example
7. **Keybindings** — two-column table (`Key` | `Action`), alternating row shading
8. **Configuration** — `config.toml` code block with `[theme]`, `[serve]` tables
9. **Known Limitations** — bulleted list, muted text

### Footer

`#f6f8fa` background, `1px #d0d7de` top border, centered:
- `igv-rs · GitHub · crates.io · MIT License`

---

## Bilingual Strategy

Two static HTML files share one `style.css`. The nav `EN / 中文` link points to the alternate file (`index.html` ↔ `index.zh.html`). No JavaScript language switching — simple anchor tag.

Content in `index.zh.html` is a full Chinese translation of all section text. Code blocks remain in English (command-line content is language-agnostic).

---

## Deployment

1. Create `docs/` at repo root with `index.html`, `index.zh.html`, `style.css`
2. On GitHub: **Settings → Pages → Source: Deploy from branch → Branch: `main` / folder: `docs/`**
3. No CI required — static files are served directly
4. Custom domain optional (configure via `docs/CNAME`)

---

## File Structure

```
docs/
├── index.html        # English single-page docs
├── index.zh.html     # Chinese single-page docs
└── style.css         # Shared GitHub-style stylesheet
```

---

## Out of Scope

- Search functionality
- Versioned docs
- Auto-generated API reference from Rust doc comments
- Dark mode toggle (use system default)
