//! BED source — implemented in Task 1.4.

use std::path::Path;

use async_trait::async_trait;

use crate::error::{IgvError, Result};
use crate::region::Region;

use super::{AnnotationSource, AnnotationTranscript};

pub struct NoodlesBedSource {
    path: std::path::PathBuf,
    display: String,
}

impl NoodlesBedSource {
    pub async fn open(path: &Path) -> Result<Self> {
        Ok(Self {
            path: path.to_path_buf(),
            display: path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("annotation")
                .to_string(),
        })
    }
}

#[async_trait]
impl AnnotationSource for NoodlesBedSource {
    async fn fetch(&self, _region: &Region) -> Result<Vec<AnnotationTranscript>> {
        Err(IgvError::Other(format!(
            "NoodlesBedSource::fetch not yet implemented (path={})",
            self.path.display()
        )))
    }

    fn display_name(&self) -> &str {
        &self.display
    }
}
