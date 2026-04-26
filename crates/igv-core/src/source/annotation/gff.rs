//! GFF3 / GTF source — implemented in Task 1.3.

use std::path::Path;

use async_trait::async_trait;

use crate::error::{IgvError, Result};
use crate::region::Region;

use super::{AnnotationFormat, AnnotationSource, AnnotationTranscript};

pub struct NoodlesGffSource {
    path: std::path::PathBuf,
    display: String,
    #[allow(dead_code)]
    format: AnnotationFormat,
}

impl NoodlesGffSource {
    pub async fn open(path: &Path, format: AnnotationFormat) -> Result<Self> {
        Ok(Self {
            path: path.to_path_buf(),
            display: path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("annotation")
                .to_string(),
            format,
        })
    }
}

#[async_trait]
impl AnnotationSource for NoodlesGffSource {
    async fn fetch(&self, _region: &Region) -> Result<Vec<AnnotationTranscript>> {
        Err(IgvError::Other(format!(
            "NoodlesGffSource::fetch not yet implemented (path={})",
            self.path.display()
        )))
    }

    fn display_name(&self) -> &str {
        &self.display
    }
}
