use std::path::{Path, PathBuf};
use std::sync::Arc;

use async_trait::async_trait;
use noodles::fasta::{self as fasta};
use tokio::sync::Mutex;

use crate::error::{IgvError, Result};
use crate::region::Region;

use super::{FastaSource, RefMeta};

pub struct NoodlesFastaSource {
    path: PathBuf,
    inner: Arc<Mutex<fasta::IndexedReader<fasta::io::BufReader<std::fs::File>>>>,
    refs: Vec<RefMeta>,
}

impl std::fmt::Debug for NoodlesFastaSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NoodlesFastaSource")
            .field("path", &self.path)
            .field("refs", &self.refs)
            .finish()
    }
}

impl NoodlesFastaSource {
    pub async fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        let p = path.clone();

        // FASTA + FAI parsing is cheap and synchronous; do it on a blocking
        // thread to avoid stalling the runtime.
        tokio::task::spawn_blocking(move || -> Result<Self> {
            let fai_path = format!("{}.fai", p.display());
            if !std::path::Path::new(&fai_path).exists() {
                return Err(IgvError::MissingIndex(fai_path.into()));
            }
            let reader = fasta::indexed_reader::Builder::default()
                .build_from_path(&p)
                .map_err(|e| IgvError::io(p.clone(), e))?;
            let index = reader.index();
            let refs = index
                .as_ref()
                .iter()
                .map(|rec| RefMeta {
                    name: std::str::from_utf8(rec.name())
                        .unwrap_or_default()
                        .to_string(),
                    length: rec.length(),
                })
                .collect();
            Ok(Self {
                path: p,
                inner: Arc::new(Mutex::new(reader)),
                refs,
            })
        })
        .await
        .map_err(|e| IgvError::Other(e.to_string()))?
    }
}

#[async_trait]
impl FastaSource for NoodlesFastaSource {
    async fn references(&self) -> Result<Vec<RefMeta>> {
        Ok(self.refs.clone())
    }

    async fn fetch(&self, region: &Region) -> Result<Vec<u8>> {
        let chrom = region.chrom.clone();
        let start = region.start;
        let end = region.end;
        let inner = Arc::clone(&self.inner);

        tokio::task::spawn_blocking(move || -> Result<Vec<u8>> {
            let mut guard = inner.blocking_lock();
            let region_str = format!("{}:{}-{}", chrom, start, end);
            let r: noodles::core::Region = region_str
                .parse()
                .map_err(|_| IgvError::InvalidRegion(region_str.clone()))?;
            let record = guard.query(&r).map_err(IgvError::noodles)?;
            Ok(record.sequence().as_ref().to_vec())
        })
        .await
        .map_err(|e| IgvError::Other(e.to_string()))?
    }
}
