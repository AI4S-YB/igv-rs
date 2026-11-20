//! BAM alignment source + shared `sam::alignment::Record` → `AlignmentRow`
//! extraction (reused by CRAM and SAM sources).

use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use async_trait::async_trait;
use noodles::bam;
use noodles::csi as csi;
use noodles::sam;
use noodles::sam::alignment::record::cigar::op::Kind;
use noodles::sam::alignment::record::data::field::{value::Array, Tag, Value};
use noodles::sam::alignment::Record as SamRecordTrait;

use crate::alignment::mods::{parse_mm_ml, BaseModCall};
use crate::error::{IgvError, Result};
use crate::genome::resolve_chrom_name;
use crate::io::{fetch_bytes, location_exists, Location, SeekableStream};
use crate::region::Region;
use crate::source::annotation::Strand;

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

#[derive(Debug, Clone, Default)]
pub struct AlignmentRow {
    pub query_name: String,
    pub flag: u16,
    pub ref_start: u64, // 1-based inclusive
    pub ref_end: u64,   // 1-based inclusive
    pub mapq: u8,
    pub is_reverse: bool,
    pub query_sequence: Vec<u8>,
    pub base_qualities: Vec<u8>,
    pub cigar: Vec<CigarOp>,
    pub tag: Option<(String, String)>, // (tag name, cleanly formatted value)
    pub mate_chrom: Option<String>,
    pub mate_pos: Option<u64>,
    pub template_len: i32,
    pub read_group: Option<String>,
    pub haplotype: Option<u32>,
    pub sa_tag: Option<String>,
    pub xs_strand: Option<Strand>,
    pub base_mods: Vec<BaseModCall>,
}

#[async_trait]
pub trait BamSource: Send + Sync {
    async fn fetch(&self, region: &Region, opts: &FetchOpts) -> Result<Vec<AlignmentRow>>;
}

#[derive(Debug, Clone)]
enum CachedBamIndex {
    Bai(bam::bai::Index),
    Csi(csi::Index),
}

/// Indexed BAM reader supporting local files and remote HTTP/S3/GS streams,
/// caching both the SAM header and the `.bai`/`.csi` index across fetches.
#[derive(Debug)]
pub struct NoodlesBamSource {
    location: Location,
    remote_stream: Option<SeekableStream>,
    header: Arc<sam::Header>,
    ref_names: Vec<String>,
    index: Arc<CachedBamIndex>,
    tag_name: Option<[u8; 2]>,
}

impl NoodlesBamSource {
    pub async fn open(path: impl AsRef<Path>, tag_name: Option<&str>) -> Result<Self> {
        let loc = Location::Local(path.as_ref().to_path_buf());
        Self::open_location(&loc, tag_name, None).await
    }

    pub async fn open_location(
        location: &Location,
        tag_name: Option<&str>,
        auth_token: Option<&str>,
    ) -> Result<Self> {
        let tag = parse_tag_name(tag_name)?;
        let loc_clone = location.clone();

        // Open stream (local or remote HTTP range) and fetch index bytes.
        let stream = SeekableStream::open(location, auth_token).await?;
        let bai_loc = location.sibling("bai");
        let csi_loc = location.sibling("csi");

        let index = if location_exists(&bai_loc, auth_token).await {
            let bytes = fetch_bytes(&bai_loc, auth_token).await?;
            let bai = tokio::task::spawn_blocking(move || {
                let mut reader = bam::bai::io::Reader::new(Cursor::new(bytes));
                reader.read_index().map_err(IgvError::noodles)
            })
            .await
            .map_err(|e| IgvError::Other(e.to_string()))??;
            CachedBamIndex::Bai(bai)
        } else if location_exists(&csi_loc, auth_token).await {
            let bytes = fetch_bytes(&csi_loc, auth_token).await?;
            let idx = tokio::task::spawn_blocking(move || {
                let mut reader = csi::io::Reader::new(Cursor::new(bytes));
                reader.read_index().map_err(IgvError::noodles)
            })
            .await
            .map_err(|e| IgvError::Other(e.to_string()))??;
            CachedBamIndex::Csi(idx)
        } else {
            return Err(IgvError::MissingIndex(PathBuf::from(bai_loc.as_str())));
        };

        let is_remote = location.is_remote();
        let remote_stream = if is_remote {
            Some(
                stream
                    .clone_handle()
                    .map_err(|e| IgvError::Other(e.to_string()))?,
            )
        } else {
            None
        };

        let header = tokio::task::spawn_blocking(move || -> Result<sam::Header> {
            let mut reader = bam::io::Reader::new(stream);
            reader.read_header().map_err(IgvError::noodles)
        })
        .await
        .map_err(|e| IgvError::Other(e.to_string()))??;

        let ref_names = header
            .reference_sequences()
            .keys()
            .map(|k| String::from_utf8_lossy(k.as_ref()).into_owned())
            .collect();

        Ok(Self {
            location: loc_clone,
            remote_stream,
            header: Arc::new(header),
            ref_names,
            index: Arc::new(index),
            tag_name: tag,
        })
    }
}

pub(crate) fn parse_tag_name(tag_name: Option<&str>) -> Result<Option<[u8; 2]>> {
    match tag_name {
        Some(t) if t.len() == 2 => {
            let bytes = t.as_bytes();
            Ok(Some([bytes[0], bytes[1]]))
        }
        None => Ok(None),
        Some(other) => Err(IgvError::Other(format!("BAM tag must be 2 chars: {other}"))),
    }
}

pub(crate) fn cigar_kind_from(kind: Kind) -> CigarKind {
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

/// Format SAM/BAM auxiliary tag values cleanly without Rust `Debug` wrappers.
pub(crate) fn format_tag_value(v: &Value<'_>) -> String {
    match v {
        Value::Character(c) => (*c as char).to_string(),
        Value::Int8(n) => n.to_string(),
        Value::UInt8(n) => n.to_string(),
        Value::Int16(n) => n.to_string(),
        Value::UInt16(n) => n.to_string(),
        Value::Int32(n) => n.to_string(),
        Value::UInt32(n) => n.to_string(),
        Value::Float(f) => format!("{f:.3}"),
        Value::String(s) => String::from_utf8_lossy(s.as_ref()).into_owned(),
        Value::Hex(s) => String::from_utf8_lossy(s.as_ref()).into_owned(),
        Value::Array(arr) => match arr {
            Array::Int8(vs) => join_nums(vs.iter()),
            Array::UInt8(vs) => join_nums(vs.iter()),
            Array::Int16(vs) => join_nums(vs.iter()),
            Array::UInt16(vs) => join_nums(vs.iter()),
            Array::Int32(vs) => join_nums(vs.iter()),
            Array::UInt32(vs) => join_nums(vs.iter()),
            Array::Float(vs) => join_nums(vs.iter()),
        },
    }
}

fn join_nums<T: std::fmt::Display>(it: Box<dyn Iterator<Item = std::io::Result<T>> + '_>) -> String {
    it.filter_map(|r| r.ok())
        .take(16)
        .map(|v| v.to_string())
        .collect::<Vec<_>>()
        .join(",")
}

/// Extract an `AlignmentRow` from any `noodles::sam::alignment::Record` (BAM, CRAM, or SAM).
pub(crate) fn sam_record_to_alignment_row<R: SamRecordTrait>(
    record: &R,
    header: &sam::Header,
    tag_name: Option<[u8; 2]>,
    opts: &FetchOpts,
) -> Result<Option<AlignmentRow>> {
    let flags = record.flags().map_err(IgvError::noodles)?;
    let flag: u16 = u16::from(flags);
    if flags.is_unmapped() {
        return Ok(None);
    }
    if !opts.include_secondary && flags.is_secondary() {
        return Ok(None);
    }
    if !opts.include_supplementary && flags.is_supplementary() {
        return Ok(None);
    }

    let query_name = record
        .name()
        .map(|n| String::from_utf8_lossy(n.as_ref()).into_owned())
        .unwrap_or_default();

    let Some(start_res) = record.alignment_start() else {
        return Ok(None);
    };
    let ref_start = start_res.map_err(IgvError::noodles)?.get() as u64;

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

    let mapq = record
        .mapping_quality()
        .and_then(|r| r.ok())
        .map(|m| m.get())
        .unwrap_or(0);
    let is_reverse = flags.is_reverse_complemented();

    let query_sequence = record.sequence().iter().collect::<Vec<u8>>();
    let base_qualities = record
        .quality_scores()
        .iter()
        .filter_map(|q| q.ok())
        .collect::<Vec<u8>>();

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
    let ref_end = ref_start + ref_consuming.saturating_sub(1) as u64;

    let mate_chrom = record
        .mate_reference_sequence(header)
        .and_then(|r| r.ok())
        .map(|(name, _)| String::from_utf8_lossy(name.as_ref()).into_owned());
    let mate_pos = record
        .mate_alignment_start()
        .and_then(|r| r.ok())
        .map(|p| p.get() as u64);
    let template_len = record.template_length().unwrap_or(0);

    let data = record.data();
    let tag = tag_name.and_then(|name| {
        data.get(&Tag::from(name)).and_then(|r| r.ok()).map(|v| {
            let key = String::from_utf8_lossy(&name).into_owned();
            (key, format_tag_value(&v))
        })
    });
    let read_group = data
        .get(&Tag::new(b'R', b'G'))
        .and_then(|r| r.ok())
        .map(|v| format_tag_value(&v));
    let haplotype = data
        .get(&Tag::new(b'H', b'P'))
        .and_then(|r| r.ok())
        .and_then(|v| v.as_int())
        .and_then(|n| u32::try_from(n).ok());
    let sa_tag = data
        .get(&Tag::new(b'S', b'A'))
        .and_then(|r| r.ok())
        .map(|v| format_tag_value(&v));
    let xs_strand = data
        .get(&Tag::new(b'X', b'S'))
        .or_else(|| data.get(&Tag::new(b't', b's')))
        .and_then(|r| r.ok())
        .and_then(|v| match v {
            Value::Character(b'+') => Some(Strand::Forward),
            Value::Character(b'-') => Some(Strand::Reverse),
            Value::String(s) if AsRef::<[u8]>::as_ref(s) == b"+" => Some(Strand::Forward),
            Value::String(s) if AsRef::<[u8]>::as_ref(s) == b"-" => Some(Strand::Reverse),
            _ => None,
        });

    let mm_str = data
        .get(&Tag::new(b'M', b'M'))
        .or_else(|| data.get(&Tag::new(b'M', b'm')))
        .and_then(|r| r.ok())
        .map(|v| format_tag_value(&v));
    let ml_bytes: Option<Vec<u8>> = data
        .get(&Tag::new(b'M', b'L'))
        .or_else(|| data.get(&Tag::new(b'M', b'l')))
        .and_then(|r| r.ok())
        .and_then(|v| match v {
            Value::Array(Array::UInt8(vals)) => {
                Some(vals.iter().filter_map(|x| x.ok()).collect())
            }
            _ => None,
        });
    let base_mods = match mm_str {
        Some(mm) => parse_mm_ml(&mm, ml_bytes.as_deref(), &query_sequence, is_reverse),
        None => Vec::new(),
    };

    Ok(Some(AlignmentRow {
        query_name,
        flag,
        ref_start,
        ref_end,
        mapq,
        is_reverse,
        query_sequence,
        base_qualities,
        cigar,
        tag,
        mate_chrom,
        mate_pos,
        template_len,
        read_group,
        haplotype,
        sa_tag,
        xs_strand,
        base_mods,
    }))
}

#[async_trait]
impl BamSource for NoodlesBamSource {
    async fn fetch(&self, region: &Region, opts: &FetchOpts) -> Result<Vec<AlignmentRow>> {
        let resolved_chrom = match resolve_chrom_name(&region.chrom, &self.ref_names) {
            Some(c) => c.to_string(),
            None => return Ok(Vec::new()),
        };

        let stream = match (&self.location, &self.remote_stream) {
            (Location::Local(p), _) => SeekableStream::open_local(p)?,
            (Location::Remote(_), Some(rs)) => rs
                .clone_handle()
                .map_err(|e| IgvError::Other(e.to_string()))?,
            (Location::Remote(u), None) => {
                return Err(IgvError::Other(format!("no remote stream for {u}")));
            }
        };

        let header = Arc::clone(&self.header);
        let index = Arc::clone(&self.index);
        let start = region.start;
        let end = region.end;
        let opts = *opts;
        let tag_name = self.tag_name;

        tokio::task::spawn_blocking(move || -> Result<Vec<AlignmentRow>> {
            let region_str = format!("{resolved_chrom}:{start}-{end}");
            let r: noodles::core::Region = region_str
                .parse()
                .map_err(|_| IgvError::InvalidRegion(region_str.clone()))?;

            let mut reader = match index.as_ref() {
                CachedBamIndex::Bai(bai) => {
                    bam::io::indexed_reader::Builder::default()
                        .set_index(bai.clone())
                        .build_from_reader(stream)
                        .map_err(IgvError::noodles)?
                }
                CachedBamIndex::Csi(idx) => {
                    bam::io::indexed_reader::Builder::default()
                        .set_index(idx.clone())
                        .build_from_reader(stream)
                        .map_err(IgvError::noodles)?
                }
            };

            let mut out = Vec::new();
            for result in reader.query(&header, &r).map_err(IgvError::noodles)? {
                let record = result.map_err(IgvError::noodles)?;
                if let Some(row) = sam_record_to_alignment_row(&record, &header, tag_name, &opts)? {
                    out.push(row);
                }
            }
            Ok(out)
        })
        .await
        .map_err(|e| IgvError::Other(e.to_string()))?
    }
}
