//! Local HTTP server that mirrors the TUI's view in igv.js.
//!
//! See `docs/superpowers/specs/2026-05-11-browser-serve-design.md`.

#![forbid(unsafe_code)]
#![warn(rust_2018_idioms, missing_debug_implementations)]

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

impl std::fmt::Debug for ServerConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ServerConfig")
            .field("bind", &self.bind)
            .field("port", &self.port)
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
    let listener = tokio::net::TcpListener::bind(bind_addr)
        .await
        .map_err(|source| ServeError::BindFailed { addr: bind_addr, source })?;
    let addr = listener.local_addr()?;
    let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();
    let join = tokio::spawn(async move {
        let server = axum::serve(listener, router).with_graceful_shutdown(async {
            let _ = shutdown_rx.await;
        });
        if let Err(err) = server.await {
            tracing::error!(?err, "igv-serve axum task ended with error");
        }
    });
    Ok(ServerHandle { addr, events, join, shutdown: Some(shutdown_tx) })
}
