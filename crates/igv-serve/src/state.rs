use std::path::PathBuf;
use std::sync::Arc;

use tokio::sync::broadcast;

use igv_core::source::{
    AnnotationSource, BamSource, FastaSource, LinkSource, SignalSource, VcfSource,
};

use crate::view::ViewEvent;

#[derive(Debug)]
pub struct TrackEntry<S: ?Sized> {
    pub source: Arc<S>,
    pub path: PathBuf,
    pub display: String,
}

impl<S: ?Sized> Clone for TrackEntry<S> {
    fn clone(&self) -> Self {
        Self {
            source: Arc::clone(&self.source),
            path: self.path.clone(),
            display: self.display.clone(),
        }
    }
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
