//! Async data-source traits and noodles-backed implementations.

pub mod bam;
pub mod fasta;
pub mod vcf;

use async_trait::async_trait;

use crate::error::Result;
use crate::region::Region;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RefMeta {
    pub name: String,
    pub length: u64,
}

#[async_trait]
pub trait FastaSource: Send + Sync {
    async fn references(&self) -> Result<Vec<RefMeta>>;
    async fn fetch(&self, region: &Region) -> Result<Vec<u8>>;
}

pub use fasta::NoodlesFastaSource;
pub use vcf::{NoodlesVcfSource, VariantRecord, VcfSource};
pub use bam::{AlignmentRow, BamSource, FetchOpts, NoodlesBamSource};
