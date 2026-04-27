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
    #[allow(dead_code)]
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
        _region: &Region,
        _opts: &FetchSignalOpts,
    ) -> Result<Vec<SignalBin>> {
        // populated in Task 2.4
        Ok(Vec::new())
    }

    fn display_name(&self) -> &str {
        &self.display
    }
}
