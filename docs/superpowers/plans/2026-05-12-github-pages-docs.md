# igv-rs GitHub Pages Documentation Site — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a bilingual (EN/ZH), single-page GitHub-Pages-ready documentation site for igv-rs in `docs/` with no build tooling.

**Architecture:** Two static HTML files (`index.html` EN, `index.zh.html` ZH) share a single `style.css`. A `.nojekyll` marker tells GitHub Pages to serve files as-is. The nav's language toggle is a plain `<a href>` between the two files. No JavaScript, no framework, no CI pipeline.

**Tech Stack:** Plain HTML5, CSS3, GitHub Pages (deploy from `docs/` on `main` branch).

---

## Task 1: Create shared stylesheet `docs/style.css`

**Files:**
- Create: `docs/style.css`

- [ ] **Step 1: Create `docs/` directory and write `style.css`**

```css
/* docs/style.css */
*, *::before, *::after { box-sizing: border-box; margin: 0; padding: 0; }

body {
  font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Helvetica, Arial, sans-serif;
  font-size: 14px;
  line-height: 1.6;
  color: #1f2328;
  background: #ffffff;
}

a { color: #0969da; text-decoration: none; }
a:hover { text-decoration: underline; }

code {
  font-family: "SFMono-Regular", Consolas, "Liberation Mono", monospace;
  font-size: 0.85em;
  background: #eaeef2;
  border-radius: 4px;
  padding: 1px 6px;
}

/* ── Navigation ────────────────────────────────────────── */
.gh-nav {
  position: sticky;
  top: 0;
  z-index: 100;
  display: flex;
  align-items: center;
  gap: 1.25rem;
  height: 56px;
  padding: 0 2rem;
  background: #ffffff;
  border-bottom: 1px solid #d0d7de;
}

.gh-nav .logo {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  font-weight: 700;
  font-size: 1rem;
  color: #1f2328;
  text-decoration: none;
}

.gh-nav .logo svg { width: 20px; height: 20px; flex-shrink: 0; }

.gh-nav .nav-links { display: flex; gap: 1rem; }
.gh-nav .nav-links a { font-size: 0.85rem; color: #636c76; }
.gh-nav .nav-links a:hover { color: #1f2328; text-decoration: none; }

.gh-nav .spacer { flex: 1; }

.gh-nav .lang-toggle {
  font-size: 0.8rem;
  color: #636c76;
  border: 1px solid #d0d7de;
  border-radius: 2em;
  padding: 3px 12px;
}
.gh-nav .lang-toggle:hover { color: #1f2328; text-decoration: none; background: #f6f8fa; }

.gh-nav .gh-btn {
  font-size: 0.8rem;
  font-weight: 500;
  color: #ffffff;
  background: #1f2328;
  border-radius: 6px;
  padding: 4px 14px;
}
.gh-nav .gh-btn:hover { background: #32383f; text-decoration: none; }

/* ── Hero ───────────────────────────────────────────────── */
.hero {
  background: #f6f8fa;
  border-bottom: 1px solid #d0d7de;
  padding: 4.5rem 2rem;
  text-align: center;
}

.hero .badge {
  display: inline-block;
  background: #ddf4ff;
  color: #0969da;
  border: 1px solid #b6e3ff;
  border-radius: 2em;
  padding: 2px 14px;
  font-size: 0.75rem;
  font-weight: 500;
  margin-bottom: 1rem;
}

.hero h1 {
  font-size: 2.4rem;
  font-weight: 700;
  letter-spacing: -0.5px;
  color: #1f2328;
  margin-bottom: 0.75rem;
}

.hero .tagline {
  color: #636c76;
  font-size: 1rem;
  max-width: 560px;
  margin: 0 auto 1.75rem;
}

.install-box {
  display: inline-flex;
  align-items: center;
  gap: 0.6rem;
  background: #ffffff;
  border: 1px solid #d0d7de;
  border-radius: 8px;
  padding: 0.55rem 1.1rem;
  font-family: "SFMono-Regular", Consolas, monospace;
  font-size: 0.875rem;
  color: #1f2328;
}

.install-box .prompt { color: #57ab5a; user-select: none; }

.hero-btns {
  display: flex;
  gap: 0.75rem;
  justify-content: center;
  margin-top: 1.25rem;
}

.btn-primary {
  background: #0969da;
  color: #ffffff;
  border-radius: 6px;
  padding: 7px 22px;
  font-size: 0.875rem;
  font-weight: 500;
}
.btn-primary:hover { background: #0860ca; text-decoration: none; }

.btn-secondary {
  background: #ffffff;
  color: #1f2328;
  border: 1px solid #d0d7de;
  border-radius: 6px;
  padding: 7px 22px;
  font-size: 0.875rem;
  font-weight: 500;
}
.btn-secondary:hover { background: #f6f8fa; text-decoration: none; }

/* ── Content ────────────────────────────────────────────── */
.content {
  max-width: 880px;
  margin: 0 auto;
  padding: 2.5rem 2rem;
}

.section {
  margin-bottom: 3rem;
  padding-bottom: 2.5rem;
  border-bottom: 1px solid #d0d7de;
}
.section:last-child { border-bottom: none; }

.section-label {
  font-size: 0.7rem;
  text-transform: uppercase;
  letter-spacing: 0.1em;
  color: #0969da;
  font-weight: 600;
  margin-bottom: 0.4rem;
}

.section h2 {
  font-size: 1.4rem;
  font-weight: 700;
  color: #1f2328;
  padding-bottom: 0.5rem;
  border-bottom: 1px solid #d0d7de;
  margin-bottom: 1rem;
}

.section p { color: #636c76; margin-bottom: 0.75rem; }
.section p:last-child { margin-bottom: 0; }

/* ── Code blocks ────────────────────────────────────────── */
.code-block {
  background: #f6f8fa;
  border: 1px solid #d0d7de;
  border-radius: 6px;
  padding: 1rem 1.25rem;
  font-family: "SFMono-Regular", Consolas, monospace;
  font-size: 0.8rem;
  color: #1f2328;
  overflow-x: auto;
  margin: 0.75rem 0;
  line-height: 1.8;
}

.code-block .cmd { display: block; }
.code-block .comment { color: #57ab5a; }
.code-block .flag { color: #0969da; }
.code-block .prompt { color: #57ab5a; user-select: none; }

/* ── Feature grid ───────────────────────────────────────── */
.features {
  display: grid;
  grid-template-columns: repeat(3, 1fr);
  gap: 1rem;
  margin: 1rem 0;
}

.feature-card {
  background: #f6f8fa;
  border: 1px solid #d0d7de;
  border-radius: 8px;
  padding: 1rem 1.1rem;
}

.feature-card .icon { font-size: 1.3rem; margin-bottom: 0.4rem; }
.feature-card h4 { font-size: 0.875rem; font-weight: 600; color: #1f2328; margin-bottom: 0.25rem; }
.feature-card p { font-size: 0.8rem; color: #636c76; margin: 0; }

/* ── Tables ─────────────────────────────────────────────── */
.data-table {
  width: 100%;
  border-collapse: collapse;
  font-size: 0.85rem;
  margin: 0.75rem 0;
  overflow-x: auto;
  display: block;
}

.data-table th {
  background: #f6f8fa;
  color: #636c76;
  font-weight: 600;
  text-align: left;
  padding: 7px 12px;
  border: 1px solid #d0d7de;
  font-size: 0.75rem;
  text-transform: uppercase;
  letter-spacing: 0.05em;
  white-space: nowrap;
}

.data-table td {
  padding: 7px 12px;
  border: 1px solid #d0d7de;
  color: #1f2328;
  white-space: nowrap;
}

.data-table tr:nth-child(even) td { background: #f6f8fa; }
.data-table .yes { color: #1a7f37; font-weight: 600; }
.data-table .no  { color: #636c76; }

/* ── Limitations list ───────────────────────────────────── */
.limits-list {
  list-style: none;
  margin: 0.75rem 0;
}

.limits-list li {
  padding: 0.6rem 0;
  border-bottom: 1px solid #f0f0f0;
  color: #636c76;
  font-size: 0.875rem;
  padding-left: 1.25rem;
  position: relative;
}

.limits-list li::before {
  content: "·";
  position: absolute;
  left: 0;
  color: #d0d7de;
  font-weight: 700;
}

.limits-list li strong { color: #1f2328; }
.limits-list li:last-child { border-bottom: none; }

/* ── Footer ─────────────────────────────────────────────── */
.gh-footer {
  background: #f6f8fa;
  border-top: 1px solid #d0d7de;
  padding: 1.5rem 2rem;
  text-align: center;
  color: #636c76;
  font-size: 0.8rem;
}

.gh-footer a { color: #636c76; }
.gh-footer a:hover { color: #0969da; }

/* ── Responsive ─────────────────────────────────────────── */
@media (max-width: 700px) {
  .gh-nav .nav-links { display: none; }
  .features { grid-template-columns: 1fr 1fr; }
  .hero h1 { font-size: 1.8rem; }
}

@media (max-width: 480px) {
  .features { grid-template-columns: 1fr; }
}
```

- [ ] **Step 2: Verify file exists**

```bash
ls -lh docs/style.css
```

Expected: file present, ~5–7 KB.

- [ ] **Step 3: Commit**

```bash
git add docs/style.css
git commit -m "docs: add shared GitHub-style stylesheet"
```

---

## Task 2: Create English documentation `docs/index.html`

**Files:**
- Create: `docs/index.html`

- [ ] **Step 1: Create `docs/index.html` — head + nav + hero**

```html
<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <title>igv-rs — Interactive Terminal Genome Viewer</title>
  <meta name="description" content="Interactive terminal genome viewer for FASTA, VCF, BAM, GFF, BED, bigWig, and BEDPE, written in Rust.">
  <link rel="stylesheet" href="style.css">
</head>
<body>

<nav class="gh-nav">
  <a class="logo" href="#">
    <svg viewBox="0 0 16 16" fill="currentColor" aria-hidden="true">
      <path d="M8 0C3.58 0 0 3.58 0 8c0 3.54 2.29 6.53 5.47 7.59.4.07.55-.17.55-.38 0-.19-.01-.82-.01-1.49-2.01.37-2.53-.49-2.69-.94-.09-.23-.48-.94-.82-1.13-.28-.15-.68-.52-.01-.53.63-.01 1.08.58 1.23.82.72 1.21 1.87.87 2.33.66.07-.52.28-.87.51-1.07-1.78-.2-3.64-.89-3.64-3.95 0-.87.31-1.59.82-2.15-.08-.2-.36-1.02.08-2.12 0 0 .67-.21 2.2.82.64-.18 1.32-.27 2-.27.68 0 1.36.09 2 .27 1.53-1.04 2.2-.82 2.2-.82.44 1.1.16 1.92.08 2.12.51.56.82 1.27.82 2.15 0 3.07-1.87 3.75-3.65 3.95.29.25.54.73.54 1.48 0 1.07-.01 1.93-.01 2.2 0 .21.15.46.55.38A8.013 8.013 0 0016 8c0-4.42-3.58-8-8-8z"/>
    </svg>
    igv-rs
  </a>
  <div class="nav-links">
    <a href="#features">Features</a>
    <a href="#install">Install</a>
    <a href="#usage">Usage</a>
    <a href="#keybindings">Keybindings</a>
    <a href="#config">Config</a>
    <a href="#snapshot">Snapshot</a>
    <a href="#browser">Browser</a>
  </div>
  <div class="spacer"></div>
  <a class="lang-toggle" href="index.zh.html">中文</a>
  <a class="gh-btn" href="https://github.com/AI4S-YB/igv-rs" target="_blank" rel="noopener">GitHub</a>
</nav>

<section class="hero">
  <div class="badge">v0.7.0 · Rust</div>
  <h1>igv-rs</h1>
  <p class="tagline">Interactive terminal genome viewer for FASTA · VCF · BAM · GFF · BED · bigWig · BEDPE</p>
  <div class="install-box">
    <span class="prompt">$</span>
    <span>cargo install igv-rs</span>
  </div>
  <div class="hero-btns">
    <a class="btn-primary" href="#install">Get Started</a>
    <a class="btn-secondary" href="https://github.com/AI4S-YB/igv-rs" target="_blank" rel="noopener">GitHub ↗</a>
  </div>
</section>
```

- [ ] **Step 2: Append Features section**

```html
<main class="content">

<!-- Features -->
<div class="section" id="features">
  <div class="section-label">Overview</div>
  <h2>Features</h2>
  <div class="features">
    <div class="feature-card">
      <div class="icon">🧬</div>
      <h4>Multi-track</h4>
      <p>BAM · VCF · GFF · BED · bigWig · BEDPE — all in one synchronized view.</p>
    </div>
    <div class="feature-card">
      <div class="icon">⚡</div>
      <h4>Async non-blocking IO</h4>
      <p>Adaptive zoom-level rendering keeps the TUI responsive at any scale.</p>
    </div>
    <div class="feature-card">
      <div class="icon">🌐</div>
      <h4>Live browser companion</h4>
      <p>Press <code>B</code> to open igv.js in your browser, synced to the TUI view.</p>
    </div>
    <div class="feature-card">
      <div class="icon">📸</div>
      <h4>Snapshot export</h4>
      <p>Publication-quality SVG / PNG — interactive or headless batch mode.</p>
    </div>
    <div class="feature-card">
      <div class="icon">🎨</div>
      <h4>8 built-in themes</h4>
      <p>dark · light · paper · solarized · dracula · gruvbox — cycle with <code>t</code>.</p>
    </div>
    <div class="feature-card">
      <div class="icon">🔖</div>
      <h4>Command palette</h4>
      <p>Jump to any coordinate or gene name. Vim-style bookmarks with <code>m&lt;c&gt;</code> / <code>'&lt;c&gt;</code>.</p>
    </div>
  </div>
</div>
```

- [ ] **Step 3: Append Install section**

```html
<!-- Install -->
<div class="section" id="install">
  <div class="section-label">Getting Started</div>
  <h2>Install</h2>
  <p>Install the latest release from <a href="https://crates.io/crates/igv-rs" target="_blank" rel="noopener">crates.io</a>:</p>
  <div class="code-block">
    <span class="cmd"><span class="prompt">$</span> cargo install igv-rs</span>
  </div>
  <p>The igv.js browser companion is bundled into the binary — no extra assets required.</p>
  <p>Pre-built binaries for Linux / macOS / Windows are attached to each
    <a href="https://github.com/AI4S-YB/igv-rs/releases" target="_blank" rel="noopener">GitHub Release</a>.
  </p>
  <p>Build from source:</p>
  <div class="code-block">
    <span class="cmd"><span class="prompt">$</span> git clone https://github.com/AI4S-YB/igv-rs</span>
    <span class="cmd"><span class="prompt">$</span> cargo build --release</span>
    <span class="cmd"><span class="comment"># binary at target/release/igv-rs</span></span>
  </div>
</div>
```

- [ ] **Step 4: Append Usage section**

```html
<!-- Usage -->
<div class="section" id="usage">
  <div class="section-label">CLI</div>
  <h2>Usage</h2>
  <div class="code-block">
    <span class="cmd"><span class="comment"># Reference only</span></span>
    <span class="cmd">igv-rs reference.fa</span>
    <span class="cmd"></span>
    <span class="cmd"><span class="comment"># With variants</span></span>
    <span class="cmd">igv-rs reference.fa <span class="flag">-v</span> variants.vcf.gz</span>
    <span class="cmd"></span>
    <span class="cmd"><span class="comment"># With alignments</span></span>
    <span class="cmd">igv-rs reference.fa <span class="flag">-b</span> alignments.bam <span class="flag">-r</span> chr1:1000-2000</span>
    <span class="cmd"></span>
    <span class="cmd"><span class="comment"># Multiple BAM tracks</span></span>
    <span class="cmd">igv-rs reference.fa <span class="flag">-b</span> sample1.bam <span class="flag">-b</span> sample2.bam</span>
    <span class="cmd"></span>
    <span class="cmd"><span class="comment"># Annotations (GFF/GTF/BED/narrowPeak auto-detected by extension)</span></span>
    <span class="cmd">igv-rs reference.fa <span class="flag">-g</span> genes.gff3 <span class="flag">-b</span> sample.bam</span>
    <span class="cmd"></span>
    <span class="cmd"><span class="comment"># Signal tracks (bigWig)</span></span>
    <span class="cmd">igv-rs reference.fa <span class="flag">-s</span> chip.bw <span class="flag">-s</span> input.bw <span class="flag">-r</span> chr1:1-10000000</span>
    <span class="cmd"></span>
    <span class="cmd"><span class="comment"># Chromatin loops (BEDPE)</span></span>
    <span class="cmd">igv-rs reference.fa <span class="flag">-l</span> loops.bedpe <span class="flag">--link-min-score</span> 5.0</span>
    <span class="cmd"></span>
    <span class="cmd"><span class="comment"># Everything at once</span></span>
    <span class="cmd">igv-rs reference.fa <span class="flag">-b</span> sample.bam <span class="flag">-g</span> genes.gff3 <span class="flag">-s</span> rna.bw <span class="flag">-l</span> loops.bedpe <span class="flag">-r</span> chr1:1000-2000</span>
  </div>
  <p>The command palette (<code>:</code> or <code>g</code>) accepts coordinates (<code>chr1:1000-2000</code>) and
     gene names — type any <code>gene_name</code>, <code>gene_id</code>, or <code>transcript_id</code> from a loaded
     GFF/GTF/BED track and the view jumps to the union span of all matching transcripts.</p>
</div>
```

- [ ] **Step 5: Append Wide-zoom Behavior section**

```html
<!-- Wide-zoom -->
<div class="section" id="zoom">
  <div class="section-label">Rendering</div>
  <h2>Wide-zoom Behavior</h2>
  <p>At wider zoom levels igv-rs skips expensive fetches to stay responsive:</p>
  <table class="data-table">
    <thead>
      <tr>
        <th>View Width</th>
        <th>Reference</th>
        <th>Reads</th>
        <th>Variants</th>
        <th>Annotations</th>
        <th>Signals</th>
        <th>Links</th>
      </tr>
    </thead>
    <tbody>
      <tr>
        <td>≤ 50 kb (per-base)</td>
        <td class="yes">yes</td>
        <td class="yes">yes</td>
        <td class="yes">yes</td>
        <td>transcripts</td>
        <td class="yes">yes</td>
        <td class="yes">yes</td>
      </tr>
      <tr>
        <td>50 kb – 500 kb</td>
        <td class="no">—</td>
        <td class="no">—</td>
        <td class="yes">yes</td>
        <td>transcripts</td>
        <td class="yes">yes</td>
        <td class="yes">yes</td>
      </tr>
      <tr>
        <td>500 kb – 5 Mb</td>
        <td class="no">—</td>
        <td class="no">—</td>
        <td class="no">—</td>
        <td>transcripts</td>
        <td class="yes">yes</td>
        <td class="yes">yes</td>
      </tr>
      <tr>
        <td>&gt; 5 Mb (overview)</td>
        <td class="no">—</td>
        <td class="no">—</td>
        <td class="no">—</td>
        <td>gene density</td>
        <td class="yes">yes</td>
        <td class="yes">yes</td>
      </tr>
    </tbody>
  </table>
  <p>The footer shows a yellow "overview" hint when fetches are gated. bigWig signal tracks remain visible at every zoom level.</p>
</div>
```

- [ ] **Step 6: Append Snapshot Export section**

```html
<!-- Snapshot -->
<div class="section" id="snapshot">
  <div class="section-label">Export</div>
  <h2>Snapshot Export</h2>
  <p>Save publication-quality SVG or PNG figures — interactive or headless batch.</p>
  <p><strong>Interactive (inside the TUI):</strong></p>
  <div class="code-block">
    <span class="cmd"><span class="comment"># Press S to save current view</span></span>
    <span class="cmd">S  →  snapshot_chr1_1000_2000.svg</span>
    <span class="cmd"></span>
    <span class="cmd"><span class="comment"># Or use the command palette for a custom path / format</span></span>
    <span class="cmd">:snapshot path/to/figure.png</span>
  </div>
  <p><strong>Headless batch (no TUI opened):</strong></p>
  <div class="code-block">
    <span class="cmd"><span class="comment"># One snapshot per BED region (col 4 = filename stem)</span></span>
    <span class="cmd">igv-rs ref.fa <span class="flag">-b</span> s.bam <span class="flag">--snapshot-bed</span> regions.bed <span class="flag">--snapshot-out</span> out/</span>
    <span class="cmd"></span>
    <span class="cmd"><span class="comment"># One snapshot per gene name (requires -g)</span></span>
    <span class="cmd">igv-rs ref.fa <span class="flag">-b</span> s.bam <span class="flag">-g</span> genes.gtf <span class="flag">--snapshot-genes</span> list.txt <span class="flag">--snapshot-out</span> out/</span>
  </div>
  <p>Common flags: <code>--snapshot-format svg|png</code> · <code>--snapshot-width &lt;px&gt;</code> · <code>--snapshot-flank &lt;fraction&gt;</code> · <code>--snapshot-theme igv|tui</code></p>
</div>
```

- [ ] **Step 7: Append Browser View section**

```html
<!-- Browser View -->
<div class="section" id="browser">
  <div class="section-label">Live Preview</div>
  <h2>Browser View (igv.js)</h2>
  <p>Press <code>B</code> inside the TUI to launch a local HTTP server and open igv.js in your default browser.
     The browser tab starts at the TUI's current region and follows your navigation in real time.</p>
  <div class="code-block">
    <span class="cmd">igv-rs ref.fa <span class="flag">-b</span> sample.bam <span class="flag">-g</span> genes.gtf <span class="flag">-l</span> loops.bedpe</span>
    <span class="cmd"><span class="comment"># inside the TUI:</span></span>
    <span class="cmd"><span class="comment">#   B          → opens browser tab (igv.js)</span></span>
    <span class="cmd"><span class="comment">#   d / s / :gene → browser follows</span></span>
    <span class="cmd"><span class="comment">#   q          → exits TUI, shuts down server</span></span>
  </div>
  <p>The server binds to <code>127.0.0.1</code> on an ephemeral port (override with <code>--serve-port</code>).
     igv.js is bundled into the binary — browser view works offline. Disable with <code>--no-browser</code>.</p>
</div>
```

- [ ] **Step 8: Append Keybindings section**

```html
<!-- Keybindings -->
<div class="section" id="keybindings">
  <div class="section-label">Reference</div>
  <h2>Keybindings</h2>
  <table class="data-table">
    <thead><tr><th>Key</th><th>Action</th></tr></thead>
    <tbody>
      <tr><td><code>a</code> / <code>←</code></td><td>Page backward (one full window)</td></tr>
      <tr><td><code>d</code> / <code>→</code></td><td>Page forward (one full window)</td></tr>
      <tr><td><code>h</code></td><td>Move backward 1/10 window (fine pan)</td></tr>
      <tr><td><code>l</code></td><td>Move forward 1/10 window (fine pan)</td></tr>
      <tr><td><code>w</code> / <code>↑</code></td><td>Zoom in</td></tr>
      <tr><td><code>s</code> / <code>↓</code></td><td>Zoom out</td></tr>
      <tr><td><code>j</code> / <code>k</code></td><td>Scroll alignment lanes down / up</td></tr>
      <tr><td><code>+</code> / <code>-</code></td><td>Grow / shrink alignment-track height</td></tr>
      <tr><td><code>]</code> / <code>[</code></td><td>Grow / shrink coverage-track height</td></tr>
      <tr><td><code>\</code></td><td>Toggle signal shared / per-track Y-scale</td></tr>
      <tr><td><code>}</code> / <code>{</code></td><td>Grow / shrink signal-track height</td></tr>
      <tr><td><code>&lt;</code> / <code>&gt;</code></td><td>Shrink / grow link-track height</td></tr>
      <tr><td><code>:</code> or <code>g</code></td><td>Open command palette (coordinate or gene name)</td></tr>
      <tr><td><code>m&lt;c&gt;</code></td><td>Set bookmark to letter <em>c</em></td></tr>
      <tr><td><code>'&lt;c&gt;</code></td><td>Jump to bookmark <em>c</em></td></tr>
      <tr><td><code>t</code></td><td>Cycle theme</td></tr>
      <tr><td><code>S</code></td><td>Save SVG snapshot of current view</td></tr>
      <tr><td><code>B</code></td><td>Open browser view (igv.js)</td></tr>
      <tr><td><code>?</code></td><td>Toggle keybinding help overlay</td></tr>
      <tr><td><code>q</code> / Ctrl-C</td><td>Quit</td></tr>
    </tbody>
  </table>
</div>
```

- [ ] **Step 9: Append Configuration section**

```html
<!-- Config -->
<div class="section" id="config">
  <div class="section-label">Configuration</div>
  <h2>Configuration</h2>
  <p>Optional <code>~/.config/igv-rs/config.toml</code> is read at startup:</p>
  <div class="code-block">
    <span class="cmd">[theme]</span>
    <span class="cmd"><span class="comment"># "dark" | "light" | "paper" | "solarized-dark" | "solarized-light"</span></span>
    <span class="cmd"><span class="comment"># | "dracula" | "gruvbox-dark"</span></span>
    <span class="cmd">preset = "dark"</span>
    <span class="cmd"></span>
    <span class="cmd">[theme.custom]</span>
    <span class="cmd"><span class="comment"># Override individual style keys</span></span>
    <span class="cmd">"A"        = "bold green"</span>
    <span class="cmd">"MISMATCH" = "bold white on red"</span>
    <span class="cmd">"SIGNAL"   = "cyan"</span>
    <span class="cmd">"LINK"     = "magenta"</span>
    <span class="cmd"></span>
    <span class="cmd">[serve]</span>
    <span class="cmd">auto_open = true   <span class="comment"># --no-browser overrides</span></span>
    <span class="cmd">port      = 0      <span class="comment"># 0 = ephemeral; --serve-port overrides</span></span>
  </div>
</div>
```

- [ ] **Step 10: Append Known Limitations section + close tags**

```html
<!-- Known Limitations -->
<div class="section" id="limitations">
  <div class="section-label">Caveats</div>
  <h2>Known Limitations</h2>
  <ul class="limits-list">
    <li><strong>Held-key debounce</strong> not implemented — tap rather than hold navigation keys.</li>
    <li><strong><code>[render]</code> config keys</strong> (zoom_factor, nav_overlap, threshold overrides) not yet read.</li>
    <li><strong><code>[bookmarks]</code> config table</strong> not loaded — in-session bookmarks work fully.</li>
    <li><strong>Coverage at wide zoom</strong> is hidden, not heat-mapped — use a precomputed bigWig for chromosome-scale depth.</li>
    <li><strong>BAM tag display</strong> uses Rust's Debug formatting (e.g. <code>Int8(42)</code> instead of <code>42</code>) when coloring by tag.</li>
    <li><strong>No signal-track caching</strong> — every region change re-fetches bigWig.</li>
    <li><strong>Single signal colormap</strong> — all bigWig tracks share the <code>SIGNAL</code> theme key.</li>
    <li><strong>bigBed (<code>.bb</code>)</strong> not yet supported.</li>
    <li><strong>No tabix / pairix for link tracks</strong> — BEDPE files &gt;1M records load slowly.</li>
    <li><strong>Browser view is loopback-only</strong> — no remote access, no auth tokens.</li>
  </ul>
</div>

</main>

<footer class="gh-footer">
  <p>
    igv-rs &nbsp;·&nbsp;
    <a href="https://github.com/AI4S-YB/igv-rs" target="_blank" rel="noopener">GitHub</a> &nbsp;·&nbsp;
    <a href="https://crates.io/crates/igv-rs" target="_blank" rel="noopener">crates.io</a> &nbsp;·&nbsp;
    MIT License
  </p>
</footer>

</body>
</html>
```

- [ ] **Step 11: Open `docs/index.html` in a browser and verify**

Check:
- Nav sticks to top when scrolling
- Hero shows badge, h1, install box, two buttons
- Features renders as 3-column card grid
- All code blocks have green `$` prompt and blue flags
- Wide-zoom table renders with yes/— columns
- Keybindings table has alternating row shading
- Language toggle link visible in nav (`中文`)
- No broken layout at 480px wide (responsive)

- [ ] **Step 12: Commit**

```bash
git add docs/index.html
git commit -m "docs: add English single-page documentation site"
```

---

## Task 3: Create Chinese documentation `docs/index.zh.html`

**Files:**
- Create: `docs/index.zh.html`

- [ ] **Step 1: Create `docs/index.zh.html`**

Full Chinese translation — identical structure to `index.html`, all user-visible text translated. Code blocks, CLI flags, and key names remain in English.

```html
<!DOCTYPE html>
<html lang="zh">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <title>igv-rs — 交互式终端基因组浏览器</title>
  <meta name="description" content="适用于 FASTA、VCF、BAM、GFF、BED、bigWig 和 BEDPE 的交互式终端基因组浏览器，使用 Rust 编写。">
  <link rel="stylesheet" href="style.css">
</head>
<body>

<nav class="gh-nav">
  <a class="logo" href="index.zh.html">
    <svg viewBox="0 0 16 16" fill="currentColor" aria-hidden="true">
      <path d="M8 0C3.58 0 0 3.58 0 8c0 3.54 2.29 6.53 5.47 7.59.4.07.55-.17.55-.38 0-.19-.01-.82-.01-1.49-2.01.37-2.53-.49-2.69-.94-.09-.23-.48-.94-.82-1.13-.28-.15-.68-.52-.01-.53.63-.01 1.08.58 1.23.82.72 1.21 1.87.87 2.33.66.07-.52.28-.87.51-1.07-1.78-.2-3.64-.89-3.64-3.95 0-.87.31-1.59.82-2.15-.08-.2-.36-1.02.08-2.12 0 0 .67-.21 2.2.82.64-.18 1.32-.27 2-.27.68 0 1.36.09 2 .27 1.53-1.04 2.2-.82 2.2-.82.44 1.1.16 1.92.08 2.12.51.56.82 1.27.82 2.15 0 3.07-1.87 3.75-3.65 3.95.29.25.54.73.54 1.48 0 1.07-.01 1.93-.01 2.2 0 .21.15.46.55.38A8.013 8.013 0 0016 8c0-4.42-3.58-8-8-8z"/>
    </svg>
    igv-rs
  </a>
  <div class="nav-links">
    <a href="#features">功能</a>
    <a href="#install">安装</a>
    <a href="#usage">用法</a>
    <a href="#keybindings">快捷键</a>
    <a href="#config">配置</a>
    <a href="#snapshot">截图</a>
    <a href="#browser">浏览器</a>
  </div>
  <div class="spacer"></div>
  <a class="lang-toggle" href="index.html">English</a>
  <a class="gh-btn" href="https://github.com/AI4S-YB/igv-rs" target="_blank" rel="noopener">GitHub</a>
</nav>

<section class="hero">
  <div class="badge">v0.7.0 · Rust</div>
  <h1>igv-rs</h1>
  <p class="tagline">适用于 FASTA · VCF · BAM · GFF · BED · bigWig · BEDPE 的交互式终端基因组浏览器</p>
  <div class="install-box">
    <span class="prompt">$</span>
    <span>cargo install igv-rs</span>
  </div>
  <div class="hero-btns">
    <a class="btn-primary" href="#install">快速开始</a>
    <a class="btn-secondary" href="https://github.com/AI4S-YB/igv-rs" target="_blank" rel="noopener">GitHub ↗</a>
  </div>
</section>

<main class="content">

<!-- 功能 -->
<div class="section" id="features">
  <div class="section-label">概览</div>
  <h2>功能特性</h2>
  <div class="features">
    <div class="feature-card">
      <div class="icon">🧬</div>
      <h4>多轨道支持</h4>
      <p>BAM · VCF · GFF · BED · bigWig · BEDPE — 在同一视图中同步显示。</p>
    </div>
    <div class="feature-card">
      <div class="icon">⚡</div>
      <h4>异步非阻塞 IO</h4>
      <p>自适应缩放级别渲染，在任意比例下保持 TUI 响应流畅。</p>
    </div>
    <div class="feature-card">
      <div class="icon">🌐</div>
      <h4>浏览器实时伴侣</h4>
      <p>按 <code>B</code> 在浏览器中打开与 TUI 同步的 igv.js 视图。</p>
    </div>
    <div class="feature-card">
      <div class="icon">📸</div>
      <h4>快照导出</h4>
      <p>发表级 SVG / PNG — 支持交互式保存和无头批量模式。</p>
    </div>
    <div class="feature-card">
      <div class="icon">🎨</div>
      <h4>8 种内置主题</h4>
      <p>dark · light · paper · solarized · dracula · gruvbox — 按 <code>t</code> 循环切换。</p>
    </div>
    <div class="feature-card">
      <div class="icon">🔖</div>
      <h4>命令面板</h4>
      <p>跳转到任意坐标或基因名称。Vim 风格书签 <code>m&lt;c&gt;</code> / <code>'&lt;c&gt;</code>。</p>
    </div>
  </div>
</div>

<!-- 安装 -->
<div class="section" id="install">
  <div class="section-label">快速开始</div>
  <h2>安装</h2>
  <p>从 <a href="https://crates.io/crates/igv-rs" target="_blank" rel="noopener">crates.io</a> 安装最新版本：</p>
  <div class="code-block">
    <span class="cmd"><span class="prompt">$</span> cargo install igv-rs</span>
  </div>
  <p>igv.js 浏览器伴侣已打包进二进制文件，无需额外资源。</p>
  <p>Linux / macOS / Windows 预编译二进制文件可在
    <a href="https://github.com/AI4S-YB/igv-rs/releases" target="_blank" rel="noopener">GitHub Releases</a>
    页面下载。
  </p>
  <p>从源码构建：</p>
  <div class="code-block">
    <span class="cmd"><span class="prompt">$</span> git clone https://github.com/AI4S-YB/igv-rs</span>
    <span class="cmd"><span class="prompt">$</span> cargo build --release</span>
    <span class="cmd"><span class="comment"># 二进制文件位于 target/release/igv-rs</span></span>
  </div>
</div>

<!-- 用法 -->
<div class="section" id="usage">
  <div class="section-label">命令行</div>
  <h2>用法</h2>
  <div class="code-block">
    <span class="cmd"><span class="comment"># 仅参考序列</span></span>
    <span class="cmd">igv-rs reference.fa</span>
    <span class="cmd"></span>
    <span class="cmd"><span class="comment"># 加载变异</span></span>
    <span class="cmd">igv-rs reference.fa <span class="flag">-v</span> variants.vcf.gz</span>
    <span class="cmd"></span>
    <span class="cmd"><span class="comment"># 加载比对</span></span>
    <span class="cmd">igv-rs reference.fa <span class="flag">-b</span> alignments.bam <span class="flag">-r</span> chr1:1000-2000</span>
    <span class="cmd"></span>
    <span class="cmd"><span class="comment"># 多 BAM 轨道</span></span>
    <span class="cmd">igv-rs reference.fa <span class="flag">-b</span> sample1.bam <span class="flag">-b</span> sample2.bam</span>
    <span class="cmd"></span>
    <span class="cmd"><span class="comment"># 注释轨道（GFF/GTF/BED/narrowPeak 按扩展名自动识别）</span></span>
    <span class="cmd">igv-rs reference.fa <span class="flag">-g</span> genes.gff3 <span class="flag">-b</span> sample.bam</span>
    <span class="cmd"></span>
    <span class="cmd"><span class="comment"># 信号轨道（bigWig）</span></span>
    <span class="cmd">igv-rs reference.fa <span class="flag">-s</span> chip.bw <span class="flag">-s</span> input.bw <span class="flag">-r</span> chr1:1-10000000</span>
    <span class="cmd"></span>
    <span class="cmd"><span class="comment"># 染色质环（BEDPE）</span></span>
    <span class="cmd">igv-rs reference.fa <span class="flag">-l</span> loops.bedpe <span class="flag">--link-min-score</span> 5.0</span>
    <span class="cmd"></span>
    <span class="cmd"><span class="comment"># 综合示例</span></span>
    <span class="cmd">igv-rs reference.fa <span class="flag">-b</span> sample.bam <span class="flag">-g</span> genes.gff3 <span class="flag">-s</span> rna.bw <span class="flag">-l</span> loops.bedpe <span class="flag">-r</span> chr1:1000-2000</span>
  </div>
  <p>命令面板（<code>:</code> 或 <code>g</code>）支持坐标（<code>chr1:1000-2000</code>）和基因名称 — 输入已加载 GFF/GTF/BED 轨道中的 <code>gene_name</code>、<code>gene_id</code> 或 <code>transcript_id</code>，视图将跳转到所有匹配转录本的联合区间。</p>
</div>

<!-- 缩放行为 -->
<div class="section" id="zoom">
  <div class="section-label">渲染</div>
  <h2>宽视图缩放行为</h2>
  <p>在较宽的缩放级别下，igv-rs 会跳过高开销的数据获取以保持响应：</p>
  <table class="data-table">
    <thead>
      <tr>
        <th>视图宽度</th>
        <th>参考序列</th>
        <th>比对读段</th>
        <th>变异</th>
        <th>注释</th>
        <th>信号</th>
        <th>环</th>
      </tr>
    </thead>
    <tbody>
      <tr>
        <td>≤ 50 kb（碱基级）</td>
        <td class="yes">是</td>
        <td class="yes">是</td>
        <td class="yes">是</td>
        <td>转录本</td>
        <td class="yes">是</td>
        <td class="yes">是</td>
      </tr>
      <tr>
        <td>50 kb – 500 kb</td>
        <td class="no">—</td>
        <td class="no">—</td>
        <td class="yes">是</td>
        <td>转录本</td>
        <td class="yes">是</td>
        <td class="yes">是</td>
      </tr>
      <tr>
        <td>500 kb – 5 Mb</td>
        <td class="no">—</td>
        <td class="no">—</td>
        <td class="no">—</td>
        <td>转录本</td>
        <td class="yes">是</td>
        <td class="yes">是</td>
      </tr>
      <tr>
        <td>&gt; 5 Mb（概览）</td>
        <td class="no">—</td>
        <td class="no">—</td>
        <td class="no">—</td>
        <td>基因密度</td>
        <td class="yes">是</td>
        <td class="yes">是</td>
      </tr>
    </tbody>
  </table>
  <p>当数据获取被限制时，页脚会显示黄色"概览"提示。bigWig 信号轨道在所有缩放级别下均可见。</p>
</div>

<!-- 快照导出 -->
<div class="section" id="snapshot">
  <div class="section-label">导出</div>
  <h2>快照导出</h2>
  <p>保存发表级 SVG 或 PNG 图像 — 支持交互式和无头批量两种模式。</p>
  <p><strong>交互式（在 TUI 内）：</strong></p>
  <div class="code-block">
    <span class="cmd"><span class="comment"># 按 S 保存当前视图</span></span>
    <span class="cmd">S  →  snapshot_chr1_1000_2000.svg</span>
    <span class="cmd"></span>
    <span class="cmd"><span class="comment"># 或通过命令面板指定路径 / 格式</span></span>
    <span class="cmd">:snapshot path/to/figure.png</span>
  </div>
  <p><strong>无头批量（不打开 TUI）：</strong></p>
  <div class="code-block">
    <span class="cmd"><span class="comment"># 每个 BED 区域一张快照（第 4 列为文件名前缀）</span></span>
    <span class="cmd">igv-rs ref.fa <span class="flag">-b</span> s.bam <span class="flag">--snapshot-bed</span> regions.bed <span class="flag">--snapshot-out</span> out/</span>
    <span class="cmd"></span>
    <span class="cmd"><span class="comment"># 每个基因名称一张快照（需要 -g 注释）</span></span>
    <span class="cmd">igv-rs ref.fa <span class="flag">-b</span> s.bam <span class="flag">-g</span> genes.gtf <span class="flag">--snapshot-genes</span> list.txt <span class="flag">--snapshot-out</span> out/</span>
  </div>
  <p>常用参数：<code>--snapshot-format svg|png</code> · <code>--snapshot-width &lt;px&gt;</code> · <code>--snapshot-flank &lt;fraction&gt;</code> · <code>--snapshot-theme igv|tui</code></p>
</div>

<!-- 浏览器视图 -->
<div class="section" id="browser">
  <div class="section-label">实时预览</div>
  <h2>浏览器视图（igv.js）</h2>
  <p>在 TUI 内按 <code>B</code> 启动本地 HTTP 服务器，并在默认浏览器中打开 igv.js。浏览器标签页从 TUI 当前区域开始，并实时跟随导航。</p>
  <div class="code-block">
    <span class="cmd">igv-rs ref.fa <span class="flag">-b</span> sample.bam <span class="flag">-g</span> genes.gtf <span class="flag">-l</span> loops.bedpe</span>
    <span class="cmd"><span class="comment"># 在 TUI 内：</span></span>
    <span class="cmd"><span class="comment">#   B          → 打开浏览器标签页（igv.js）</span></span>
    <span class="cmd"><span class="comment">#   d / s / :gene → 浏览器同步跟随</span></span>
    <span class="cmd"><span class="comment">#   q          → 退出 TUI，关闭服务器</span></span>
  </div>
  <p>服务器绑定在 <code>127.0.0.1</code> 的临时端口（可通过 <code>--serve-port</code> 覆盖）。igv.js 已打包进二进制，离线可用。使用 <code>--no-browser</code> 禁用此功能。</p>
</div>

<!-- 快捷键 -->
<div class="section" id="keybindings">
  <div class="section-label">参考</div>
  <h2>快捷键</h2>
  <table class="data-table">
    <thead><tr><th>按键</th><th>功能</th></tr></thead>
    <tbody>
      <tr><td><code>a</code> / <code>←</code></td><td>向前翻页（整个视窗）</td></tr>
      <tr><td><code>d</code> / <code>→</code></td><td>向后翻页（整个视窗）</td></tr>
      <tr><td><code>h</code></td><td>向前平移 1/10 视窗（精细）</td></tr>
      <tr><td><code>l</code></td><td>向后平移 1/10 视窗（精细）</td></tr>
      <tr><td><code>w</code> / <code>↑</code></td><td>放大</td></tr>
      <tr><td><code>s</code> / <code>↓</code></td><td>缩小</td></tr>
      <tr><td><code>j</code> / <code>k</code></td><td>比对泳道向下 / 向上滚动</td></tr>
      <tr><td><code>+</code> / <code>-</code></td><td>增大 / 缩小比对轨道高度</td></tr>
      <tr><td><code>]</code> / <code>[</code></td><td>增大 / 缩小覆盖度轨道高度</td></tr>
      <tr><td><code>\</code></td><td>切换信号轨道共享 / 独立 Y 轴</td></tr>
      <tr><td><code>}</code> / <code>{</code></td><td>增大 / 缩小信号轨道高度</td></tr>
      <tr><td><code>&lt;</code> / <code>&gt;</code></td><td>缩小 / 增大环轨道高度</td></tr>
      <tr><td><code>:</code> 或 <code>g</code></td><td>打开命令面板（坐标或基因名称）</td></tr>
      <tr><td><code>m&lt;c&gt;</code></td><td>设置书签到字母 <em>c</em></td></tr>
      <tr><td><code>'&lt;c&gt;</code></td><td>跳转到书签 <em>c</em></td></tr>
      <tr><td><code>t</code></td><td>循环切换主题</td></tr>
      <tr><td><code>S</code></td><td>保存当前视图 SVG 快照</td></tr>
      <tr><td><code>B</code></td><td>打开浏览器视图（igv.js）</td></tr>
      <tr><td><code>?</code></td><td>切换快捷键帮助覆盖层</td></tr>
      <tr><td><code>q</code> / Ctrl-C</td><td>退出</td></tr>
    </tbody>
  </table>
</div>

<!-- 配置 -->
<div class="section" id="config">
  <div class="section-label">配置</div>
  <h2>配置文件</h2>
  <p>启动时自动读取可选的 <code>~/.config/igv-rs/config.toml</code>：</p>
  <div class="code-block">
    <span class="cmd">[theme]</span>
    <span class="cmd"><span class="comment"># "dark" | "light" | "paper" | "solarized-dark" | "solarized-light"</span></span>
    <span class="cmd"><span class="comment"># | "dracula" | "gruvbox-dark"</span></span>
    <span class="cmd">preset = "dark"</span>
    <span class="cmd"></span>
    <span class="cmd">[theme.custom]</span>
    <span class="cmd"><span class="comment"># 覆盖单个样式键</span></span>
    <span class="cmd">"A"        = "bold green"</span>
    <span class="cmd">"MISMATCH" = "bold white on red"</span>
    <span class="cmd">"SIGNAL"   = "cyan"</span>
    <span class="cmd">"LINK"     = "magenta"</span>
    <span class="cmd"></span>
    <span class="cmd">[serve]</span>
    <span class="cmd">auto_open = true   <span class="comment"># --no-browser 可覆盖</span></span>
    <span class="cmd">port      = 0      <span class="comment"># 0 = 临时端口；--serve-port 可覆盖</span></span>
  </div>
</div>

<!-- 已知限制 -->
<div class="section" id="limitations">
  <div class="section-label">注意事项</div>
  <h2>已知限制</h2>
  <ul class="limits-list">
    <li><strong>长按按键去抖</strong>未实现 — 建议点击而非长按导航键。</li>
    <li><strong><code>[render]</code> 配置键</strong>（zoom_factor、nav_overlap、阈值覆盖）尚未读取。</li>
    <li><strong><code>[bookmarks]</code> 配置表</strong>未加载 — 会话内书签功能正常。</li>
    <li><strong>宽视图覆盖度</strong>被隐藏而非热力图显示 — 染色体级别深度建议使用预计算 bigWig。</li>
    <li><strong>BAM 标签显示</strong>按标签着色时使用 Rust 的 Debug 格式（如 <code>Int8(42)</code> 而非 <code>42</code>）。</li>
    <li><strong>无信号轨道缓存</strong> — 每次区域变更均重新获取 bigWig 数据。</li>
    <li><strong>单一信号色图</strong> — 所有 bigWig 轨道共用 <code>SIGNAL</code> 主题键。</li>
    <li><strong>bigBed（<code>.bb</code>）</strong>暂不支持。</li>
    <li><strong>环轨道无 tabix/pairix 支持</strong> — 超过 100 万条记录的 BEDPE 文件加载较慢。</li>
    <li><strong>浏览器视图仅限本机回环</strong> — 不支持远程访问和认证令牌。</li>
  </ul>
</div>

</main>

<footer class="gh-footer">
  <p>
    igv-rs &nbsp;·&nbsp;
    <a href="https://github.com/AI4S-YB/igv-rs" target="_blank" rel="noopener">GitHub</a> &nbsp;·&nbsp;
    <a href="https://crates.io/crates/igv-rs" target="_blank" rel="noopener">crates.io</a> &nbsp;·&nbsp;
    MIT 许可证
  </p>
</footer>

</body>
</html>
```

- [ ] **Step 2: Open `docs/index.zh.html` in a browser and verify**

Check:
- `<html lang="zh">` is set
- Nav shows 中文 section names
- Language toggle shows "English" and links to `index.html`
- All code blocks unchanged (English CLI commands)
- Footer shows "MIT 许可证"
- Layout identical to English version

- [ ] **Step 3: Commit**

```bash
git add docs/index.zh.html
git commit -m "docs: add Chinese single-page documentation site"
```

---

## Task 4: Add `.nojekyll` and enable GitHub Pages

**Files:**
- Create: `docs/.nojekyll`

- [ ] **Step 1: Create `.nojekyll`**

```bash
touch docs/.nojekyll
```

This empty file tells GitHub Pages to skip Jekyll processing so files with leading underscores and the CSS are served as-is.

- [ ] **Step 2: Commit**

```bash
git add docs/.nojekyll
git commit -m "docs: add .nojekyll for GitHub Pages"
```

- [ ] **Step 3: Push and enable GitHub Pages**

```bash
git push origin main
```

Then in the GitHub repo:
1. Go to **Settings → Pages**
2. Under "Build and deployment" → Source: **Deploy from a branch**
3. Branch: **main** / Folder: **`/docs`**
4. Click **Save**

After ~60 seconds, the site is live at `https://<org>.github.io/igv-rs/`.

- [ ] **Step 4: Verify live site**

Open `https://<org>.github.io/igv-rs/` and confirm:
- English page loads with correct styles
- `/igv-rs/index.zh.html` loads Chinese version
- Language toggle works between the two
- All anchor links scroll to correct sections
- Nav stays sticky on scroll
