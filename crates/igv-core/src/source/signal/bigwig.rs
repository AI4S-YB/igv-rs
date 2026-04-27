//! BigWig signal source backed by the `bigtools` crate.
//!
//! BBI header is parsed once at `open()` and the reader is held in a
//! `tokio::sync::Mutex` for the lifetime of the source — concurrent
//! `fetch()` calls against the same file serialize, distinct files run
//! fully in parallel.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::Mutex;

use crate::error::{IgvError, Result};
use crate::region::Region;

use super::{FetchSignalOpts, SignalBin, SignalSource};

// Concrete bigtools type alias — kept local so bigtools API churn doesn't
// leak into the trait.
type BwReader = bigtools::BigWigRead<bigtools::utils::reopen::ReopenableFile>;

pub struct BigWigSignalSource {
    display: String,
    #[allow(dead_code)]
    path: PathBuf,
    reader: Arc<Mutex<BwReader>>,
}

impl std::fmt::Debug for BigWigSignalSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BigWigSignalSource")
            .field("display", &self.display)
            .field("path", &self.path)
            .finish_non_exhaustive()
    }
}

impl BigWigSignalSource {
    pub async fn open(path: &Path) -> Result<Self> {
        let p = path.to_path_buf();
        let reader = tokio::task::spawn_blocking(move || -> Result<BwReader> {
            bigtools::BigWigRead::open_file(&p)
                .map_err(|e| IgvError::Other(format!("bigwig open: {e}")))
        })
        .await
        .map_err(|e| IgvError::Other(e.to_string()))??;

        Ok(Self {
            display: path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("signal")
                .to_string(),
            path: path.to_path_buf(),
            reader: Arc::new(Mutex::new(reader)),
        })
    }
}

#[async_trait]
impl SignalSource for BigWigSignalSource {
    async fn fetch(
        &self,
        region: &Region,
        opts: &FetchSignalOpts,
    ) -> Result<Vec<SignalBin>> {
        let chrom = region.chrom.clone();
        // bigtools uses 0-based half-open [start, end); Region is 1-based inclusive.
        let start = (region.start.saturating_sub(1)) as u32;
        let end = region.end.min(u32::MAX as u64) as u32;
        let max_bins = opts.max_bins.max(1);
        let bp_per_col = (region.width().max(1) as u32).saturating_div(max_bins);

        // v1 always uses Max for the zoom summary regardless of opts.summary.
        // Future work: honor SignalSummary::{Mean, Sum, Min} via the corresponding
        // ZoomRecord::summary fields. Tracked under "Open / deferred items" in
        // docs/superpowers/specs/2026-04-27-bigwig-signal-design.md §9.
        let _ = opts.summary;

        let reader = Arc::clone(&self.reader);
        let bins = tokio::task::spawn_blocking(move || -> Result<Vec<SignalBin>> {
            let mut guard = reader.blocking_lock();

            // Verify chrom exists; if not, return empty Vec.
            let chrom_known = guard
                .chroms()
                .iter()
                .any(|c| c.name == chrom);
            if !chrom_known {
                tracing::debug!("bigwig: chrom not found: {chrom}");
                return Ok(Vec::new());
            }

            if bp_per_col >= 16 {
                // Zoom-summary path.
                // Pick the best zoom level: largest reduction_level that is <= bp_per_col,
                // or if none qualifies, use the smallest available.
                let reduction_level: Option<u32> = {
                    let headers = &guard.info().zoom_headers;
                    if headers.is_empty() {
                        None
                    } else {
                        let chosen = headers
                            .iter()
                            .filter(|h| h.reduction_level <= bp_per_col)
                            .max_by_key(|h| h.reduction_level)
                            .or_else(|| headers.iter().min_by_key(|h| h.reduction_level));
                        chosen.map(|h| h.reduction_level)
                    }
                };

                let level = match reduction_level {
                    None => {
                        // No usable zoom level — fall back to raw.
                        let values: Vec<_> = guard
                            .get_interval(&chrom, start, end)
                            .map_err(|e| IgvError::Other(format!("bigwig values: {e}")))?
                            .filter_map(|r| r.ok())
                            .collect();
                        drop(guard);
                        return Ok(values
                            .into_iter()
                            .map(|v| SignalBin {
                                start: u64::from(v.start) + 1,
                                end: u64::from(v.end),
                                value: v.value,
                            })
                            .collect());
                    }
                    Some(level) => level,
                };

                let records: Vec<_> = guard
                    .get_zoom_interval(&chrom, start, end, level)
                    .map_err(|e| IgvError::Other(format!("bigwig zoom: {e}")))?
                    .filter_map(|r| r.ok())
                    .collect();
                drop(guard);

                let bins = records
                    .into_iter()
                    .map(|z| SignalBin {
                        start: u64::from(z.start) + 1, // back to 1-based inclusive
                        end: u64::from(z.end),
                        value: z.summary.max_val as f32, // always max — see top of fetch() for rationale
                    })
                    .collect();
                Ok(bins)
            } else {
                // Raw path: collect while holding the lock, then map CPU-only after releasing.
                let values: Vec<_> = guard
                    .get_interval(&chrom, start, end)
                    .map_err(|e| IgvError::Other(format!("bigwig values: {e}")))?
                    .filter_map(|r| r.ok())
                    .collect();
                drop(guard);
                Ok(values
                    .into_iter()
                    .map(|v| SignalBin {
                        start: u64::from(v.start) + 1, // 0-based → 1-based inclusive
                        end: u64::from(v.end),
                        value: v.value,
                    })
                    .collect())
            }
        })
        .await
        .map_err(|e| IgvError::Other(e.to_string()))??;

        Ok(bins)
    }

    fn display_name(&self) -> &str {
        &self.display
    }
}

