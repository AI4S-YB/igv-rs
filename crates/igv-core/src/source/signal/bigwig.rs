//! BigWig signal source backed by the `bigtools` crate.
//! Populated in Phase 2; this stub keeps `signal.rs` compiling until then.

use std::path::Path;

use async_trait::async_trait;

use crate::error::{IgvError, Result};
use crate::region::Region;

use super::{FetchSignalOpts, SignalBin, SignalSource};

#[derive(Debug)]
pub struct BigWigSignalSource {
    display: String,
}

impl BigWigSignalSource {
    pub async fn open(_path: &Path) -> Result<Self> {
        Err(IgvError::Other("bigwig backend not yet implemented".into()))
    }
}

#[async_trait]
impl SignalSource for BigWigSignalSource {
    async fn fetch(
        &self,
        _region: &Region,
        _opts: &FetchSignalOpts,
    ) -> Result<Vec<SignalBin>> {
        Ok(Vec::new())
    }

    fn display_name(&self) -> &str {
        &self.display
    }
}
