//! Plain-text / gzipped SAM alignment source (in-memory interval index).

use std::collections::HashMap;
use std::io::BufReader;
use std::sync::Arc;

use async_trait::async_trait;
use iset::IntervalMap;
use noodles::sam;
use noodles::sam::alignment::Record as SamRecordTrait;

use crate::error::{IgvError, Result};
use crate::genome::resolve_chrom_name;
use crate::io::{fetch_bytes, maybe_gunzip, Location};
use crate::region::Region;
use crate::source::bam::{
    parse_tag_name, sam_record_to_alignment_row, AlignmentRow, BamSource, FetchOpts,
};

pub struct SamMemorySource {
    records: Vec<AlignmentRow>,
    trees: HashMap<String, IntervalMap<u64, usize>>,
    chrom_names: Vec<String>,
}

impl std::fmt::Debug for SamMemorySource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SamMemorySource")
            .field("records", &self.records.len())
            .finish_non_exhaustive()
    }
}

impl SamMemorySource {
    pub async fn open_location(
        location: &Location,
        tag_name: Option<&str>,
        auth_token: Option<&str>,
    ) -> Result<Self> {
        let tag = parse_tag_name(tag_name)?;
        let raw = fetch_bytes(location, auth_token).await?;
        let plain = tokio::task::spawn_blocking(move || maybe_gunzip(raw))
            .await
            .map_err(|e| IgvError::Other(e.to_string()))??;

        type SamLoadOutput = (Vec<AlignmentRow>, HashMap<String, IntervalMap<u64, usize>>);
        let (records, trees) = tokio::task::spawn_blocking(
            move || -> Result<SamLoadOutput> {
                let mut reader = sam::io::Reader::new(BufReader::new(&plain[..]));
                let header = reader.read_header().map_err(IgvError::noodles)?;
                let mut records = Vec::new();
                let mut trees: HashMap<String, IntervalMap<u64, usize>> = HashMap::new();
                let all_opts = FetchOpts {
                    include_secondary: true,
                    include_supplementary: true,
                };
                for result in reader.records() {
                    let record = result.map_err(IgvError::noodles)?;
                    let Some(chrom) = record
                        .reference_sequence(&header)
                        .and_then(|r| r.ok())
                        .map(|(n, _)| String::from_utf8_lossy(n.as_ref()).into_owned())
                    else {
                        continue;
                    };
                    if let Some(row) =
                        sam_record_to_alignment_row(&record, &header, tag, &all_opts)?
                    {
                        let idx = records.len();
                        let s = row.ref_start;
                        let e = row.ref_end.max(s).saturating_add(1);
                        records.push(row);
                        trees.entry(chrom).or_default().insert(s..e, idx);
                    }
                }
                Ok((records, trees))
            },
        )
        .await
        .map_err(|e| IgvError::Other(e.to_string()))??;

        let chrom_names = trees.keys().cloned().collect();
        Ok(Self {
            records,
            trees,
            chrom_names,
        })
    }
}

#[async_trait]
impl BamSource for SamMemorySource {
    async fn fetch(&self, region: &Region, opts: &FetchOpts) -> Result<Vec<AlignmentRow>> {
        let Some(chrom) = resolve_chrom_name(&region.chrom, &self.chrom_names) else {
            return Ok(Vec::new());
        };
        let Some(tree) = self.trees.get(chrom) else {
            return Ok(Vec::new());
        };
        let lo = region.start;
        let hi = region.end.saturating_add(1);
        let mut out = Vec::new();
        for (_, &idx) in tree.iter(lo..hi) {
            let row = &self.records[idx];
            if !opts.include_secondary && (row.flag & 0x100 != 0) {
                continue;
            }
            if !opts.include_supplementary && (row.flag & 0x800 != 0) {
                continue;
            }
            out.push(row.clone());
        }
        out.sort_by_key(|r| r.ref_start);
        Ok(out)
    }
}

/// Open any supported alignment format (`.bam`, `.cram`, `.sam`, `.paf`) from a local path or URL.
pub async fn open_alignment(
    spec: &str,
    fasta: Arc<dyn crate::source::FastaSource>,
    tag_name: Option<&str>,
    auth_token: Option<&str>,
) -> Result<Arc<dyn BamSource>> {
    let loc = Location::parse(spec);
    let lower = loc.lowercase_name();
    if lower.ends_with(".cram") {
        let src =
            crate::source::cram::NoodlesCramSource::open_location(&loc, fasta, tag_name, auth_token)
                .await?;
        Ok(Arc::new(src))
    } else if lower.ends_with(".sam") || lower.ends_with(".sam.gz") {
        let src = SamMemorySource::open_location(&loc, tag_name, auth_token).await?;
        Ok(Arc::new(src))
    } else if lower.ends_with(".paf") || lower.ends_with(".paf.gz") {
        let src = crate::source::paf::PafAlignmentSource::open_location(&loc, auth_token).await?;
        Ok(Arc::new(src))
    } else {
        let src =
            crate::source::bam::NoodlesBamSource::open_location(&loc, tag_name, auth_token).await?;
        Ok(Arc::new(src))
    }
}
