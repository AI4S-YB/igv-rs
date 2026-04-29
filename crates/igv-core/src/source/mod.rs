//! Async data-source traits and noodles-backed implementations.

pub mod annotation;
pub mod signal;
pub mod link;
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
pub use annotation::{
    AnnotationBlock, AnnotationFormat, AnnotationSource, AnnotationTranscript, BlockKind,
    Strand, TranscriptKind, open_annotation,
};
pub use signal::{
    open_signal, FetchSignalOpts, SignalBin, SignalFormat, SignalSource, SignalSummary,
};
pub use link::{
    open_link, FetchLinkOpts, LinkFormat, LinkRecord, LinkScope, LinkSource, VisibleLink,
};
