//! Async data-source traits and format backends.

pub mod annotation;
pub mod bam;
pub mod bcf;
pub mod cram;
pub mod fasta;
pub mod gwas;
pub mod link;
pub mod paf;
pub mod sam;
pub mod seg;
pub mod signal;
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

pub use annotation::{
    open_annotation, open_annotation_location, AnnotationBlock, AnnotationFormat,
    AnnotationSource, AnnotationTranscript, BlockKind, Strand, TranscriptKind,
};
pub use bam::{AlignmentRow, BamSource, FetchOpts, NoodlesBamSource};
pub use bcf::NoodlesBcfSource;
pub use cram::NoodlesCramSource;
pub use fasta::NoodlesFastaSource;
pub use gwas::{FileGwasSource, GwasPoint, GwasSource};
pub use link::{
    open_link, open_link_location, BigInteractLinkSource, FetchLinkOpts, LinkFormat, LinkRecord,
    LinkScope, LinkSource, VisibleLink,
};
pub use paf::PafAlignmentSource;
pub use sam::{open_alignment, SamMemorySource};
pub use seg::{FileSegSource, SegInterval, SegSource, SegTrackData};
pub use signal::{
    open_signal, open_signal_location, FetchSignalOpts, SignalBin, SignalFormat, SignalSource,
    SignalSummary,
};
pub use vcf::{open_variant, GenotypeCall, NoodlesVcfSource, SampleGenotype, VariantRecord, VcfSource};
