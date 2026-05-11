//! Browser-view lifecycle controller. Lazily starts an `igv-serve`
//! instance on the first `B` press and pushes view events on every
//! committed region change.

use std::path::PathBuf;
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
    pub fasta_path: PathBuf,
    pub vcf_path: Option<PathBuf>,
    pub vcf_display: Option<String>,
}

impl ServeController {
    pub fn new(auto_open: bool, port: u16, fasta_path: PathBuf, vcf_path: Option<PathBuf>) -> Self {
        let vcf_display = vcf_path.as_ref().map(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("vcf")
                .to_string()
        });
        Self {
            handle: None,
            url: None,
            last_pushed: None,
            auto_open,
            port,
            fasta_path,
            vcf_path,
            vcf_display,
        }
    }

    pub async fn open(&mut self, state: &AppState) -> Result<String> {
        if self.handle.is_none() {
            let cfg = self.build_config(state);
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

    fn build_config(&self, state: &AppState) -> ServerConfig {
        let bams = state
            .bams
            .iter()
            .map(|t| TrackEntry {
                source: Arc::clone(&t.source),
                path: t.path.clone(),
                display: t.display.clone(),
            })
            .collect();
        let signals = state
            .signals
            .iter()
            .map(|t| TrackEntry {
                source: Arc::clone(&t.source),
                path: t.path.clone(),
                display: t.display.clone(),
            })
            .collect();
        let annotations = state
            .annotations
            .iter()
            .map(|t| TrackEntry {
                source: Arc::clone(&t.source),
                path: t.path.clone(),
                display: t.display.clone(),
            })
            .collect();
        let links = state
            .links
            .iter()
            .map(|t| TrackEntry {
                source: Arc::clone(&t.source),
                path: t.path.clone(),
                display: t.display.clone(),
            })
            .collect();
        let vcfs = match (&state.vcf, &self.vcf_path) {
            (Some(src), Some(path)) => vec![TrackEntry {
                source: Arc::clone(src),
                path: path.clone(),
                display: self
                    .vcf_display
                    .clone()
                    .unwrap_or_else(|| "vcf".to_string()),
            }],
            _ => Vec::new(),
        };

        ServerConfig {
            bind: std::net::IpAddr::from([127, 0, 0, 1]),
            port: self.port,
            fasta: Arc::clone(&state.fasta),
            fasta_path: self.fasta_path.clone(),
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
        }
    }
}
