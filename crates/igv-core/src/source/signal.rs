//! Signal-track source — numeric quantitative tracks (`bigWig`, `wig`, `bedGraph`).

use async_trait::async_trait;
use std::path::Path;
use std::sync::Arc;

use crate::error::{IgvError, Result};
use crate::io::Location;
use crate::region::Region;

pub mod bigwig;
pub mod wig;

#[derive(Debug, Clone, PartialEq)]
pub struct SignalBin {
    pub start: u64, // 1-based inclusive
    pub end: u64,   // 1-based inclusive
    pub value: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SignalSummary {
    #[default]
    Max,
    Mean,
    Sum,
    Min,
}

impl SignalSummary {
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "max" => Some(Self::Max),
            "mean" | "avg" | "average" => Some(Self::Mean),
            "sum" => Some(Self::Sum),
            "min" => Some(Self::Min),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Max => "max",
            Self::Mean => "mean",
            Self::Sum => "sum",
            Self::Min => "min",
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct FetchSignalOpts {
    pub max_bins: u32,
    pub summary: SignalSummary,
}

impl Default for FetchSignalOpts {
    fn default() -> Self {
        Self {
            max_bins: 200,
            summary: SignalSummary::Max,
        }
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
    Wig,
}

impl SignalFormat {
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "bw" | "bigwig" => Some(Self::BigWig),
            "wig" | "bdg" | "bedgraph" => Some(Self::Wig),
            _ => None,
        }
    }

    pub fn from_path(path: &Path) -> Option<Self> {
        let lower = path
            .file_name()
            .and_then(|n| n.to_str())
            .map(|s| s.to_ascii_lowercase())?;
        Self::from_name(&lower)
    }

    pub fn from_name(lower: &str) -> Option<Self> {
        if lower.ends_with(".bw") || lower.ends_with(".bigwig") {
            return Some(Self::BigWig);
        }
        if lower.ends_with(".wig")
            || lower.ends_with(".wig.gz")
            || lower.ends_with(".bdg")
            || lower.ends_with(".bdg.gz")
            || lower.ends_with(".bedgraph")
            || lower.ends_with(".bedgraph.gz")
        {
            return Some(Self::Wig);
        }
        None
    }
}

pub async fn open_signal(
    path: &Path,
    format_override: Option<SignalFormat>,
) -> Result<Arc<dyn SignalSource>> {
    let loc = Location::Local(path.to_path_buf());
    open_signal_location(&loc, format_override, None).await
}

pub async fn open_signal_location(
    location: &Location,
    format_override: Option<SignalFormat>,
    auth_token: Option<&str>,
) -> Result<Arc<dyn SignalSource>> {
    let format = format_override
        .or_else(|| SignalFormat::from_name(&location.lowercase_name()))
        .ok_or_else(|| {
            IgvError::Other(format!(
                "cannot determine signal format for '{}'; pass --signal-format",
                location.as_str()
            ))
        })?;
    match format {
        SignalFormat::BigWig => {
            let src = bigwig::BigWigSignalSource::open_location(location, auth_token).await?;
            Ok(Arc::new(src))
        }
        SignalFormat::Wig => {
            let src = wig::WigSignalSource::open_location(location, auth_token).await?;
            Ok(Arc::new(src))
        }
    }
}
