//! CRAM alignment source backed by `noodles-cram` and our `FastaSource`.

use std::collections::HashMap;
use std::io::Cursor;
use std::sync::Arc;

use async_trait::async_trait;
use noodles::cram::{self, crai};
use noodles::fasta as nfasta;
use noodles::sam;

use crate::error::{IgvError, Result};
use crate::genome::resolve_chrom_name;
use crate::io::{fetch_bytes, location_exists, Location, SeekableStream};
use crate::region::Region;
use crate::source::bam::{parse_tag_name, sam_record_to_alignment_row, AlignmentRow, BamSource, FetchOpts};
use crate::source::FastaSource;

struct FastaRepoAdapter {
    fasta: Arc<dyn FastaSource>,
    handle: tokio::runtime::Handle,
    lengths: HashMap<String, u64>,
    ref_names: Vec<String>,
}

impl nfasta::repository::Adapter for FastaRepoAdapter {
    fn get(&mut self, name: &[u8]) -> Option<std::io::Result<nfasta::Record>> {
        let req_name = std::str::from_utf8(name).ok()?;
        let resolved = resolve_chrom_name(req_name, &self.ref_names)
            .unwrap_or(req_name)
            .to_string();
        let len = self.lengths.get(&resolved).copied().filter(|&l| l > 0)?;
        let region = Region::new(resolved, 1, len).ok()?;
        let res = self
            .handle
            .block_on(self.fasta.fetch(&region))
            .map_err(|e| std::io::Error::other(e.to_string()));
        Some(res.map(|bytes| {
            nfasta::Record::new(
                nfasta::record::Definition::new(req_name.to_string(), None),
                nfasta::record::Sequence::from(bytes),
            )
        }))
    }
}

/// CRAM source supporting `.crai`-indexed random access (local or remote)
/// with automatic fallback to sequential container scan when `.crai` is absent.
#[derive(Debug)]
pub struct NoodlesCramSource {
    location: Location,
    remote_stream: Option<SeekableStream>,
    header: Arc<sam::Header>,
    ref_names: Vec<String>,
    crai: Option<Arc<crai::Index>>,
    repo: nfasta::Repository,
    tag_name: Option<[u8; 2]>,
}

impl NoodlesCramSource {
    pub async fn open_location(
        location: &Location,
        fasta: Arc<dyn FastaSource>,
        tag_name: Option<&str>,
        auth_token: Option<&str>,
    ) -> Result<Self> {
        let tag = parse_tag_name(tag_name)?;
        let refs = fasta.references().await?;
        let lengths: HashMap<String, u64> = refs.iter().map(|r| (r.name.clone(), r.length)).collect();
        let ref_names_fa: Vec<String> = refs.into_iter().map(|r| r.name).collect();
        let handle = tokio::runtime::Handle::current();

        let repo = nfasta::Repository::new(FastaRepoAdapter {
            fasta,
            handle,
            lengths,
            ref_names: ref_names_fa,
        });

        let crai_loc = location.sibling("crai");
        let crai = if location_exists(&crai_loc, auth_token).await {
            let bytes = fetch_bytes(&crai_loc, auth_token).await?;
            let idx = tokio::task::spawn_blocking(move || {
                let mut reader = crai::Reader::new(Cursor::new(bytes));
                reader.read_index().map_err(IgvError::noodles)
            })
            .await
            .map_err(|e| IgvError::Other(e.to_string()))??;
            Some(Arc::new(idx))
        } else {
            None
        };

        let stream = SeekableStream::open(location, auth_token).await?;
        let remote_stream = if location.is_remote() {
            Some(
                stream
                    .clone_handle()
                    .map_err(|e| IgvError::Other(e.to_string()))?,
            )
        } else {
            None
        };

        let header = tokio::task::spawn_blocking(move || -> Result<sam::Header> {
            let mut reader = cram::io::Reader::new(stream);
            let def = reader.read_file_definition().map_err(IgvError::noodles)?;
            if def.version().major() < 3 {
                return Err(IgvError::Other(format!(
                    "CRAM {}.{} is unsupported (requires CRAM 3.0+)",
                    def.version().major(),
                    def.version().minor()
                )));
            }
            reader.read_file_header().map_err(IgvError::noodles)
        })
        .await
        .map_err(|e| IgvError::Other(e.to_string()))??;

        let ref_names = header
            .reference_sequences()
            .keys()
            .map(|k| String::from_utf8_lossy(k.as_ref()).into_owned())
            .collect();

        Ok(Self {
            location: location.clone(),
            remote_stream,
            header: Arc::new(header),
            ref_names,
            crai,
            repo,
            tag_name: tag,
        })
    }
}

#[async_trait]
impl BamSource for NoodlesCramSource {
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
        let crai = self.crai.clone();
        let repo = self.repo.clone();
        let start = region.start;
        let end = region.end;
        let opts = *opts;
        let tag_name = self.tag_name;

        tokio::task::spawn_blocking(move || -> Result<Vec<AlignmentRow>> {
            let region_str = format!("{resolved_chrom}:{start}-{end}");
            let r: noodles::core::Region = region_str
                .parse()
                .map_err(|_| IgvError::InvalidRegion(region_str.clone()))?;

            let mut out = Vec::new();
            if let Some(crai_idx) = crai {
                let mut reader = cram::io::indexed_reader::Builder::default()
                    .set_reference_sequence_repository(repo)
                    .set_index((*crai_idx).clone())
                    .build_from_reader(stream)
                    .map_err(IgvError::noodles)?;
                for result in reader.query(&header, &r).map_err(IgvError::noodles)? {
                    let record = result.map_err(IgvError::noodles)?;
                    if let Some(row) =
                        sam_record_to_alignment_row(&record, &header, tag_name, &opts)?
                    {
                        out.push(row);
                    }
                }
            } else {
                // Fallback when .crai is absent: sequential scan and interval filter
                let target_id = header
                    .reference_sequences()
                    .get_index_of(resolved_chrom.as_bytes());
                let Some(tid) = target_id else {
                    return Ok(Vec::new());
                };
                let mut reader = cram::io::reader::Builder::default()
                    .set_reference_sequence_repository(repo)
                    .build_from_reader(stream);
                let _ = reader.read_header().map_err(IgvError::noodles)?;
                for result in reader.records(&header) {
                    let record = result.map_err(IgvError::noodles)?;
                    if record.reference_sequence_id() != Some(tid) {
                        continue;
                    }
                    if let Some(row) =
                        sam_record_to_alignment_row(&record, &header, tag_name, &opts)?
                    {
                        if row.ref_end >= start && row.ref_start <= end {
                            out.push(row);
                        }
                    }
                }
            }
            Ok(out)
        })
        .await
        .map_err(|e| IgvError::Other(e.to_string()))?
    }
}
