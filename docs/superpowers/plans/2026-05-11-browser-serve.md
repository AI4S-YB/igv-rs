# Browser-Serve Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a `B` keystroke to the TUI that boots a local axum HTTP server, mirrors the current view in igv.js inside a real browser, and follows TUI navigation via Server-Sent Events.

**Architecture:** New crate `igv-serve` (parallel to `igv-render`) consumes `igv-core` source traits, serves binary tracks as static Range-fetched files via `tower_http`, and serves annotation/link tracks as JSON converted from `igv-core` records. `igv-tui` owns a lazy `ServeController` that calls `igv_serve::spawn` on first `B` press and pushes `ViewEvent`s through a `tokio::sync::broadcast` channel on every committed region change. igv.js itself is `include_bytes!`-embedded into the binary; the browser receives an HTML shell that boots it.

**Tech Stack:** axum 0.7, tower-http 0.5, tokio (broadcast / SSE), `webbrowser` 1.0, `serde_json`, `insta` (snapshot tests), `reqwest` 0.12 (test-only). Spec: `docs/superpowers/specs/2026-05-11-browser-serve-design.md`.

**Performance non-negotiables (see spec §9a).** igv.js bombards the server
with Range requests for hundreds of MB of BAM/bigWig data; a toy stack
will stall. Every binary-file route MUST stream via
`tower_http::services::ServeFile` (no `Vec<u8>` buffering). SSE MUST
stream via axum's `Sse<Stream>` adapter. Handlers MUST be `async fn` —
no `block_on`, no synchronous `std::fs::read`, no `Mutex` held across
`.await`. The axum server runs on the existing multi-thread tokio
runtime (`#[tokio::main]` default). HTTP/1.1 keep-alive (axum default)
must remain on so igv.js can reuse one connection across many seeks.

---

## File map

**New files**

- `crates/igv-serve/Cargo.toml`
- `crates/igv-serve/assets/igv.esm.min.js` — vendored from igv.js GitHub release
- `crates/igv-serve/src/lib.rs` — public API
- `crates/igv-serve/src/state.rs` — `ServerState`
- `crates/igv-serve/src/view.rs` — `ViewEvent`, `ViewSnapshot`
- `crates/igv-serve/src/error.rs` — `ServeError`
- `crates/igv-serve/src/feature_json.rs` — record → igv.js JSON conversions
- `crates/igv-serve/src/routes/mod.rs`
- `crates/igv-serve/src/routes/index.rs` — `GET /`
- `crates/igv-serve/src/routes/assets.rs` — `GET /assets/igv.esm.min.js`
- `crates/igv-serve/src/routes/config.rs` — `GET /api/config`
- `crates/igv-serve/src/routes/file.rs` — `GET /file/*`
- `crates/igv-serve/src/routes/features.rs` — `GET /api/features/*`
- `crates/igv-serve/src/routes/jump.rs` — `GET /api/jump`
- `crates/igv-serve/src/routes/sse.rs` — `GET /api/sse`
- `crates/igv-serve/tests/http.rs` — integration suite
- `crates/igv-tui/src/app/serve.rs` — `ServeController`
- `scripts/update-igvjs.sh` — refresh the vendored asset

**Modified files**

- `Cargo.toml` — add `igv-serve` to workspace members + dependency entries
- `crates/igv-core/src/source/annotation.rs` — add `find_by_name_union` free function
- `crates/igv-tui/Cargo.toml` — depend on `igv-serve` + `webbrowser`
- `crates/igv-tui/src/app/action.rs` — `Action::OpenBrowser`
- `crates/igv-tui/src/app/mod.rs` — re-export `serve` module
- `crates/igv-tui/src/app/state.rs` — replace `find_gene_region` body with the lifted function call
- `crates/igv-tui/src/input.rs` — bind `B` → `Action::OpenBrowser`
- `crates/igv-tui/src/cli.rs` — `--no-browser`, `--serve-port`
- `crates/igv-tui/src/main.rs` — instantiate `ServeController`, wire `notify_view` at the loaded-count==expected hook (line 410), handle `Action::OpenBrowser`
- `crates/igv-tui/src/ui/widgets/doc.rs` — help overlay row for `B`
- `crates/igv-tui/src/ui/theme.rs` (or wherever `[theme]` is parsed) — add `[serve]` table
- `README.md` — new "Browser view" section

---

### Task 1: Vendor igv.js asset and scaffold the `igv-serve` crate

**Files:**
- Create: `crates/igv-serve/Cargo.toml`
- Create: `crates/igv-serve/src/lib.rs`
- Create: `crates/igv-serve/assets/igv.esm.min.js`
- Create: `scripts/update-igvjs.sh`
- Modify: `Cargo.toml` (workspace root)

- [ ] **Step 1: Add the new crate to the workspace members**

Edit `/home/xzg/project/igv_rs/Cargo.toml`:

```toml
[workspace]
resolver = "2"
members = ["crates/igv-core", "crates/igv-render", "crates/igv-serve", "crates/igv-tui"]
```

- [ ] **Step 2: Add workspace-level deps for the new crate**

Append to `[workspace.dependencies]` in the same file:

```toml
axum = { version = "0.7", default-features = false, features = ["http1", "tokio", "json", "query"] }
tower-http = { version = "0.5", default-features = false, features = ["fs"] }
tokio-stream = { version = "0.1", default-features = false }
webbrowser = "1"
mime_guess = "2"
serde_json = "1"
reqwest = { version = "0.12", default-features = false, features = ["rustls-tls", "stream"] }
```

- [ ] **Step 3: Write the crate manifest**

Create `/home/xzg/project/igv_rs/crates/igv-serve/Cargo.toml`:

```toml
[package]
name = "igv-serve"
version.workspace = true
edition.workspace = true
license.workspace = true
authors.workspace = true
rust-version.workspace = true

[dependencies]
igv-core = { path = "../igv-core" }
axum.workspace = true
tower-http.workspace = true
tokio = { workspace = true, features = ["sync", "macros", "rt", "rt-multi-thread", "net", "fs"] }
tokio-stream.workspace = true
async-trait.workspace = true
futures.workspace = true
serde = { workspace = true, features = ["derive"] }
serde_json.workspace = true
thiserror.workspace = true
anyhow.workspace = true
tracing.workspace = true
mime_guess.workspace = true

[dev-dependencies]
tokio = { workspace = true, features = ["test-util"] }
reqwest.workspace = true
tempfile.workspace = true
insta = { workspace = true, features = ["json"] }
```

- [ ] **Step 4: Create a minimal lib.rs to compile**

Create `/home/xzg/project/igv_rs/crates/igv-serve/src/lib.rs`:

```rust
//! Local HTTP server that mirrors the TUI's view in igv.js.
//!
//! See `docs/superpowers/specs/2026-05-11-browser-serve-design.md`.

#![forbid(unsafe_code)]
#![warn(rust_2018_idioms)]
```

- [ ] **Step 5: Vendor `igv.esm.min.js`**

Download the released ESM bundle from the upstream tag and place it at
`/home/xzg/project/igv_rs/crates/igv-serve/assets/igv.esm.min.js`. Until
later tasks reference it, an empty placeholder file is sufficient so the
crate builds; replace it with the real asset before Task 4.

```bash
mkdir -p /home/xzg/project/igv_rs/crates/igv-serve/assets
curl -sSL \
  https://cdn.jsdelivr.net/npm/igv@3.0.5/dist/igv.esm.min.js \
  -o /home/xzg/project/igv_rs/crates/igv-serve/assets/igv.esm.min.js
test -s /home/xzg/project/igv_rs/crates/igv-serve/assets/igv.esm.min.js
```

Pin the version inside `scripts/update-igvjs.sh`:

```bash
#!/usr/bin/env bash
# Refresh the vendored igv.js asset. Bump IGVJS_VERSION when upgrading.
set -euo pipefail
IGVJS_VERSION="${IGVJS_VERSION:-3.0.5}"
DEST="$(dirname "$0")/../crates/igv-serve/assets/igv.esm.min.js"
curl -sSL \
  "https://cdn.jsdelivr.net/npm/igv@${IGVJS_VERSION}/dist/igv.esm.min.js" \
  -o "$DEST"
echo "wrote $DEST (igv.js ${IGVJS_VERSION}, $(wc -c <"$DEST") bytes)"
```

```bash
chmod +x /home/xzg/project/igv_rs/scripts/update-igvjs.sh
```

- [ ] **Step 6: Verify the crate compiles**

Run: `cargo check -p igv-serve`
Expected: success, no warnings.

- [ ] **Step 7: Commit**

```bash
git add Cargo.toml crates/igv-serve scripts/update-igvjs.sh
git commit -m "feat(serve): scaffold igv-serve crate and vendor igv.js"
```

---

### Task 2: Lift `find_gene_region` into `igv-core`

The TUI palette already does multi-isoform gene-name → region resolution
inside `AppState::find_gene_region`. The HTTP `/api/jump` route needs the
same behaviour but does not have an `AppState`. Extract the logic into a
free function in `igv-core` and have the TUI call it.

**Files:**
- Modify: `crates/igv-core/src/source/annotation.rs`
- Modify: `crates/igv-tui/src/app/state.rs:478-515`
- Test: `crates/igv-core/src/source/annotation.rs` (inline `#[cfg(test)]`)

- [ ] **Step 1: Write the failing test**

Append to the existing `#[cfg(test)] mod tests` in
`crates/igv-core/src/source/annotation.rs`:

```rust
use crate::region::Region;
use async_trait::async_trait;
use std::sync::Arc;

struct StubSource {
    name: String,
    rows: Vec<(String, AnnotationTranscript)>, // (chrom, transcript)
}

#[async_trait]
impl AnnotationSource for StubSource {
    async fn fetch(&self, _region: &Region) -> crate::Result<Vec<AnnotationTranscript>> {
        Ok(Vec::new())
    }
    fn display_name(&self) -> &str {
        &self.name
    }
    fn find_by_name(&self, query: &str) -> Vec<(String, AnnotationTranscript)> {
        let q = query.to_ascii_lowercase();
        self.rows
            .iter()
            .filter(|(_, tx)| tx.name.to_ascii_lowercase() == q)
            .cloned()
            .collect()
    }
}

fn tx(name: &str, blocks: &[(u64, u64)]) -> AnnotationTranscript {
    AnnotationTranscript {
        id: name.into(),
        name: name.into(),
        gene_id: None,
        kind: TranscriptKind::Other,
        strand: Strand::Forward,
        blocks: blocks
            .iter()
            .map(|(s, e)| AnnotationBlock {
                start: *s,
                end: *e,
                kind: BlockKind::Exon,
            })
            .collect(),
    }
}

#[test]
fn find_by_name_union_unions_isoforms_on_same_chrom() {
    let src: Arc<dyn AnnotationSource> = Arc::new(StubSource {
        name: "stub".into(),
        rows: vec![
            ("chr1".into(), tx("BRCA1", &[(1000, 2000)])),
            ("chr1".into(), tx("BRCA1", &[(1500, 3000)])),
        ],
    });
    let (region, label) = find_by_name_union(&[src], "brca1").unwrap();
    assert_eq!(region.chrom, "chr1");
    assert_eq!(region.start, 1000);
    assert_eq!(region.end, 3000);
    assert_eq!(label, "BRCA1");
}

#[test]
fn find_by_name_union_misses_return_none() {
    let src: Arc<dyn AnnotationSource> = Arc::new(StubSource {
        name: "stub".into(),
        rows: vec![],
    });
    assert!(find_by_name_union(&[src], "xyz").is_none());
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test -p igv-core find_by_name_union -- --nocapture`
Expected: FAIL with "cannot find function `find_by_name_union`".

- [ ] **Step 3: Implement the function**

Append to `crates/igv-core/src/source/annotation.rs` (above the
`#[cfg(test)]` block):

```rust
/// Multi-track gene-name → region resolver used by both the TUI command
/// palette and the HTTP `/api/jump` endpoint. Returns the union span of
/// all transcripts matching `query` on the first chromosome seen, plus a
/// display label (the first matched transcript's `name`).
pub fn find_by_name_union(
    sources: &[std::sync::Arc<dyn AnnotationSource>],
    query: &str,
) -> Option<(crate::region::Region, String)> {
    if query.is_empty() {
        return None;
    }
    let mut chrom: Option<String> = None;
    let mut span: Option<(u64, u64)> = None;
    let mut label: Option<String> = None;
    for src in sources {
        for (c, tx) in src.find_by_name(query) {
            let Some((s, e)) = tx.span() else { continue };
            match &chrom {
                None => {
                    chrom = Some(c);
                    span = Some((s, e));
                    label = Some(tx.name.clone());
                }
                Some(existing) if existing == &c => {
                    let (cs, ce) = span.unwrap();
                    span = Some((cs.min(s), ce.max(e)));
                }
                Some(_) => {}
            }
        }
    }
    let chrom = chrom?;
    let (s, e) = span?;
    let region = crate::region::Region::new(chrom, s, e).ok()?;
    Some((region, label.unwrap_or_else(|| query.to_string())))
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p igv-core find_by_name_union`
Expected: 2 tests pass.

- [ ] **Step 5: Replace the TUI duplicate**

Replace the body of `AppState::find_gene_region` in
`crates/igv-tui/src/app/state.rs` (lines 483-515) with a call to the new
function. The signatures stay the same; just delegate:

```rust
fn find_gene_region(&self, query: &str) -> Option<(Region, String)> {
    let sources: Vec<_> = self.annotations.iter().map(|t| t.source.clone()).collect();
    let (region, label) = igv_core::source::annotation::find_by_name_union(&sources, query)?;
    let status = format!("{label} ({region})");
    Some((region, status))
}
```

- [ ] **Step 6: Verify the TUI tests still pass**

Run: `cargo test -p igv-tui`
Expected: all pre-existing tests still pass.

- [ ] **Step 7: Commit**

```bash
git add crates/igv-core/src/source/annotation.rs crates/igv-tui/src/app/state.rs
git commit -m "refactor(core): lift find_by_name_union out of TUI for reuse"
```

---

### Task 3: Public types — `ViewEvent`, `ViewSnapshot`, `TrackEntry`, `ServerConfig`, `ServeError`

**Files:**
- Create: `crates/igv-serve/src/view.rs`
- Create: `crates/igv-serve/src/error.rs`
- Create: `crates/igv-serve/src/state.rs`
- Modify: `crates/igv-serve/src/lib.rs`
- Test: same files (`#[cfg(test)]` inline)

- [ ] **Step 1: Write the failing test**

Create `crates/igv-serve/src/view.rs`:

```rust
//! View event types pushed from TUI to browser, and the initial config
//! snapshot used to build `/api/config`.

use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ViewEvent {
    pub chrom: String,
    pub start: u64,
    pub end: u64,
}

#[derive(Debug, Clone)]
pub struct ViewSnapshot {
    pub initial: ViewEvent,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn view_event_serializes_to_expected_keys() {
        let ev = ViewEvent {
            chrom: "chr1".into(),
            start: 1000,
            end: 2000,
        };
        let s = serde_json::to_string(&ev).unwrap();
        assert_eq!(s, r#"{"chrom":"chr1","start":1000,"end":2000}"#);
    }
}
```

- [ ] **Step 2: Create the error type**

Create `crates/igv-serve/src/error.rs`:

```rust
use std::io;

#[derive(Debug, thiserror::Error)]
pub enum ServeError {
    #[error("failed to bind to {addr}: {source}")]
    BindFailed {
        addr: std::net::SocketAddr,
        #[source]
        source: io::Error,
    },
    #[error("missing index sibling for {path}")]
    MissingIndex { path: std::path::PathBuf },
    #[error("io error: {0}")]
    Io(#[from] io::Error),
}
```

- [ ] **Step 3: Create `ServerState` and `TrackEntry`**

Create `crates/igv-serve/src/state.rs`:

```rust
use std::path::PathBuf;
use std::sync::Arc;

use tokio::sync::broadcast;

use igv_core::source::{
    AnnotationSource, BamSource, FastaSource, LinkSource, SignalSource, VcfSource,
};

use crate::view::ViewEvent;

#[derive(Debug, Clone)]
pub struct TrackEntry<S: ?Sized> {
    pub source: Arc<S>,
    pub path: PathBuf,
    pub display: String,
}

#[derive(Clone)]
pub struct ServerState {
    pub fasta: Arc<dyn FastaSource>,
    pub fasta_path: PathBuf,
    pub bams: Vec<TrackEntry<dyn BamSource>>,
    pub vcfs: Vec<TrackEntry<dyn VcfSource>>,
    pub annotations: Vec<TrackEntry<dyn AnnotationSource>>,
    pub signals: Vec<TrackEntry<dyn SignalSource>>,
    pub links: Vec<TrackEntry<dyn LinkSource>>,
    pub initial: ViewEvent,
    pub link_min_score: Option<f64>,
    pub events: broadcast::Sender<ViewEvent>,
}

impl std::fmt::Debug for ServerState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ServerState")
            .field("fasta_path", &self.fasta_path)
            .field("bams", &self.bams.len())
            .field("vcfs", &self.vcfs.len())
            .field("annotations", &self.annotations.len())
            .field("signals", &self.signals.len())
            .field("links", &self.links.len())
            .field("initial", &self.initial)
            .field("link_min_score", &self.link_min_score)
            .finish()
    }
}
```

- [ ] **Step 4: Wire the modules into `lib.rs`**

Replace `crates/igv-serve/src/lib.rs` with:

```rust
//! Local HTTP server that mirrors the TUI's view in igv.js.
//!
//! See `docs/superpowers/specs/2026-05-11-browser-serve-design.md`.

#![forbid(unsafe_code)]
#![warn(rust_2018_idioms)]

pub mod error;
pub mod state;
pub mod view;

pub use error::ServeError;
pub use state::{ServerState, TrackEntry};
pub use view::{ViewEvent, ViewSnapshot};
```

- [ ] **Step 5: Run the test**

Run: `cargo test -p igv-serve`
Expected: 1 test passes.

- [ ] **Step 6: Commit**

```bash
git add crates/igv-serve/src
git commit -m "feat(serve): public types (ViewEvent, ServerState, ServeError)"
```

---

### Task 4: HTTP scaffold — `ServerConfig`, `spawn`, `shutdown`, `GET /`, `GET /assets/igv.esm.min.js`

**Files:**
- Create: `crates/igv-serve/src/routes/mod.rs`
- Create: `crates/igv-serve/src/routes/index.rs`
- Create: `crates/igv-serve/src/routes/assets.rs`
- Modify: `crates/igv-serve/src/lib.rs`
- Create: `crates/igv-serve/tests/http.rs`

- [ ] **Step 1: Write the failing integration test**

Create `crates/igv-serve/tests/http.rs`:

```rust
use std::path::PathBuf;
use std::sync::Arc;

use igv_serve::{spawn, ServerConfig, ViewEvent};

async fn empty_config() -> ServerConfig {
    // Build the smallest valid ServerConfig: a fasta source from a
    // temporary empty file. The integration suite doesn't actually
    // fetch anything from the source; it just needs the Arc to exist.
    let dir = tempfile::tempdir().unwrap();
    let fasta_path = dir.path().join("ref.fa");
    std::fs::write(&fasta_path, b">chr1\nACGT\n").unwrap();
    std::fs::write(dir.path().join("ref.fa.fai"), b"chr1\t4\t6\t4\t5\n").unwrap();
    let fasta = igv_core::source::NoodlesFastaSource::open(&fasta_path)
        .await
        .unwrap();
    let cfg = ServerConfig {
        bind: std::net::IpAddr::from([127, 0, 0, 1]),
        port: 0,
        fasta: Arc::new(fasta),
        fasta_path,
        bams: vec![],
        vcfs: vec![],
        annotations: vec![],
        signals: vec![],
        links: vec![],
        initial: ViewEvent { chrom: "chr1".into(), start: 0, end: 4 },
        link_min_score: None,
    };
    std::mem::forget(dir); // keep files alive for the duration of the test
    cfg
}

#[tokio::test]
async fn root_serves_html_with_igv_module() {
    let h = spawn(empty_config().await).await.unwrap();
    let body = reqwest::get(format!("http://{}/", h.addr))
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(body.contains("/assets/igv.esm.min.js"));
    h.shutdown().await;
}

#[tokio::test]
async fn assets_serves_igvjs() {
    let h = spawn(empty_config().await).await.unwrap();
    let resp = reqwest::get(format!("http://{}/assets/igv.esm.min.js", h.addr))
        .await
        .unwrap();
    assert!(resp.status().is_success());
    let ct = resp.headers().get("content-type").unwrap().to_str().unwrap();
    assert!(ct.starts_with("application/javascript"));
    assert!(resp.bytes().await.unwrap().len() > 1000);
    h.shutdown().await;
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test -p igv-serve --test http`
Expected: FAIL with "cannot find function `spawn`" or "cannot find type `ServerConfig`".

- [ ] **Step 3: Implement `ServerConfig`, `spawn`, `ServerHandle`, `shutdown`**

Replace `crates/igv-serve/src/lib.rs` with:

```rust
//! Local HTTP server that mirrors the TUI's view in igv.js.
//!
//! See `docs/superpowers/specs/2026-05-11-browser-serve-design.md`.

#![forbid(unsafe_code)]
#![warn(rust_2018_idioms)]

use std::net::{IpAddr, SocketAddr};
use std::path::PathBuf;
use std::sync::Arc;

use tokio::sync::{broadcast, oneshot};
use tokio::task::JoinHandle;

use igv_core::source::{
    AnnotationSource, BamSource, FastaSource, LinkSource, SignalSource, VcfSource,
};

pub mod error;
pub mod routes;
pub mod state;
pub mod view;

pub use error::ServeError;
pub use state::{ServerState, TrackEntry};
pub use view::{ViewEvent, ViewSnapshot};

pub struct ServerConfig {
    pub bind: IpAddr,
    pub port: u16,
    pub fasta: Arc<dyn FastaSource>,
    pub fasta_path: PathBuf,
    pub bams: Vec<TrackEntry<dyn BamSource>>,
    pub vcfs: Vec<TrackEntry<dyn VcfSource>>,
    pub annotations: Vec<TrackEntry<dyn AnnotationSource>>,
    pub signals: Vec<TrackEntry<dyn SignalSource>>,
    pub links: Vec<TrackEntry<dyn LinkSource>>,
    pub initial: ViewEvent,
    pub link_min_score: Option<f64>,
}

#[derive(Debug)]
pub struct ServerHandle {
    pub addr: SocketAddr,
    pub events: broadcast::Sender<ViewEvent>,
    join: JoinHandle<()>,
    shutdown: Option<oneshot::Sender<()>>,
}

impl ServerHandle {
    pub fn push_view(&self, ev: ViewEvent) {
        let _ = self.events.send(ev);
    }
    pub async fn shutdown(mut self) {
        if let Some(tx) = self.shutdown.take() {
            let _ = tx.send(());
        }
        let _ = self.join.await;
    }
}

pub async fn spawn(cfg: ServerConfig) -> Result<ServerHandle, ServeError> {
    let (events, _rx) = broadcast::channel::<ViewEvent>(32);
    let state = ServerState {
        fasta: cfg.fasta,
        fasta_path: cfg.fasta_path,
        bams: cfg.bams,
        vcfs: cfg.vcfs,
        annotations: cfg.annotations,
        signals: cfg.signals,
        links: cfg.links,
        initial: cfg.initial,
        link_min_score: cfg.link_min_score,
        events: events.clone(),
    };

    let router = routes::build(state);
    let bind_addr = SocketAddr::new(cfg.bind, cfg.port);
    let listener = tokio::net::TcpListener::bind(bind_addr).await
        .map_err(|source| ServeError::BindFailed { addr: bind_addr, source })?;
    let addr = listener.local_addr()?;
    let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();
    let join = tokio::spawn(async move {
        let server = axum::serve(listener, router)
            .with_graceful_shutdown(async {
                let _ = shutdown_rx.await;
            });
        if let Err(err) = server.await {
            tracing::error!(?err, "igv-serve axum task ended with error");
        }
    });
    Ok(ServerHandle { addr, events, join, shutdown: Some(shutdown_tx) })
}
```

- [ ] **Step 4: Create the router scaffold**

Create `crates/igv-serve/src/routes/mod.rs`:

```rust
use axum::Router;

use crate::state::ServerState;

pub mod assets;
pub mod index;

pub fn build(state: ServerState) -> Router {
    Router::new()
        .merge(index::router())
        .merge(assets::router())
        .with_state(state)
}
```

- [ ] **Step 5: Create the `/` route**

Create `crates/igv-serve/src/routes/index.rs`:

```rust
use axum::{response::Html, routing::get, Router};

use crate::state::ServerState;

pub fn router() -> Router<ServerState> {
    Router::new().route("/", get(handler))
}

async fn handler() -> Html<&'static str> {
    Html(include_str!("../../assets/index.html"))
}
```

Create the HTML template at
`crates/igv-serve/assets/index.html`:

```html
<!doctype html>
<html lang="en">
  <head>
    <meta charset="utf-8">
    <title>igv-rs browser view</title>
    <style>
      html, body { margin: 0; height: 100%; background: #fff; font-family: system-ui, sans-serif; }
      #igv-root { height: 100vh; }
    </style>
  </head>
  <body>
    <div id="igv-root"></div>
    <script type="module">
      import igv from "/assets/igv.esm.min.js";
      const cfg = await fetch("/api/config").then(r => r.json());
      const browser = await igv.createBrowser(document.getElementById("igv-root"), cfg);
      const es = new EventSource("/api/sse");
      es.addEventListener("view", e => {
        const { chrom, start, end } = JSON.parse(e.data);
        browser.search(`${chrom}:${start}-${end}`);
      });
    </script>
  </body>
</html>
```

- [ ] **Step 6: Create the `/assets/igv.esm.min.js` route**

Create `crates/igv-serve/src/routes/assets.rs`:

```rust
use axum::{
    http::header,
    response::{IntoResponse, Response},
    routing::get,
    Router,
};

use crate::state::ServerState;

const IGV_JS: &[u8] = include_bytes!("../../assets/igv.esm.min.js");

pub fn router() -> Router<ServerState> {
    Router::new().route("/assets/igv.esm.min.js", get(igvjs))
}

async fn igvjs() -> Response {
    (
        [
            (header::CONTENT_TYPE, "application/javascript; charset=utf-8"),
            (header::CACHE_CONTROL, "public, max-age=31536000, immutable"),
        ],
        IGV_JS,
    )
        .into_response()
}
```

- [ ] **Step 7: Run the tests to verify they pass**

Run: `cargo test -p igv-serve --test http`
Expected: 2 tests pass.

- [ ] **Step 8: Commit**

```bash
git add crates/igv-serve
git commit -m "feat(serve): axum scaffold with / + /assets/igv.esm.min.js"
```

---

### Task 5: `GET /api/config`

**Files:**
- Create: `crates/igv-serve/src/routes/config.rs`
- Modify: `crates/igv-serve/src/routes/mod.rs`
- Test: `crates/igv-serve/tests/http.rs`

- [ ] **Step 1: Write the failing test**

Append to `crates/igv-serve/tests/http.rs`:

```rust
#[tokio::test]
async fn api_config_emits_reference_and_locus() {
    let h = spawn(empty_config().await).await.unwrap();
    let body: serde_json::Value = reqwest::get(format!("http://{}/api/config", h.addr))
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(body["reference"]["fastaURL"], "/file/fasta");
    assert_eq!(body["reference"]["indexURL"], "/file/fasta.fai");
    assert_eq!(body["locus"], "chr1:0-4");
    assert!(body["tracks"].as_array().unwrap().is_empty());
    h.shutdown().await;
}
```

- [ ] **Step 2: Run it to verify it fails**

Run: `cargo test -p igv-serve --test http api_config_emits_reference`
Expected: FAIL with 404 / no route.

- [ ] **Step 3: Implement the route**

Create `crates/igv-serve/src/routes/config.rs`:

```rust
use axum::{extract::State, response::Json, routing::get, Router};
use serde_json::{json, Value};

use crate::state::ServerState;

pub fn router() -> Router<ServerState> {
    Router::new().route("/api/config", get(handler))
}

async fn handler(State(state): State<ServerState>) -> Json<Value> {
    let mut tracks: Vec<Value> = Vec::new();

    for (idx, t) in state.bams.iter().enumerate() {
        tracks.push(json!({
            "name": t.display,
            "type": "alignment",
            "format": "bam",
            "url": format!("/file/bam/{idx}"),
            "indexURL": format!("/file/bam/{idx}.bai"),
        }));
    }
    for (idx, t) in state.vcfs.iter().enumerate() {
        tracks.push(json!({
            "name": t.display,
            "type": "variant",
            "format": "vcf",
            "url": format!("/file/vcf/{idx}"),
            "indexURL": format!("/file/vcf/{idx}.tbi"),
        }));
    }
    for (idx, t) in state.signals.iter().enumerate() {
        tracks.push(json!({
            "name": t.display,
            "type": "wig",
            "format": "bigwig",
            "url": format!("/file/signal/{idx}"),
        }));
    }
    for (idx, t) in state.annotations.iter().enumerate() {
        tracks.push(json!({
            "name": t.display,
            "type": "annotation",
            "sourceType": "custom",
            "source": {
                "url": format!("/api/features/annotation/{idx}"),
                "queryable": true,
            }
        }));
    }
    for (idx, t) in state.links.iter().enumerate() {
        tracks.push(json!({
            "name": t.display,
            "type": "interact",
            "sourceType": "custom",
            "source": {
                "url": format!("/api/features/link/{idx}"),
                "queryable": true,
            }
        }));
    }

    Json(json!({
        "reference": {
            "id": "user-fasta",
            "name": "Reference",
            "fastaURL": "/file/fasta",
            "indexURL": "/file/fasta.fai",
            "wholeGenomeView": false,
        },
        "locus": format!("{}:{}-{}", state.initial.chrom, state.initial.start, state.initial.end),
        "tracks": tracks,
    }))
}
```

- [ ] **Step 4: Wire into `routes::build`**

Edit `crates/igv-serve/src/routes/mod.rs`:

```rust
pub mod assets;
pub mod config;
pub mod index;

pub fn build(state: ServerState) -> Router {
    Router::new()
        .merge(index::router())
        .merge(assets::router())
        .merge(config::router())
        .with_state(state)
}
```

- [ ] **Step 5: Run the test**

Run: `cargo test -p igv-serve --test http`
Expected: 3 tests pass.

- [ ] **Step 6: Commit**

```bash
git add crates/igv-serve/src/routes
git commit -m "feat(serve): /api/config emits igv.js browser config"
```

---

### Task 6: `GET /file/*` — Range-served static files

**Files:**
- Create: `crates/igv-serve/src/routes/file.rs`
- Modify: `crates/igv-serve/src/routes/mod.rs`
- Test: `crates/igv-serve/tests/http.rs`

The implementation reads the underlying file straight from disk (not
through `igv-core::source`), because igv.js does its own random access
and only needs raw bytes + `Range` semantics.

- [ ] **Step 1: Write the failing test**

Append to `crates/igv-serve/tests/http.rs`:

```rust
#[tokio::test]
async fn file_fasta_supports_range() {
    let h = spawn(empty_config().await).await.unwrap();
    // The empty_config fixture writes ">chr1\nACGT\n" (11 bytes) at /file/fasta.
    // Request a 4-byte range starting at offset 6 — that's "ACGT".
    let resp = reqwest::Client::new()
        .get(format!("http://{}/file/fasta", h.addr))
        .header("Range", "bytes=6-9")
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), reqwest::StatusCode::PARTIAL_CONTENT);
    let body = resp.bytes().await.unwrap();
    assert_eq!(&body[..], b"ACGT");
    h.shutdown().await;
}

#[tokio::test]
async fn file_fasta_index_returns_200() {
    let h = spawn(empty_config().await).await.unwrap();
    let resp = reqwest::get(format!("http://{}/file/fasta.fai", h.addr))
        .await
        .unwrap();
    assert!(resp.status().is_success());
    assert!(resp.bytes().await.unwrap().starts_with(b"chr1\t4"));
    h.shutdown().await;
}

#[tokio::test]
async fn file_unknown_kind_returns_404() {
    let h = spawn(empty_config().await).await.unwrap();
    let resp = reqwest::get(format!("http://{}/file/bam/0", h.addr))
        .await
        .unwrap();
    assert_eq!(resp.status(), reqwest::StatusCode::NOT_FOUND);
    h.shutdown().await;
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p igv-serve --test http file_`
Expected: FAIL (404 from the missing routes).

- [ ] **Step 3: Implement the route**

Create `crates/igv-serve/src/routes/file.rs`:

```rust
use std::path::{Path, PathBuf};

use axum::{
    extract::{Path as AxumPath, State},
    http::{Request, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
    Router,
};
use tower::ServiceExt; // for `.oneshot`
use tower_http::services::ServeFile;

use crate::state::ServerState;

pub fn router() -> Router<ServerState> {
    Router::new()
        .route("/file/fasta", get(serve_fasta))
        .route("/file/fasta.fai", get(serve_fasta_fai))
        .route("/file/bam/:idx", get(serve_bam))
        .route("/file/bam/:idx.bai", get(serve_bam_bai))
        .route("/file/vcf/:idx", get(serve_vcf))
        .route("/file/vcf/:idx.tbi", get(serve_vcf_tbi))
        .route("/file/signal/:idx", get(serve_signal))
}

async fn serve_path(path: PathBuf, req: Request<axum::body::Body>) -> Response {
    if !path.exists() {
        return (StatusCode::NOT_FOUND, "not found").into_response();
    }
    match ServeFile::new(&path).oneshot(req).await {
        Ok(resp) => resp.into_response(),
        Err(err) => {
            tracing::error!(?err, ?path, "ServeFile error");
            (StatusCode::INTERNAL_SERVER_ERROR, "io error").into_response()
        }
    }
}

fn sibling(path: &Path, ext: &str) -> PathBuf {
    let mut s = path.as_os_str().to_owned();
    s.push(format!(".{ext}"));
    PathBuf::from(s)
}

async fn serve_fasta(State(s): State<ServerState>, req: Request<axum::body::Body>) -> Response {
    serve_path(s.fasta_path.clone(), req).await
}
async fn serve_fasta_fai(State(s): State<ServerState>, req: Request<axum::body::Body>) -> Response {
    serve_path(sibling(&s.fasta_path, "fai"), req).await
}

async fn serve_bam(
    State(s): State<ServerState>,
    AxumPath(idx): AxumPath<usize>,
    req: Request<axum::body::Body>,
) -> Response {
    let Some(t) = s.bams.get(idx) else {
        return (StatusCode::NOT_FOUND, "no such bam").into_response();
    };
    serve_path(t.path.clone(), req).await
}
async fn serve_bam_bai(
    State(s): State<ServerState>,
    AxumPath(idx): AxumPath<usize>,
    req: Request<axum::body::Body>,
) -> Response {
    let Some(t) = s.bams.get(idx) else {
        return (StatusCode::NOT_FOUND, "no such bam").into_response();
    };
    serve_path(sibling(&t.path, "bai"), req).await
}

async fn serve_vcf(
    State(s): State<ServerState>,
    AxumPath(idx): AxumPath<usize>,
    req: Request<axum::body::Body>,
) -> Response {
    let Some(t) = s.vcfs.get(idx) else {
        return (StatusCode::NOT_FOUND, "no such vcf").into_response();
    };
    serve_path(t.path.clone(), req).await
}
async fn serve_vcf_tbi(
    State(s): State<ServerState>,
    AxumPath(idx): AxumPath<usize>,
    req: Request<axum::body::Body>,
) -> Response {
    let Some(t) = s.vcfs.get(idx) else {
        return (StatusCode::NOT_FOUND, "no such vcf").into_response();
    };
    serve_path(sibling(&t.path, "tbi"), req).await
}

async fn serve_signal(
    State(s): State<ServerState>,
    AxumPath(idx): AxumPath<usize>,
    req: Request<axum::body::Body>,
) -> Response {
    let Some(t) = s.signals.get(idx) else {
        return (StatusCode::NOT_FOUND, "no such signal").into_response();
    };
    serve_path(t.path.clone(), req).await
}
```

Add `tower` to `crates/igv-serve/Cargo.toml` `[dependencies]`:

```toml
tower = { version = "0.5", features = ["util"] }
```

- [ ] **Step 4: Wire into `routes::build`**

Add `pub mod file;` and `.merge(file::router())` in
`crates/igv-serve/src/routes/mod.rs`.

- [ ] **Step 5: Run the tests**

Run: `cargo test -p igv-serve --test http file_`
Expected: 3 tests pass.

- [ ] **Step 6: Commit**

```bash
git add crates/igv-serve
git commit -m "feat(serve): /file/* routes with Range support via ServeFile"
```

---

### Task 7: `feature_json::annotation` + `GET /api/features/annotation/{idx}`

**Files:**
- Create: `crates/igv-serve/src/feature_json.rs`
- Create: `crates/igv-serve/src/routes/features.rs`
- Modify: `crates/igv-serve/src/lib.rs`, `routes/mod.rs`
- Test: inline in `feature_json.rs` + `tests/http.rs`

- [ ] **Step 1: Write the failing unit test**

Create `crates/igv-serve/src/feature_json.rs`:

```rust
//! Conversions from igv-core record types to the JSON shapes igv.js
//! expects for custom feature sources.

use serde_json::{json, Value};

use igv_core::source::{
    AnnotationBlock, AnnotationTranscript, BlockKind, LinkRecord, LinkScope, Strand,
};

pub fn annotation_to_json(chrom: &str, tx: &AnnotationTranscript) -> Value {
    let (start, end) = tx.span().unwrap_or((0, 0));
    let exons: Vec<Value> = tx
        .blocks
        .iter()
        .filter(|b| matches!(b.kind, BlockKind::Exon | BlockKind::Cds))
        .map(|b| json!({ "start": b.start, "end": b.end }))
        .collect();
    json!({
        "chr": chrom,
        "start": start,
        "end": end,
        "name": tx.name,
        "strand": match tx.strand {
            Strand::Forward => "+",
            Strand::Reverse => "-",
            Strand::Unknown => ".",
        },
        "type": "transcript",
        "exons": exons,
    })
}

pub fn link_to_json(rec: &LinkRecord) -> Value {
    let (chr1, s1, e1, chr2, s2, e2) = match &rec.scope {
        LinkScope::BothIn { chrom, a, b } => (
            chrom.clone(), a.start, a.end, chrom.clone(), b.start, b.end,
        ),
        LinkScope::PartialCis { chrom, anchor, partner } => (
            chrom.clone(),
            anchor.start,
            anchor.end,
            chrom.clone(),
            partner.start,
            partner.end,
        ),
        LinkScope::Trans { left, right } => (
            left.chrom.clone(),
            left.start,
            left.end,
            right.chrom.clone(),
            right.start,
            right.end,
        ),
    };
    let mut v = json!({
        "chr1": chr1, "start1": s1, "end1": e1,
        "chr2": chr2, "start2": s2, "end2": e2,
    });
    if let Some(score) = rec.score {
        v["score"] = json!(score);
        v["value"] = json!(score);
    }
    if let Some(name) = &rec.name {
        v["name"] = json!(name);
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tx() -> AnnotationTranscript {
        AnnotationTranscript {
            id: "tx1".into(),
            name: "BRCA1".into(),
            gene_id: None,
            kind: igv_core::source::TranscriptKind::Other,
            strand: Strand::Forward,
            blocks: vec![
                AnnotationBlock { start: 100, end: 200, kind: BlockKind::Exon },
                AnnotationBlock { start: 300, end: 400, kind: BlockKind::Exon },
            ],
        }
    }

    #[test]
    fn annotation_to_json_includes_chr_span_strand_exons() {
        let v = annotation_to_json("chr1", &tx());
        assert_eq!(v["chr"], "chr1");
        assert_eq!(v["start"], 100);
        assert_eq!(v["end"], 400);
        assert_eq!(v["strand"], "+");
        assert_eq!(v["exons"].as_array().unwrap().len(), 2);
    }
}
```

(The exact field names of `LinkRecord`/`LinkScope` are taken from
`crates/igv-core/src/source/link.rs`. If a field name differs, replicate
the actual on-disk type — do not invent fields.)

- [ ] **Step 2: Verify the unit tests pass**

Run: `cargo test -p igv-serve feature_json`
Expected: 1 test passes.

- [ ] **Step 3: Write the failing integration test for `/api/features/annotation/{idx}`**

Add to `crates/igv-serve/tests/http.rs` a new helper that builds a config
with one annotation track loaded from a tiny BED file, then queries the
endpoint. Place this helper next to `empty_config`:

```rust
async fn config_with_bed(bed_body: &str) -> (ServerConfig, tempfile::TempDir) {
    let mut cfg = empty_config().await;
    let dir = tempfile::tempdir().unwrap();
    let bed = dir.path().join("genes.bed");
    std::fs::write(&bed, bed_body).unwrap();
    let src = igv_core::source::open_annotation(&bed, None).await.unwrap();
    cfg.annotations.push(igv_serve::TrackEntry {
        source: src,
        path: bed.clone(),
        display: "genes.bed".into(),
    });
    (cfg, dir)
}
```

Replace `empty_config` to return `(ServerConfig, Option<tempfile::TempDir>)`
or restructure so that the temp dir handle stays alive for the duration
of each test — the simplest approach is to return `(cfg, dir)` and have
each test bind both names. Update the earlier tests accordingly (drop
the `std::mem::forget(dir)` hack).

Then add the new test:

```rust
#[tokio::test]
async fn api_features_annotation_returns_overlapping_records() {
    let (cfg, _dir) = config_with_bed(
        "chr1\t100\t400\tBRCA1\t0\t+\n\
         chr1\t1000\t2000\tFOO\t0\t-\n",
    )
    .await;
    let h = spawn(cfg).await.unwrap();
    let body: serde_json::Value = reqwest::get(format!(
        "http://{}/api/features/annotation/0?chrom=chr1&start=0&end=500",
        h.addr
    ))
    .await
    .unwrap()
    .json()
    .await
    .unwrap();
    let arr = body.as_array().unwrap();
    assert_eq!(arr.len(), 1);
    assert_eq!(arr[0]["name"], "BRCA1");
    assert_eq!(arr[0]["chr"], "chr1");
    h.shutdown().await;
}
```

- [ ] **Step 4: Verify it fails**

Run: `cargo test -p igv-serve --test http api_features_annotation`
Expected: FAIL with 404.

- [ ] **Step 5: Implement the route**

Create `crates/igv-serve/src/routes/features.rs`:

```rust
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Json, Response},
    routing::get,
    Router,
};
use serde::Deserialize;
use serde_json::Value;

use igv_core::region::Region;

use crate::feature_json::{annotation_to_json, link_to_json};
use crate::state::ServerState;

pub fn router() -> Router<ServerState> {
    Router::new()
        .route("/api/features/annotation/:idx", get(annotation))
        .route("/api/features/link/:idx", get(link))
}

#[derive(Debug, Deserialize)]
struct Window {
    chrom: String,
    start: u64,
    end: u64,
}

async fn annotation(
    State(s): State<ServerState>,
    Path(idx): Path<usize>,
    Query(w): Query<Window>,
) -> Response {
    let Some(t) = s.annotations.get(idx) else {
        return (StatusCode::NOT_FOUND, "no such annotation").into_response();
    };
    let Ok(region) = Region::new(w.chrom.clone(), w.start, w.end) else {
        return (StatusCode::BAD_REQUEST, "bad region").into_response();
    };
    match t.source.fetch(&region).await {
        Ok(records) => {
            let arr: Vec<Value> = records
                .iter()
                .map(|tx| annotation_to_json(&w.chrom, tx))
                .collect();
            Json(arr).into_response()
        }
        Err(err) => {
            tracing::error!(?err, "annotation fetch failed");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": format!("{err}") })),
            )
                .into_response()
        }
    }
}

async fn link(
    State(s): State<ServerState>,
    Path(idx): Path<usize>,
    Query(w): Query<Window>,
) -> Response {
    let Some(t) = s.links.get(idx) else {
        return (StatusCode::NOT_FOUND, "no such link").into_response();
    };
    let Ok(region) = Region::new(w.chrom.clone(), w.start, w.end) else {
        return (StatusCode::BAD_REQUEST, "bad region").into_response();
    };
    let opts = igv_core::source::FetchLinkOpts {
        min_score: s.link_min_score,
        ..Default::default()
    };
    match t.source.fetch(&region, &opts).await {
        Ok(links) => {
            let arr: Vec<Value> = links.iter().map(|vl| link_to_json(&vl.record)).collect();
            Json(arr).into_response()
        }
        Err(err) => {
            tracing::error!(?err, "link fetch failed");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": format!("{err}") })),
            )
                .into_response()
        }
    }
}
```

The exact signature of `LinkSource::fetch` and `FetchLinkOpts` is defined
in `crates/igv-core/src/source/link.rs`. The structure of `VisibleLink`
exposes a `.record: LinkRecord` field; if the field is named differently
or the fetch returns a flat `Vec<LinkRecord>`, adjust the map line
accordingly without changing the public route contract.

- [ ] **Step 6: Wire into the router and `lib.rs`**

```rust
// routes/mod.rs
pub mod features;
// in build():
.merge(features::router())
```

```rust
// lib.rs
pub mod feature_json;
```

- [ ] **Step 7: Run the tests**

Run: `cargo test -p igv-serve`
Expected: all green; the new annotation integration test passes.

- [ ] **Step 8: Commit**

```bash
git add crates/igv-serve
git commit -m "feat(serve): /api/features/annotation + JSON conversion"
```

---

### Task 8: `GET /api/features/link/{idx}` end-to-end test + score filter

The route handler shipped in Task 7 already serves links. This task adds
a focused integration test exercising the BEDPE pipeline including
`link_min_score`.

**Files:**
- Test: `crates/igv-serve/tests/http.rs`

- [ ] **Step 1: Write the failing test**

Append to `crates/igv-serve/tests/http.rs`:

```rust
async fn config_with_bedpe(body: &str, min_score: Option<f64>) -> (ServerConfig, tempfile::TempDir) {
    let mut cfg = empty_config().await;
    let dir = tempfile::tempdir().unwrap();
    let bedpe = dir.path().join("loops.bedpe");
    std::fs::write(&bedpe, body).unwrap();
    let src = igv_core::source::open_link(&bedpe, None).await.unwrap();
    cfg.links.push(igv_serve::TrackEntry {
        source: src,
        path: bedpe.clone(),
        display: "loops.bedpe".into(),
    });
    cfg.link_min_score = min_score;
    (cfg, dir)
}

#[tokio::test]
async fn api_features_link_drops_below_min_score() {
    // Two cis loops on chr1; the second has score 9.0, the first 1.0.
    let (cfg, _dir) = config_with_bedpe(
        "chr1\t100\t200\tchr1\t300\t400\tloop_a\t1.0\n\
         chr1\t500\t600\tchr1\t700\t800\tloop_b\t9.0\n",
        Some(5.0),
    )
    .await;
    let h = spawn(cfg).await.unwrap();
    let body: serde_json::Value = reqwest::get(format!(
        "http://{}/api/features/link/0?chrom=chr1&start=0&end=1000",
        h.addr
    ))
    .await
    .unwrap()
    .json()
    .await
    .unwrap();
    let arr = body.as_array().unwrap();
    assert_eq!(arr.len(), 1);
    assert_eq!(arr[0]["name"], "loop_b");
    h.shutdown().await;
}
```

- [ ] **Step 2: Run it**

Run: `cargo test -p igv-serve --test http api_features_link_drops`
Expected: PASS (the route was already implemented; this confirms the score filter is honored).

- [ ] **Step 3: Commit**

```bash
git add crates/igv-serve/tests/http.rs
git commit -m "test(serve): link endpoint honours link_min_score"
```

---

### Task 9: `GET /api/jump`

**Files:**
- Create: `crates/igv-serve/src/routes/jump.rs`
- Modify: `crates/igv-serve/src/routes/mod.rs`
- Test: `crates/igv-serve/tests/http.rs`

- [ ] **Step 1: Write the failing test**

Append to `crates/igv-serve/tests/http.rs`:

```rust
#[tokio::test]
async fn api_jump_resolves_gene_name() {
    let (cfg, _dir) = config_with_bed("chr1\t100\t400\tBRCA1\t0\t+\n").await;
    let h = spawn(cfg).await.unwrap();
    let body: serde_json::Value = reqwest::get(format!("http://{}/api/jump?name=BRCA1", h.addr))
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(body["chrom"], "chr1");
    assert_eq!(body["start"], 100);
    assert_eq!(body["end"], 400);
    h.shutdown().await;
}

#[tokio::test]
async fn api_jump_rejects_bad_name() {
    let h = spawn(empty_config().await).await.unwrap();
    let resp = reqwest::get(format!("http://{}/api/jump?name=ab%20cd", h.addr))
        .await
        .unwrap();
    assert_eq!(resp.status(), reqwest::StatusCode::BAD_REQUEST);
    h.shutdown().await;
}

#[tokio::test]
async fn api_jump_unknown_gene_returns_404() {
    let h = spawn(empty_config().await).await.unwrap();
    let resp = reqwest::get(format!("http://{}/api/jump?name=BRCA1", h.addr))
        .await
        .unwrap();
    assert_eq!(resp.status(), reqwest::StatusCode::NOT_FOUND);
    h.shutdown().await;
}
```

- [ ] **Step 2: Run them**

Run: `cargo test -p igv-serve --test http api_jump`
Expected: FAIL with 404 (no route yet).

- [ ] **Step 3: Implement the route**

Create `crates/igv-serve/src/routes/jump.rs`:

```rust
use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::{IntoResponse, Json, Response},
    routing::get,
    Router,
};
use serde::Deserialize;
use serde_json::json;

use crate::state::ServerState;

pub fn router() -> Router<ServerState> {
    Router::new().route("/api/jump", get(handler))
}

#[derive(Debug, Deserialize)]
struct Q {
    name: String,
}

fn name_is_valid(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 64
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | ':' | '-'))
}

async fn handler(State(s): State<ServerState>, Query(q): Query<Q>) -> Response {
    if !name_is_valid(&q.name) {
        return (StatusCode::BAD_REQUEST, "bad name").into_response();
    }
    let sources: Vec<_> = s.annotations.iter().map(|t| t.source.clone()).collect();
    match igv_core::source::annotation::find_by_name_union(&sources, &q.name) {
        Some((region, _label)) => Json(json!({
            "chrom": region.chrom,
            "start": region.start,
            "end": region.end,
        }))
        .into_response(),
        None => (StatusCode::NOT_FOUND, "not found").into_response(),
    }
}
```

- [ ] **Step 4: Wire into the router**

```rust
// routes/mod.rs
pub mod jump;
.merge(jump::router())
```

- [ ] **Step 5: Run the tests**

Run: `cargo test -p igv-serve --test http api_jump`
Expected: 3 tests pass.

- [ ] **Step 6: Commit**

```bash
git add crates/igv-serve
git commit -m "feat(serve): /api/jump gene-name resolver with input validation"
```

---

### Task 10: `GET /api/sse` + `push_view` end-to-end

**Files:**
- Create: `crates/igv-serve/src/routes/sse.rs`
- Modify: `crates/igv-serve/src/routes/mod.rs`
- Test: `crates/igv-serve/tests/http.rs`

- [ ] **Step 1: Write the failing test**

Append to `crates/igv-serve/tests/http.rs`:

```rust
use futures::StreamExt;

#[tokio::test]
async fn sse_emits_pushed_view_event() {
    let h = spawn(empty_config().await).await.unwrap();
    let url = format!("http://{}/api/sse", h.addr);
    // Open the stream first, then push.
    let client = reqwest::Client::new();
    let mut stream = client.get(&url).send().await.unwrap().bytes_stream();
    // Give the server a tick to attach the receiver.
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    h.push_view(ViewEvent { chrom: "chr2".into(), start: 5, end: 10 });
    let mut got = String::new();
    while let Some(chunk) = stream.next().await {
        got.push_str(&String::from_utf8_lossy(&chunk.unwrap()));
        if got.contains("\"chrom\":\"chr2\"") {
            break;
        }
    }
    assert!(got.contains("event: view"));
    assert!(got.contains(r#""start":5"#));
    h.shutdown().await;
}
```

Add `futures` to `crates/igv-serve/Cargo.toml` `[dev-dependencies]` if not already inherited.

- [ ] **Step 2: Run it**

Run: `cargo test -p igv-serve --test http sse_emits`
Expected: FAIL with 404.

- [ ] **Step 3: Implement the route**

Create `crates/igv-serve/src/routes/sse.rs`:

```rust
use axum::{
    extract::State,
    response::sse::{Event, KeepAlive, Sse},
    routing::get,
    Router,
};
use futures::stream::Stream;
use std::convert::Infallible;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::StreamExt;

use crate::state::ServerState;

pub fn router() -> Router<ServerState> {
    Router::new().route("/api/sse", get(handler))
}

async fn handler(
    State(s): State<ServerState>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let rx = s.events.subscribe();
    let stream = BroadcastStream::new(rx).filter_map(|res| match res {
        Ok(ev) => Some(Ok(Event::default()
            .event("view")
            .json_data(ev)
            .expect("ViewEvent serializes to JSON"))),
        Err(_lag) => None, // drop lagged events silently
    });
    Sse::new(stream).keep_alive(KeepAlive::default())
}
```

Add `tokio-stream` features `sync` to `crates/igv-serve/Cargo.toml`:

```toml
tokio-stream = { workspace = true, features = ["sync"] }
```

(Confirm `axum`'s `sse` requires no extra feature for the version pinned in
the workspace. Axum 0.7 has SSE in the default surface when the `tokio`
feature is enabled.)

- [ ] **Step 4: Wire into the router**

```rust
// routes/mod.rs
pub mod sse;
.merge(sse::router())
```

- [ ] **Step 5: Run the test**

Run: `cargo test -p igv-serve --test http sse_emits`
Expected: PASS.

- [ ] **Step 6: Run the full crate test suite**

Run: `cargo test -p igv-serve`
Expected: every test passes.

- [ ] **Step 7: Commit**

```bash
git add crates/igv-serve
git commit -m "feat(serve): /api/sse broadcasts TUI → browser view events"
```

---

### Task 11: `igv-tui` — `Action::OpenBrowser`, `B` keybinding, `ServeController`

**Files:**
- Modify: `crates/igv-tui/Cargo.toml`
- Modify: `crates/igv-tui/src/app/action.rs`
- Modify: `crates/igv-tui/src/app/mod.rs`
- Create: `crates/igv-tui/src/app/serve.rs`
- Modify: `crates/igv-tui/src/input.rs`

- [ ] **Step 1: Depend on `igv-serve` and `webbrowser`**

Add to `[dependencies]` in `crates/igv-tui/Cargo.toml`:

```toml
igv-serve = { path = "../igv-serve" }
webbrowser.workspace = true
```

- [ ] **Step 2: Add the new action**

Edit `crates/igv-tui/src/app/action.rs` — add a variant to the `Action`
enum (preserve all existing variants):

```rust
pub enum Action {
    // ... existing variants ...
    OpenBrowser,
}
```

- [ ] **Step 3: Write the failing keybinding test**

Append to `crates/igv-tui/src/input.rs`'s `#[cfg(test)] mod tests`:

```rust
#[test]
fn capital_b_opens_browser() {
    let mut s = InputState::default();
    assert!(matches!(s.map(&key('B'), false), Action::OpenBrowser));
}
```

- [ ] **Step 4: Run it to verify failure**

Run: `cargo test -p igv-tui capital_b_opens_browser`
Expected: FAIL (`B` not in the match yet).

- [ ] **Step 5: Bind `B` in the keymap**

In `crates/igv-tui/src/input.rs`, add this match arm next to the existing
`KeyCode::Char('S') => …` arm (line ~90):

```rust
KeyCode::Char('B') => Action::OpenBrowser,
```

- [ ] **Step 6: Run the test**

Run: `cargo test -p igv-tui capital_b_opens_browser`
Expected: PASS.

- [ ] **Step 7: Create the controller**

Create `crates/igv-tui/src/app/serve.rs`:

```rust
//! Browser-view lifecycle controller. Lazily starts an `igv-serve`
//! instance on the first `B` press and pushes view events on every
//! committed region change.

use std::sync::Arc;

use anyhow::Result;

use igv_serve::{spawn, ServerConfig, ServerHandle, TrackEntry, ViewEvent};

use crate::app::state::AppState;

#[derive(Debug, Default)]
pub struct ServeController {
    handle: Option<ServerHandle>,
    url: Option<String>,
    last_pushed: Option<ViewEvent>,
    pub auto_open: bool,
    pub port: u16,
}

impl ServeController {
    pub fn new(auto_open: bool, port: u16) -> Self {
        Self { auto_open, port, ..Default::default() }
    }

    pub async fn open(&mut self, state: &AppState) -> Result<String> {
        if self.handle.is_none() {
            let cfg = build_config(state, self.port)?;
            let h = spawn(cfg).await?;
            self.url = Some(format!("http://{}/", h.addr));
            self.handle = Some(h);
        }
        let url = self.url.clone().unwrap();
        if self.auto_open {
            let _ = webbrowser::open(&url);
        }
        Ok(url)
    }

    pub fn notify_view(&mut self, state: &AppState) {
        let ev = ViewEvent {
            chrom: state.region.chrom.clone(),
            start: state.region.start,
            end: state.region.end,
        };
        if Some(&ev) == self.last_pushed.as_ref() {
            return;
        }
        if let Some(h) = &self.handle {
            h.push_view(ev.clone());
        }
        self.last_pushed = Some(ev);
    }

    pub async fn shutdown(mut self) {
        if let Some(h) = self.handle.take() {
            h.shutdown().await;
        }
    }
}

fn build_config(state: &AppState, port: u16) -> Result<ServerConfig> {
    let bams: Vec<TrackEntry<dyn igv_core::source::BamSource>> = state
        .bams
        .iter()
        .map(|t| TrackEntry {
            source: Arc::clone(&t.source),
            path: t.path.clone(),
            display: t.display.clone(),
        })
        .collect();
    let signals: Vec<TrackEntry<dyn igv_core::source::SignalSource>> = state
        .signals
        .iter()
        .map(|t| TrackEntry {
            source: Arc::clone(&t.source),
            path: t.path.clone(),
            display: t.display.clone(),
        })
        .collect();
    let annotations: Vec<TrackEntry<dyn igv_core::source::AnnotationSource>> = state
        .annotations
        .iter()
        .map(|t| TrackEntry {
            source: Arc::clone(&t.source),
            path: t.path.clone(),
            display: t.display.clone(),
        })
        .collect();
    let links: Vec<TrackEntry<dyn igv_core::source::LinkSource>> = state
        .link_tracks
        .iter()
        .map(|t| TrackEntry {
            source: Arc::clone(&t.source),
            path: t.path.clone(),
            display: t.display.clone(),
        })
        .collect();
    let vcfs = state
        .vcf
        .as_ref()
        .map(|v| {
            vec![TrackEntry {
                source: Arc::clone(&v.source),
                path: v.path.clone(),
                display: v.display.clone(),
            }]
        })
        .unwrap_or_default();

    Ok(ServerConfig {
        bind: std::net::IpAddr::from([127, 0, 0, 1]),
        port,
        fasta: Arc::clone(&state.fasta),
        fasta_path: state.fasta_path.clone(),
        bams,
        vcfs,
        annotations,
        signals,
        links,
        initial: ViewEvent {
            chrom: state.region.chrom.clone(),
            start: state.region.start,
            end: state.region.end,
        },
        link_min_score: state.link_min_score,
    })
}
```

NOTE: this controller assumes `AppState` exposes `bams`, `vcf`, `signals`,
`annotations`, `link_tracks`, `fasta`, `fasta_path`, `region`, and
`link_min_score`. If any of these are missing or named differently in
`crates/igv-tui/src/app/state.rs`, add them (or rename the accesses).
The TUI is responsible for storing the same path+display+Arc triple that
`igv-serve::TrackEntry` requires; if today it only stores the Arc, expand
the existing TUI track structs to also keep the `path: PathBuf` and
`display: String` (both are already loaded in `main.rs` lines 72-80).

- [ ] **Step 8: Re-export the module**

Edit `crates/igv-tui/src/app/mod.rs`:

```rust
pub mod serve;
```

- [ ] **Step 9: Verify the workspace builds**

Run: `cargo check -p igv-tui`
Expected: success.

- [ ] **Step 10: Commit**

```bash
git add crates/igv-tui/Cargo.toml crates/igv-tui/src/app/{action.rs,mod.rs,serve.rs} crates/igv-tui/src/input.rs
git commit -m "feat(tui): B keybinding + ServeController for browser view"
```

---

### Task 12: Main-loop wiring — open browser on `B`, push view on every commit

**Files:**
- Modify: `crates/igv-tui/src/main.rs`

- [ ] **Step 1: Instantiate the controller before the event loop**

Find the place where `AppState` is constructed in `crates/igv-tui/src/main.rs`
(near the top of `main()`, after sources are loaded). Just before the
event loop, add:

```rust
let mut serve_controller = igv_tui::app::serve::ServeController::new(
    !args.no_browser,
    args.serve_port,
);
```

(`args.no_browser` and `args.serve_port` arrive in Task 13.)

- [ ] **Step 2: Handle `Action::OpenBrowser` inside the event loop**

In the event-handling branch where `state.apply(action)` is called, add a
match before `apply`:

```rust
if matches!(action, igv_tui::app::action::Action::OpenBrowser) {
    match serve_controller.open(state).await {
        Ok(url) => state.set_status(StatusKind::Info, format!("browser → {url}")),
        Err(e) => state.set_status(StatusKind::Error, format!("serve failed: {e}")),
    }
    continue; // do not pass OpenBrowser into state.apply
}
```

Note: depending on the existing match shape (the file uses
`state.apply(action)` which probably returns `Option<LoadRequest>`), the
right place may instead be inside `state.apply` returning early — adapt
to whichever pattern fits without breaking other actions.

- [ ] **Step 3: Push view events when a load batch completes**

Locate the line at `crates/igv-tui/src/main.rs:410`:

```rust
if state.loaded_count >= state.expected_loads() {
    state.loading = false;
}
```

Replace with:

```rust
if state.loaded_count >= state.expected_loads() {
    state.loading = false;
    serve_controller.notify_view(state);
}
```

- [ ] **Step 4: Shut the server down on graceful exit**

After the event loop exits, before `disable_raw_mode()`, add:

```rust
serve_controller.shutdown().await;
```

- [ ] **Step 5: Verify the build**

Run: `cargo build -p igv-tui`
Expected: success.

- [ ] **Step 6: Verify the existing TUI tests still pass**

Run: `cargo test -p igv-tui`
Expected: green.

- [ ] **Step 7: Commit**

```bash
git add crates/igv-tui/src/main.rs
git commit -m "feat(tui): wire ServeController into the main event loop"
```

---

### Task 13: CLI flags — `--no-browser`, `--serve-port`

**Files:**
- Modify: `crates/igv-tui/src/cli.rs`

- [ ] **Step 1: Add the flags**

Append to the `Cli` struct in `crates/igv-tui/src/cli.rs`:

```rust
/// Disable the `B` keystroke / browser launch (CI, headless servers).
#[arg(long = "no-browser")]
pub no_browser: bool,

/// TCP port for the browser-view HTTP server. 0 picks any free port.
#[arg(long = "serve-port", default_value_t = 0)]
pub serve_port: u16,
```

- [ ] **Step 2: Confirm the build still passes**

Run: `cargo build -p igv-tui`
Expected: success (these fields are already referenced in Task 12).

- [ ] **Step 3: Write a parsing smoke test**

Append to `crates/igv-tui/src/cli.rs` a `#[cfg(test)]` block if none
exists, otherwise add inside it:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[test]
    fn no_browser_and_serve_port_parse() {
        let cli = Cli::parse_from([
            "igv-rs",
            "ref.fa",
            "--no-browser",
            "--serve-port", "9001",
        ]);
        assert!(cli.no_browser);
        assert_eq!(cli.serve_port, 9001);
    }

    #[test]
    fn serve_port_defaults_to_zero() {
        let cli = Cli::parse_from(["igv-rs", "ref.fa"]);
        assert_eq!(cli.serve_port, 0);
        assert!(!cli.no_browser);
    }
}
```

- [ ] **Step 4: Run the tests**

Run: `cargo test -p igv-tui cli::tests`
Expected: 2 tests pass.

- [ ] **Step 5: Commit**

```bash
git add crates/igv-tui/src/cli.rs
git commit -m "feat(tui-cli): --no-browser and --serve-port flags"
```

---

### Task 14: `[serve]` config table + footer status + help overlay row

**Files:**
- Modify: `crates/igv-tui/src/ui/theme.rs` (or wherever the config TOML is parsed today)
- Modify: `crates/igv-tui/src/ui/widgets/doc.rs`
- Modify: `crates/igv-tui/src/main.rs` (so CLI overrides config)

- [ ] **Step 1: Inspect the existing config parser**

Run: `grep -n "\\[theme\\]\\|config.toml\\|toml::from" /home/xzg/project/igv_rs/crates/igv-tui/src/ -r`

That tells you which file owns config loading. The new keys live next to
the existing `[theme]` table.

- [ ] **Step 2: Add the `[serve]` table to the config schema**

In the same file, extend the `RootConfig` (or equivalent) struct:

```rust
#[derive(Debug, Default, serde::Deserialize)]
pub struct ServeConfig {
    #[serde(default = "default_auto_open")]
    pub auto_open: bool,
    #[serde(default)]
    pub port: u16,
}

fn default_auto_open() -> bool { true }
```

and add `pub serve: ServeConfig` to the root struct (with `#[serde(default)]`).

- [ ] **Step 3: Apply precedence in `main.rs`**

In `crates/igv-tui/src/main.rs`, after CLI parse and config load, resolve
the effective values:

```rust
let auto_open = !args.no_browser && config.serve.auto_open;
let port = if args.serve_port != 0 { args.serve_port } else { config.serve.port };
let mut serve_controller = igv_tui::app::serve::ServeController::new(auto_open, port);
```

Replace the earlier instantiation from Task 12 step 1.

- [ ] **Step 4: Add the help overlay row**

In `crates/igv-tui/src/ui/widgets/doc.rs`, locate the table of keybinding
rows (search for the `S — snapshot` row added by an earlier release) and
insert immediately after it:

```rust
("B", "open browser view (igv.js)"),
```

If the table uses a different shape, match it line-for-line — do not
restructure the widget.

- [ ] **Step 5: Build and run the existing tests**

Run: `cargo test -p igv-tui`
Expected: green.

- [ ] **Step 6: Commit**

```bash
git add crates/igv-tui/src
git commit -m "feat(tui): [serve] config table + B in help overlay"
```

---

### Task 15: README usage section

**Files:**
- Modify: `README.md`

- [ ] **Step 1: Append the new section**

Insert before the "Configuration" heading in `/home/xzg/project/igv_rs/README.md`:

````markdown
### Browser view (igv.js)

Inside the TUI press `B` to launch a local HTTP server and open
[igv.js](https://github.com/igvteam/igv.js) in your default browser. The
browser starts at the TUI's current region and follows your navigation
in real time:

```bash
igv-rs ref.fa -b sample.bam -g genes.gtf -l loops.bedpe
# inside the TUI:
#   B          → open browser view
#   d/s/:gene  → browser tab follows
#   q          → exits the TUI and shuts the server down
```

The server binds to `127.0.0.1` on an ephemeral port (override with
`--serve-port`) and exposes only the tracks you passed on the CLI. Disable
the keystroke entirely with `--no-browser`. No data leaves the loopback
interface; there is no authentication and no remote-access support in
v0.5 — use this for one-shot inspection on the same machine.

`igv.js` itself is bundled into the binary, so the browser view works
offline.
````

- [ ] **Step 2: Update the keybinding table**

Add this row to the "Keybindings" list:

```markdown
- `B` — open browser view (igv.js); see "Browser view" above
```

- [ ] **Step 3: Note the new limitation**

Add to "Known limitations":

```markdown
- **Browser view is loopback-only.** Remote access (LAN / WAN), auth
  tokens, and reverse browser → TUI sync are tracked as follow-up specs.
```

- [ ] **Step 4: Commit**

```bash
git add README.md
git commit -m "docs(readme): document the browser view feature"
```

---

### Task 16: Final integration check

- [ ] **Step 1: Run the entire workspace**

```bash
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Expected: every target compiles, every test passes, zero clippy warnings.

- [ ] **Step 2: Manual smoke (documented, not in CI)**

Document this in a new section at the bottom of
`docs/superpowers/specs/2026-05-11-browser-serve-design.md` (or a
companion `docs/superpowers/manual-tests/2026-05-11-browser-serve.md`):

```markdown
1. Run `igv-rs reference.fa -b sample.bam -g genes.gtf -l loops.bedpe`.
2. Press `B`. Default browser opens a tab at `http://127.0.0.1:<port>/`.
3. Verify the browser shows reference, alignment, annotation, link
   tracks at the TUI's current region.
4. Press `d` in the TUI. The browser viewport advances.
5. Press `:BRCA1` in the TUI. The browser jumps to BRCA1.
6. Quit the TUI with `q`. The browser tab shows network errors as
   expected; the process exits cleanly.
```

- [ ] **Step 3: Commit (only if the smoke doc was created)**

```bash
git add docs/superpowers/manual-tests
git commit -m "docs(serve): manual smoke checklist for browser view"
```

---

## Self-review (writer-side, fix inline)

**Spec coverage check.** Walked each section of
`docs/superpowers/specs/2026-05-11-browser-serve-design.md`:

- §2 decisions: all five locked decisions are visible in the file layout
  (hybrid → Tasks 6 vs 7-8; TUI keystroke → Task 11; SSE → Task 10;
  bundled igv.js → Task 1; new crate → Task 1).
- §3 architecture diagram: matches Tasks 4 + 11 + 12.
- §4.1 public API: `ServerHandle`, `ServerConfig`, `TrackEntry`,
  `ServeError` are defined in Tasks 3 and 4.
- §4.2 igv-tui integration: covered by Tasks 11-14.
- §5.1 routes table: each row → Tasks 4, 5, 6, 7, 8, 9, 10.
- §5.2 `/api/config` shape: Task 5 plus self-correction (no `mappings`
  field, matching the spec edit).
- §5.3 JSON shapes: Task 7 (annotation + link).
- §5.4 SSE protocol: Task 10.
- §6 lifecycle diagram: Task 11 + 12.
- §7 CLI / config: Tasks 13, 14.
- §8 error handling: Tasks 4 (`ServeError`), 6 (404 on missing file), 7
  (500 on fetch error), 9 (400 on bad name).
- §9 security: hard-coded `127.0.0.1` in Task 11 step 7; whitelist in
  Task 6 (explicit per-path routes, no `ServeDir`); name regex in Task 9.
- §10 out of scope: nothing implemented here; README note in Task 15.
- §11 testing: every layer mentioned in the matrix has a task.
- §12 acceptance criteria 1-8: 1-3 → Task 12 + 15; 4 → Task 9; 5 →
  Task 8; 6 → Task 12 step 4; 7 → Task 1 (vendored asset); 8 → Task 16.

**Placeholder scan.** Searched the plan for "TBD", "TODO", "implement
later", vague phrases — none present. Each step contains the actual
code or command.

**Type consistency.** `ServerHandle::shutdown` is defined as `async fn …`
in Task 4 step 3 and called as `h.shutdown().await` in Tasks 6, 7, 8, 9,
10 — consistent. `push_view` signature matches across Task 4 (definition)
and Task 11 (call). `ViewEvent` field names (`chrom`, `start`, `end`)
match across Tasks 3, 5, 10, 11. `TrackEntry` shape (`source`, `path`,
`display`) is consistent across Task 3 definition and Task 11 usage.

No outstanding gaps. Plan is ready to execute.
