//! BigWig signal source backed by `bigtools` over `SeekableStream`
//! (supports local files, remote HTTP/S3/GS URLs, chrom aliases, and
//! all `SignalSummary` aggregation modes).

use std::path::Path;
use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::Mutex;

use crate::error::{IgvError, Result};
use crate::genome::resolve_chrom_name;
use crate::io::{Location, SeekableStream};
use crate::region::Region;

use super::{FetchSignalOpts, SignalBin, SignalSource, SignalSummary};

type BwReader = bigtools::BigWigRead<SeekableStream>;

pub struct BigWigSignalSource {
    display: String,
    reader: Arc<Mutex<BwReader>>,
    chroms: Vec<String>,
}

impl std::fmt::Debug for BigWigSignalSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BigWigSignalSource")
            .field("display", &self.display)
            .field("chroms", &self.chroms.len())
            .finish_non_exhaustive()
    }
}

impl BigWigSignalSource {
    pub async fn open(path: &Path) -> Result<Self> {
        let loc = Location::Local(path.to_path_buf());
        Self::open_location(&loc, None).await
    }

    pub async fn open_location(location: &Location, auth_token: Option<&str>) -> Result<Self> {
        let display = location.display_name();
        let stream = SeekableStream::open(location, auth_token).await?;
        let reader = tokio::task::spawn_blocking(move || -> Result<BwReader> {
            bigtools::BigWigRead::open(stream)
                .map_err(|e| IgvError::Other(format!("bigwig open: {e}")))
        })
        .await
        .map_err(|e| IgvError::Other(e.to_string()))??;
        let chroms = reader.chroms().iter().map(|c| c.name.clone()).collect();

        Ok(Self {
            display,
            reader: Arc::new(Mutex::new(reader)),
            chroms,
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
        let Some(chrom) = resolve_chrom_name(&region.chrom, &self.chroms).map(str::to_string)
        else {
            return Ok(Vec::new());
        };
        let start = (region.start.saturating_sub(1)) as u32;
        let end = region.end.min(u32::MAX as u64) as u32;
        let max_bins = opts.max_bins.max(1);
        let bp_per_col = (region.width().max(1) as u32).saturating_div(max_bins);
        let summary = opts.summary;

        let reader = Arc::clone(&self.reader);
        let bins = tokio::task::spawn_blocking(move || -> Result<Vec<SignalBin>> {
            let mut guard = reader.blocking_lock();

            if bp_per_col >= 16 {
                let reduction_level: Option<u32> = {
                    let headers = &guard.info().zoom_headers;
                    if headers.is_empty() {
                        None
                    } else {
                        headers
                            .iter()
                            .filter(|h| h.reduction_level <= bp_per_col)
                            .max_by_key(|h| h.reduction_level)
                            .or_else(|| headers.iter().min_by_key(|h| h.reduction_level))
                            .map(|h| h.reduction_level)
                    }
                };

                if let Some(level) = reduction_level {
                    let records: Vec<_> = guard
                        .get_zoom_interval(&chrom, start, end, level)
                        .map_err(|e| IgvError::Other(format!("bigwig zoom: {e}")))?
                        .filter_map(|r| r.ok())
                        .collect();
                    drop(guard);

                    return Ok(records
                        .into_iter()
                        .map(|z| {
                            let val = match summary {
                                SignalSummary::Max => z.summary.max_val,
                                SignalSummary::Min => z.summary.min_val,
                                SignalSummary::Sum => z.summary.sum,
                                SignalSummary::Mean => {
                                    if z.summary.bases_covered > 0 {
                                        z.summary.sum / z.summary.bases_covered as f64
                                    } else {
                                        0.0
                                    }
                                }
                            };
                            SignalBin {
                                start: u64::from(z.start) + 1,
                                end: u64::from(z.end),
                                value: val as f32,
                            }
                        })
                        .collect());
                }
            }

            let values: Vec<_> = guard
                .get_interval(&chrom, start, end)
                .map_err(|e| IgvError::Other(format!("bigwig values: {e}")))?
                .filter_map(|r| r.ok())
                .collect();
            drop(guard);
            Ok(values
                .into_iter()
                .map(|v| SignalBin {
                    start: u64::from(v.start) + 1,
                    end: u64::from(v.end),
                    value: v.value,
                })
                .collect())
        })
        .await
        .map_err(|e| IgvError::Other(e.to_string()))??;

        Ok(bins)
    }

    fn display_name(&self) -> &str {
        &self.display
    }
}
