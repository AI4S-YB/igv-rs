use std::path::{Path, PathBuf};

use async_trait::async_trait;
use noodles::bam;
use noodles::sam::alignment::record::cigar::op::Kind;

use crate::error::{IgvError, Result};
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
    pub ref_start: u64, // 1-based inclusive
    pub ref_end: u64,   // 1-based inclusive
    pub mapq: u8,
    pub is_reverse: bool,
    pub query_sequence: Vec<u8>,
    pub cigar: Vec<CigarOp>,
    pub tag: Option<(String, String)>, // (tag name, value as string)
}

#[async_trait]
pub trait BamSource: Send + Sync {
    async fn fetch(&self, region: &Region, opts: &FetchOpts) -> Result<Vec<AlignmentRow>>;
}

/// Note: noodles' `bam::io::IndexedReader` stores a `Box<dyn BinningIndex>` which
/// is not `Send`, so we cannot stash it inside an `Arc<Mutex<...>>` shared across
/// `spawn_blocking` boundaries. Instead, we cache the path and re-open per fetch.
#[derive(Debug, Clone)]
pub struct NoodlesBamSource {
    path: PathBuf,
    tag_name: Option<[u8; 2]>,
}

impl NoodlesBamSource {
    pub async fn open(path: impl AsRef<Path>, tag_name: Option<&str>) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        let p = path.clone();
        let tag = match tag_name {
            Some(t) if t.len() == 2 => {
                let bytes = t.as_bytes();
                Some([bytes[0], bytes[1]])
            }
            None => None,
            Some(other) => return Err(IgvError::Other(format!("BAM tag must be 2 chars: {other}"))),
        };
        // Validate by opening and reading the header.
        tokio::task::spawn_blocking(move || -> Result<()> {
            let mut reader = bam::io::indexed_reader::Builder::default()
                .build_from_path(&p)
                .map_err(|e| IgvError::io(p.clone(), e))?;
            reader.read_header().map_err(IgvError::noodles)?;
            Ok(())
        })
        .await
        .map_err(|e| IgvError::Other(e.to_string()))??;
        Ok(Self {
            path,
            tag_name: tag,
        })
    }
}

fn cigar_kind_from(kind: Kind) -> CigarKind {
    match kind {
        Kind::Match => CigarKind::Match,
        Kind::Insertion => CigarKind::Insertion,
        Kind::Deletion => CigarKind::Deletion,
        Kind::Skip => CigarKind::Skip,
        Kind::SoftClip => CigarKind::SoftClip,
        Kind::HardClip => CigarKind::HardClip,
        Kind::Pad => CigarKind::Padding,
        Kind::SequenceMatch => CigarKind::SeqMatch,
        Kind::SequenceMismatch => CigarKind::SeqMismatch,
    }
}

#[async_trait]
impl BamSource for NoodlesBamSource {
    async fn fetch(&self, region: &Region, opts: &FetchOpts) -> Result<Vec<AlignmentRow>> {
        let path = self.path.clone();
        let region = region.clone();
        let opts = *opts;
        let tag_name = self.tag_name;

        tokio::task::spawn_blocking(move || -> Result<Vec<AlignmentRow>> {
            let mut reader = bam::io::indexed_reader::Builder::default()
                .build_from_path(&path)
                .map_err(|e| IgvError::io(path.clone(), e))?;
            let header = reader.read_header().map_err(IgvError::noodles)?;

            let region_str = format!("{}:{}-{}", region.chrom, region.start, region.end);
            let r: noodles::core::Region = region_str
                .parse()
                .map_err(|_| IgvError::InvalidRegion(region_str.clone()))?;

            let mut out = Vec::new();
            for result in reader.query(&header, &r).map_err(IgvError::noodles)? {
                let record = result.map_err(IgvError::noodles)?;
                let flags = record.flags();
                let flag: u16 = u16::from(flags);
                if flags.is_unmapped() {
                    continue;
                }
                if !opts.include_secondary && flags.is_secondary() {
                    continue;
                }
                if !opts.include_supplementary && flags.is_supplementary() {
                    continue;
                }

                let query_name = record
                    .name()
                    .map(|n| std::str::from_utf8(n.as_ref()).unwrap_or("").to_string())
                    .unwrap_or_default();

                let ref_start_i32 = record
                    .alignment_start()
                    .ok_or_else(|| IgvError::Other("missing alignment start".into()))?
                    .map_err(IgvError::noodles)?
                    .get();
                let cigar: Vec<CigarOp> = record
                    .cigar()
                    .iter()
                    .map(|op| {
                        let op = op.map_err(IgvError::noodles)?;
                        Ok(CigarOp {
                            kind: cigar_kind_from(op.kind()),
                            len: op.len() as u32,
                        })
                    })
                    .collect::<Result<Vec<_>>>()?;

                let mapq = record.mapping_quality().map(|m| m.get()).unwrap_or(0);
                let is_reverse = flags.is_reverse_complemented();

                let query_sequence = record.sequence().iter().collect::<Vec<u8>>();

                // Span on reference = sum of consuming ops.
                let ref_consuming: u32 = cigar
                    .iter()
                    .filter(|op| {
                        matches!(
                            op.kind,
                            CigarKind::Match
                                | CigarKind::Deletion
                                | CigarKind::Skip
                                | CigarKind::SeqMatch
                                | CigarKind::SeqMismatch
                        )
                    })
                    .map(|op| op.len)
                    .sum();
                let ref_start = ref_start_i32 as u64;
                let ref_end = ref_start + ref_consuming.saturating_sub(1) as u64;

                let tag = tag_name.and_then(|name| {
                    let data = record.data();
                    data.get(&name).and_then(|r| r.ok()).map(|v| {
                        let key = std::str::from_utf8(&name).unwrap_or("").to_string();
                        (key, format!("{:?}", v))
                    })
                });

                out.push(AlignmentRow {
                    query_name,
                    flag,
                    ref_start,
                    ref_end,
                    mapq,
                    is_reverse,
                    query_sequence,
                    cigar,
                    tag,
                });
            }
            Ok(out)
        })
        .await
        .map_err(|e| IgvError::Other(e.to_string()))?
    }
}
