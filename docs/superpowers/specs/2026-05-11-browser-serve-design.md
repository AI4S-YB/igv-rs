# Browser-serve design — embed igv.js via a local HTTP server

**Status:** Draft (design approved 2026-05-11; implementation plan pending)
**Spec id:** `2026-05-11-browser-serve`
**Owner:** igv-rs contributors

## 1. Motivation

`igv-rs` already produces two views of genomic data: an interactive terminal
UI (`igv-tui`) and headless SVG / PNG snapshots (`igv-render`). Both are
text-or-bitmap output. Several tasks (mouse-driven zoom, tooltip inspection,
showing a result to a non-CLI colleague on the same workstation) are better
served by the existing [igv.js](https://github.com/igvteam/igv.js) browser
viewer. This spec adds a third output: spin up a local HTTP server inside the
TUI process, bundle igv.js into the binary, and have the browser mirror the
TUI's current view.

The user-facing flow is a single keystroke:

1. Run `igv-rs reference.fa -b sample.bam -g genes.gtf -l loops.bedpe` as
   usual.
2. Press `B` inside the TUI.
3. A browser tab opens at `http://127.0.0.1:<port>/` showing the same tracks
   in igv.js, jumped to the TUI's current region.
4. Continue navigating in the TUI; the browser viewport follows via
   Server-Sent Events.

The scope is single-user, single-workstation, loopback-only. Remote access,
auth, and sharing are explicitly out of scope (see §10).

## 2. Decisions locked during brainstorming

| # | Decision | Chosen value |
|---|----------|---|
| 1 | Purpose | Reuse the `igv-core` data pipeline rather than expose raw files only |
| 2 | Integration depth | Hybrid: binary tracks (FASTA/BAM/bigWig/VCF) served as static Range-fetched files; annotation and link tracks served as JSON from igv-core |
| 3 | Launch | TUI keystroke (`B`); no separate `igv-rs serve` subcommand in v1 |
| 4 | Sync direction | TUI → browser (one-way push via SSE) |
| 5 | igv.js delivery | Bundled into the binary with `include_bytes!` |
| 6 | Crate layout | New crate `igv-serve` consuming `igv-core`, called from `igv-tui` — parallel to `igv-render` |
| 7 | VCF in v1 | Static file (igv.js owns tabix parsing); JSON path deferred to v2 |
| 8 | Security | Hardcoded `127.0.0.1`, file whitelist, no token; one-shot use cases only |

## 3. Architecture

```
                     ┌─────────────────────────┐
                     │       igv-tui (bin)     │
                     │                         │
   keystroke `B` ───▶│  ServeController        │
                     │   ├─ ensure_started()   │
                     │   ├─ open browser       │
   region change ───▶│   └─ notify_view()      │
                     │           │             │
                     └───────────┼─────────────┘
                                 │ Arc<dyn Source> + broadcast<ViewEvent>
                                 ▼
                     ┌─────────────────────────┐
                     │      igv-serve (lib)    │
                     │                         │
                     │  axum Router            │
                     │  ├─ /                   │
                     │  ├─ /assets/igv.esm…    │
                     │  ├─ /api/config         │
                     │  ├─ /file/...           │ ◀── tower_http::ServeFile (Range)
                     │  ├─ /api/features/...   │ ◀── igv-core sources
                     │  ├─ /api/jump           │
                     │  └─ /api/sse            │ ◀── broadcast::Receiver
                     └───────────┬─────────────┘
                                 │ HTTP (loopback only)
                                 ▼
                     ┌─────────────────────────┐
                     │   user's web browser    │
                     │   igv.js                │
                     └─────────────────────────┘
```

`igv-serve` depends on `igv-core` only. `igv-tui` depends on `igv-serve` for
the lifecycle helpers. `igv-render` is untouched.

## 4. Crate layout

```
crates/igv-serve/
├── Cargo.toml
├── assets/
│   └── igv.esm.min.js          # vendored from upstream tag; updated via PR
├── src/
│   ├── lib.rs                  # public API: ServerHandle, spawn, ServerConfig, ViewEvent
│   ├── state.rs                # ServerState (Arc sources + broadcast::Sender)
│   ├── view.rs                 # ViewEvent { chrom, start, end }, ViewSnapshot
│   ├── routes/
│   │   ├── mod.rs              # Router assembly
│   │   ├── index.rs            # GET /
│   │   ├── assets.rs           # GET /assets/igv.esm.min.js
│   │   ├── config.rs           # GET /api/config
│   │   ├── file.rs             # GET /file/<kind>/<idx>
│   │   ├── features.rs         # GET /api/features/<kind>/<idx>
│   │   ├── jump.rs             # GET /api/jump
│   │   └── sse.rs              # GET /api/sse
│   └── feature_json.rs         # igv-core record → igv.js JSON shape
└── tests/
    └── http.rs                 # axum/tower oneshot + reqwest end-to-end
```

### 4.1 Public API

```rust
pub struct ServerHandle {
    pub addr: SocketAddr,
    pub events: broadcast::Sender<ViewEvent>, // exposed for push_view
    join: JoinHandle<()>,
    shutdown: oneshot::Sender<()>,
}

pub struct ServerConfig {
    pub bind: IpAddr,                                    // forced to 127.0.0.1 in v1
    pub port: u16,                                       // 0 = ephemeral
    pub fasta: Arc<dyn FastaSource>,
    pub fasta_path: PathBuf,
    pub bams: Vec<TrackEntry<dyn BamSource>>,
    pub vcfs: Vec<TrackEntry<dyn VcfSource>>,
    pub annotations: Vec<TrackEntry<dyn AnnotationSource>>,
    pub signals: Vec<TrackEntry<dyn SignalSource>>,
    pub links: Vec<TrackEntry<dyn LinkSource>>,
    pub initial: ViewSnapshot,
    pub link_min_score: Option<f64>,
}

pub struct TrackEntry<S: ?Sized> {
    pub source: Arc<S>,
    pub path: PathBuf,      // for /file serving
    pub display: String,    // shown in igv.js track name
}

pub async fn spawn(cfg: ServerConfig) -> Result<ServerHandle, ServeError>;

impl ServerHandle {
    pub fn push_view(&self, ev: ViewEvent);
    pub async fn shutdown(self);
}
```

`ServeError` is a `thiserror` enum covering `BindFailed`, `MissingIndex`,
`IoError`.

### 4.2 igv-tui integration

```
crates/igv-tui/src/app/serve.rs       (new)
crates/igv-tui/src/app/action.rs      add Action::OpenBrowser
crates/igv-tui/src/input.rs           bind 'B' → Action::OpenBrowser
crates/igv-tui/src/ui/widgets/doc.rs  add `B` row to keybinding help
crates/igv-tui/src/main.rs            wire ServeController into the main loop
```

`ServeController` owns `Option<ServerHandle>`. First `B` press calls
`igv_serve::spawn`; subsequent presses only reopen the browser. After every
TUI navigation that commits (existing `expected_loads == 0` hook), the main
loop calls `controller.notify_view(&state)`.

## 5. HTTP API contract

### 5.1 Routes

| Route | Method | Purpose | Notes |
|---|---|---|---|
| `/` | GET | HTML shell with igv.js boot script | served as `text/html; charset=utf-8` |
| `/assets/igv.esm.min.js` | GET | Bundled igv.js | `application/javascript`, `Cache-Control: public, max-age=31536000, immutable` |
| `/api/config` | GET | Initial igv.js browser config | JSON; locus comes from `ServerConfig.initial` |
| `/file/fasta` `/file/fasta.fai` | GET | FASTA + index | `tower_http::services::ServeFile`, Range-enabled |
| `/file/bam/{idx}` `/file/bam/{idx}.bai` | GET | BAM + index | same |
| `/file/vcf/{idx}` `/file/vcf/{idx}.tbi` | GET | VCF + tabix | same |
| `/file/signal/{idx}` | GET | bigWig | same |
| `/api/features/annotation/{idx}` | GET | GFF/GTF/BED → JSON | query `chrom`, `start`, `end` |
| `/api/features/link/{idx}` | GET | BEDPE → JSON | applies `link_min_score` |
| `/api/jump?name=BRCA1` | GET | Gene name → region | gene index from igv-core |
| `/api/sse` | GET | TUI → browser push | `text/event-stream` |

### 5.2 `/api/config` response

```json
{
  "reference": {
    "id": "user-fasta",
    "name": "Reference",
    "fastaURL": "/file/fasta",
    "indexURL": "/file/fasta.fai",
    "wholeGenomeView": false
  },
  "locus": "chr1:1000-2000",
  "tracks": [
    {
      "name": "sample1.bam",
      "type": "alignment",
      "format": "bam",
      "url": "/file/bam/0",
      "indexURL": "/file/bam/0.bai"
    },
    {
      "name": "variants.vcf.gz",
      "type": "variant",
      "format": "vcf",
      "url": "/file/vcf/0",
      "indexURL": "/file/vcf/0.tbi"
    },
    {
      "name": "rna.bw",
      "type": "wig",
      "format": "bigwig",
      "url": "/file/signal/0"
    },
    {
      "name": "genes.gtf",
      "type": "annotation",
      "sourceType": "custom",
      "source": {
        "url": "/api/features/annotation/0",
        "queryable": true
      }
    },
    {
      "name": "loops.bedpe",
      "type": "interact",
      "sourceType": "custom",
      "source": { "url": "/api/features/link/0", "queryable": true }
    }
  ]
}
```

### 5.3 Feature JSON shapes

**Annotation** — emitted by `feature_json::annotation`:

```json
[
  {
    "chr": "chr1",
    "start": 1000,
    "end": 2000,
    "name": "BRCA1",
    "strand": "+",
    "type": "transcript",
    "exons": [
      { "start": 1000, "end": 1200 },
      { "start": 1500, "end": 2000 }
    ]
  }
]
```

**Link (BEDPE → igv.js interact)** — emitted by `feature_json::link`:

```json
[
  {
    "chr1": "chr1", "start1": 1000, "end1": 2000,
    "chr2": "chr1", "start2": 5000, "end2": 6000,
    "score": 12.3, "value": 12.3,
    "name": "loop_1"
  }
]
```

### 5.4 SSE protocol

```
event: view
data: {"chrom":"chr1","start":1000,"end":2000}
```

Browser handler: `browser.search(`${chrom}:${start}-${end}`)`.

Multiple tabs subscribe to the same broadcast channel; lag drops the oldest
events (only the latest view matters).

## 6. TUI lifecycle

```
TUI startup
   └─ no server started (lazy)

user presses B (first time)
   ├─ ServeController::ensure_started(&state)
   │    ├─ build ServerConfig from AppState (Arcs already loaded)
   │    └─ tokio::spawn(igv_serve::spawn(cfg))
   ├─ store url = "http://127.0.0.1:<port>/"
   └─ webbrowser::open(url)

user presses B (subsequent)
   └─ webbrowser::open(url) only

any region change committed
   ├─ ev = ViewEvent { chrom, start, end }
   ├─ skip if ev == last_pushed
   └─ handle.push_view(ev)

TUI exit
   └─ ServeController::Drop → oneshot send (graceful axum shutdown)
```

Debounce: `last_pushed` avoids re-broadcasting unchanged regions. Held-key
panning still incurs one event per committed fetch (acceptable — matches the
TUI itself).

## 7. CLI and config

```
--no-browser            disable the B key entirely
--serve-port <PORT>     explicit port, default 0 (ephemeral)
```

`~/.config/igv-rs/config.toml`:

```toml
[serve]
auto_open = true   # if false, B just prints URL to footer
port = 0
# bind = "127.0.0.1"   # reserved key, ignored in v1
```

Footer behaviour: pressing `B` flashes `browser → http://127.0.0.1:<port>/`
through the existing transient-status mechanism (same as theme rename).

Keybinding help overlay (`?`) gains a `B — open browser view` row.

## 8. Error handling

| Failure | Behaviour |
|---|---|
| Bind to `--serve-port` fails | TUI footer shows error; controller stays in “not started”; next `B` retries |
| `webbrowser::open` fails (headless / ssh) | Treated as success; footer prints URL, user opens manually |
| Underlying file missing for `/file/...` | `ServeFile` returns 404; igv.js shows a track-load error; TUI unaffected |
| `/api/features/*` internal error | 500 with `{ "error": "..." }`; no stack/paths leaked |
| SSE client disconnect | `broadcast::Receiver` drop is silent server-side |
| Server task panics | Logged via `tracing`; controller marks handle dead; next `B` respawns |
| TUI exits while browser open | Loopback connections fail; igv.js shows network errors; TUI exits cleanly |

Server runs in `tokio::spawn`; panics never reach the TUI render loop.

## 9. Security (v1)

1. Hardcoded `bind = 127.0.0.1`. `--bind` and `--listen 0.0.0.0` are not
   accepted in v1. Exposing BAM/VCF contents to the LAN requires deliberate
   opt-in deferred to a follow-up spec.
2. File whitelist: every `/file/...` route is bound to a single absolute
   path from `ServerConfig`. No `ServeDir`, no path traversal possible.
   Index siblings (`.fai`, `.bai`, `.tbi`) are explicitly registered too.
3. `/api/jump?name=` accepts `[A-Za-z0-9_.:-]{1,64}` only; other input
   returns 400.
4. No CORS, no cookies, no upload endpoints. 100 % read-only.
5. `tracing` logs peer addr + path for diagnostics.

Out of scope: tokens, TLS, multi-tenant, remote-host workflows. Loopback
single-user is the supported model.

## 9a. Performance requirements

igv.js is a real browser viewer doing real random access into multi-hundred-MB
BAM / bigWig files, then re-fetching aggressively on every pan or zoom. The
server cannot be a toy — `python -m http.server` and similar single-threaded
blocking implementations would stall, drop ranges, and starve the browser
under realistic loads. The following constraints are non-negotiable for v1:

1. **Async, multi-threaded runtime.** axum 0.7 on the existing
   `#[tokio::main]` multi-thread runtime. Each request runs on the tokio
   thread pool; no request blocks another. No `block_on`, no synchronous
   `std::fs::read` in handlers.
2. **Streamed binary responses.** `/file/*` MUST use
   `tower_http::services::ServeFile`, which streams the response body in
   chunks and honors `Range` natively. No `Vec<u8>` buffering of full
   files in memory.
3. **Streamed SSE.** `/api/sse` uses axum's `Sse<Stream>` adapter wrapping
   a `BroadcastStream`. Events flush as they arrive; no per-request
   buffering.
4. **Bounded JSON payloads.** `/api/features/*` only ever returns records
   for a single visible window — the same `Region` bound the TUI uses.
   Worst-case payload is therefore O(records-in-window), already limited
   by igv-core's source semantics.
5. **No global locks.** `ServerState` is `Clone` and made of `Arc`s
   (`Arc<dyn FastaSource>` etc.). Handlers borrow Arcs, never hold a
   `Mutex` across `.await`.
6. **Broadcast back-pressure.** Capacity 32 on `broadcast::Sender`;
   lagged subscribers drop the oldest events silently (only the latest
   view matters).
7. **Keep-alive enabled.** axum's default HTTP/1.1 keep-alive lets igv.js
   reuse a connection across many Range fetches in one navigation —
   crucial for BAM seek-heavy workloads.

These are the same disciplines the existing async data layer in `igv-core`
already follows; the HTTP layer just must not undo them.

## 10. Out of scope for v1

These are deliberate non-goals; each becomes a separate spec if pursued:

- VCF JSON endpoint (let igv.js handle tabix in v1).
- BAM JSON / htsget endpoint.
- Remote access (`--listen 0.0.0.0`) + auth tokens.
- Browser → TUI reverse sync.
- Headless `igv-rs serve` subcommand. The `igv-serve` crate is shaped to
  enable this with a small `main.rs` addition later.
- User-provided igv.js path override.
- Multiple independent view sessions per process.
- Shared / persisted bookmarks.
- Server-side sequence or coverage rendering — igv.js handles its own.

## 11. Testing strategy

| Layer | Coverage | Approach |
|---|---|---|
| Unit | `feature_json` conversions (annotation, link) | `cargo test` with fixture records |
| Router | 404, Range, `/api/config` shape, `/api/jump` hit/miss | `tower::ServiceExt::oneshot` |
| End-to-end | Real `tokio::test` server, `reqwest` Range request, one SSE event | inside `crates/igv-serve/tests/http.rs` |
| TUI integration | `ServeController::open` is idempotent; `notify_view` dedupes | mock `ServerHandle` |
| Snapshot | `/api/config` JSON shape | `insta::assert_json_snapshot!` |
| Manual smoke | Real browser, navigate TUI, verify igv.js follows | documented checklist; not in CI |

Explicit non-tests: igv.js itself, browser rendering, SSE auto-reconnect.

## 12. Acceptance criteria

1. `igv-rs reference.fa -b sample.bam -g genes.gtf -l loops.bedpe` starts as
   before; pressing `B` opens the default browser to a local URL.
2. The browser shows reference, BAM, GTF, BEDPE tracks at the TUI's current
   region.
3. Pressing `d` / `s` / `:` in the TUI moves the browser viewport
   accordingly.
4. Annotation tracks resolve gene names via igv-core's index (the same
   `gene_name`/`gene_id`/`transcript_id` lookup that the TUI command palette
   uses).
5. BEDPE tracks honour `--link-min-score` server-side.
6. Quitting the TUI (`q` / Ctrl-C) shuts the server cleanly.
7. The release binary stays self-contained — no network access required to
   render the browser view.
8. All tests in §11 (except manual smoke) pass in CI.

## 13. References

- igv.js: https://github.com/igvteam/igv.js
- Existing snapshot design: `docs/superpowers/specs/2026-04-28-snapshot-export-design.md`
- BEDPE link design (defines the on-screen model we mirror in browser):
  `docs/superpowers/specs/2026-04-29-bedpe-link-design.md`
