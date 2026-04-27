//! Signal-track source — numeric quantitative tracks (bigWig today;
//! bedGraph / wig in the future) rendered as bar-chart widgets.
//!
//! The concrete bigtools-backed implementation lives in `signal::bigwig`.

use async_trait::async_trait;
use std::path::Path;
use std::sync::Arc;

use crate::error::{IgvError, Result};
use crate::region::Region;

pub mod bigwig;

#[derive(Debug, Clone, PartialEq)]
pub struct SignalBin {
    pub start: u64,   // 1-based inclusive
    pub end: u64,     // 1-based inclusive
    pub value: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignalSummary {
    Max,
    Mean,
    Sum,
    Min,
}

#[derive(Debug, Clone, Copy)]
pub struct FetchSignalOpts {
    pub max_bins: u32,
    pub summary: SignalSummary,
}

impl Default for FetchSignalOpts {
    fn default() -> Self {
        Self { max_bins: 200, summary: SignalSummary::Max }
    }
}

#[async_trait]
pub trait SignalSource: Send + Sync {
    async fn fetch(
        &self,
        region: &Region,
        opts: &FetchSignalOpts,
    ) -> Result<Vec<SignalBin>>;
    fn display_name(&self) -> &str;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignalFormat {
    BigWig,
}

impl SignalFormat {
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "bw" | "bigwig" => Some(Self::BigWig),
            _ => None,
        }
    }

    pub fn from_path(path: &Path) -> Option<Self> {
        let lower = path
            .file_name()
            .and_then(|n| n.to_str())
            .map(|s| s.to_ascii_lowercase())?;
        if lower.ends_with(".bw") || lower.ends_with(".bigwig") {
            return Some(Self::BigWig);
        }
        None
    }
}

/// Open a signal file, dispatching to the right backend by extension
/// (or by `format_override` if given).
pub async fn open_signal(
    path: &Path,
    format_override: Option<SignalFormat>,
) -> Result<Arc<dyn SignalSource>> {
    let format = format_override
        .or_else(|| SignalFormat::from_path(path))
        .ok_or_else(|| {
            IgvError::Other(format!(
                "cannot determine signal format for '{}'; pass --signal-format",
                path.display()
            ))
        })?;
    match format {
        SignalFormat::BigWig => {
            let src = bigwig::BigWigSignalSource::open(path).await?;
            Ok(Arc::new(src))
        }
    }
}
