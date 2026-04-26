//! placeholder - implemented in Task 2.4.

use async_trait::async_trait;

use crate::error::Result;
use crate::region::Region;

#[derive(Debug, Default, Clone, Copy)]
pub struct FetchOpts {
    pub include_secondary: bool,
    pub include_supplementary: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CigarKind {
    Match,
    Insertion,
    Deletion,
    Skip,
    SoftClip,
    HardClip,
    Padding,
    SeqMatch,
    SeqMismatch,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CigarOp {
    pub kind: CigarKind,
    pub len: u32,
}

#[derive(Debug, Clone)]
pub struct AlignmentRow {
    pub query_name: String,
    pub flag: u16,
    pub ref_start: u64,
    pub ref_end: u64,
    pub mapq: u8,
    pub is_reverse: bool,
    pub query_sequence: Vec<u8>,
    pub cigar: Vec<CigarOp>,
    pub tag: Option<(String, String)>,
}

#[async_trait]
pub trait BamSource: Send + Sync {
    async fn fetch(&self, region: &Region, opts: &FetchOpts) -> Result<Vec<AlignmentRow>>;
}

#[derive(Debug)]
pub struct NoodlesBamSource;
