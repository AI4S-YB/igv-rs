//! Browser-view lifecycle controller. Lazily starts an `igv-serve`
//! instance on the first `B` press and pushes view events on every
//! committed region change.

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;

use igv_serve::{spawn, ServerConfig, ServerHandle, TrackEntry, ViewEvent};

use crate::app::state::AppState;

#[derive(Debug)]
pub struct ServeController {
    handle: Option<ServerHandle>,
    url: Option<String>,
    last_pushed: Option<ViewEvent>,
    pub auto_open: bool,
    pub host: std::net::IpAddr,
    pub port: u16,
    pub fasta_path: PathBuf,
    pub vcf_path: Option<PathBuf>,
    pub vcf_display: Option<String>,
}

impl Default for ServeController {
    fn default() -> Self {
        Self {
            handle: None,
            url: None,
            last_pushed: None,
            auto_open: false,
            host: crate::ui::theme::default_host(),
            port: 0,
            fasta_path: PathBuf::default(),
            vcf_path: None,
            vcf_display: None,
        }
    }
}

impl ServeController {
    pub fn new(
        auto_open: bool,
        host: std::net::IpAddr,
        port: u16,
        fasta_path: PathBuf,
        vcf_path: Option<PathBuf>,
    ) -> Self {
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
            host,
            port,
            fasta_path,
            vcf_path,
            vcf_display,
        }
    }

    pub async fn open(&mut self, state: &AppState) -> Result<String> {
        if self.handle.is_none() {
            if !self.host.is_loopback() {
                tracing::warn!(
                    host = %self.host,
                    "serve host is not loopback; the browser view is reachable from \
                     the network with no authentication — bind only on a trusted network"
                );
            }
            let cfg = self.build_config(state);
            let h = spawn(cfg).await?;
            self.url = Some(serve_url(h.addr));
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
            bind: self.host,
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

/// Build the client-facing URL for a server bound at `addr`.
///
/// The bound socket address carries the *server's* listen IP, which may be an
/// unspecified "any" address (`0.0.0.0` or `::`) when the user configures a
/// broad bind. That is a valid *bind* target but not a valid *destination* for
/// a browser — macOS and Windows refuse `http://0.0.0.0:…`. Rewrite an
/// unspecified IP to its loopback analogue so the URL we pass to `webbrowser`
/// (and print to the user) actually connects. The server still listens on the
/// broad address; only the client URL is normalized.
fn serve_url(addr: std::net::SocketAddr) -> String {
    let ip = if addr.ip().is_unspecified() {
        match addr.ip() {
            std::net::IpAddr::V4(_) => std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
            std::net::IpAddr::V6(_) => std::net::IpAddr::V6(std::net::Ipv6Addr::LOCALHOST),
        }
    } else {
        addr.ip()
    };
    format!("http://{}/", std::net::SocketAddr::new(ip, addr.port()))
}

#[cfg(test)]
mod tests {
    use super::serve_url;
    use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};

    #[test]
    fn serve_url_keeps_loopback_v4() {
        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 8080);
        assert_eq!(serve_url(addr), "http://127.0.0.1:8080/");
    }

    #[test]
    fn serve_url_keeps_loopback_v6() {
        let addr = SocketAddr::new(IpAddr::V6(Ipv6Addr::LOCALHOST), 8080);
        assert_eq!(serve_url(addr), "http://[::1]:8080/");
    }

    #[test]
    fn serve_url_rewrites_unspecified_v4_to_loopback() {
        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 8080);
        assert_eq!(serve_url(addr), "http://127.0.0.1:8080/");
    }

    #[test]
    fn serve_url_rewrites_unspecified_v6_to_loopback() {
        let addr = SocketAddr::new(IpAddr::V6(Ipv6Addr::UNSPECIFIED), 8080);
        assert_eq!(serve_url(addr), "http://[::1]:8080/");
    }

    #[test]
    fn serve_url_keeps_concrete_lan_address() {
        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 10)), 9000);
        assert_eq!(serve_url(addr), "http://192.168.1.10:9000/");
    }
}
