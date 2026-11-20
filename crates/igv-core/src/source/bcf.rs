//! BCF binary variant source (indexed `.bcf` + `.csi` or sequential fallback).

use std::collections::HashMap;
use std::io::{Cursor, Read, Seek, SeekFrom};
use std::sync::Arc;

use async_trait::async_trait;
use iset::IntervalMap;
use noodles::bcf;
use noodles::csi as csi;
use noodles::vcf;

use crate::error::{IgvError, Result};
use crate::genome::resolve_chrom_name;
use crate::io::{fetch_bytes, location_exists, Location, SeekableStream};
use crate::pedigree::Pedigree;
use crate::region::Region;
use crate::source::vcf::{
    extract_variant_record, reorder_sample_names, VariantRecord, VcfSource,
};

#[derive(Debug)]
pub struct NoodlesBcfSource {
    display: String,
    sample_names: Vec<String>,
    backend: BcfBackend,
    pedigree: Option<Arc<Pedigree>>,
}

#[derive(Debug)]
enum BcfBackend {
    Indexed {
        location: Location,
        remote_stream: Option<SeekableStream>,
        header: Arc<vcf::Header>,
        index: Arc<csi::Index>,
        chrom_names: Vec<String>,
    },
    InMemory {
        records: Vec<VariantRecord>,
        trees: HashMap<String, IntervalMap<u64, usize>>,
        chrom_names: Vec<String>,
    },
}

impl NoodlesBcfSource {
    pub async fn open_location(
        location: &Location,
        pedigree: Option<Arc<Pedigree>>,
        auth_token: Option<&str>,
    ) -> Result<Self> {
        let display = location.display_name();
        let csi_loc = location.sibling("csi");

        if location_exists(&csi_loc, auth_token).await {
            let bytes = fetch_bytes(&csi_loc, auth_token).await?;
            let index = tokio::task::spawn_blocking(move || {
                let mut r = csi::io::Reader::new(Cursor::new(bytes));
                r.read_index().map_err(IgvError::noodles)
            })
            .await
            .map_err(|e| IgvError::Other(e.to_string()))??;

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

            let header = tokio::task::spawn_blocking(move || -> Result<vcf::Header> {
                let mut reader = bcf::io::Reader::new(stream);
                reader.read_header().map_err(IgvError::noodles)
            })
            .await
            .map_err(|e| IgvError::Other(e.to_string()))??;

            let raw_samples: Vec<String> = header.sample_names().iter().cloned().collect();
            let sample_names = reorder_sample_names(&raw_samples, pedigree.as_deref());
            let chrom_names = header.contigs().keys().cloned().collect();

            return Ok(Self {
                display,
                sample_names,
                backend: BcfBackend::Indexed {
                    location: location.clone(),
                    remote_stream,
                    header: Arc::new(header),
                    index: Arc::new(index),
                    chrom_names,
                },
                pedigree,
            });
        }

        // Fallback for unindexed BCF or .bcf.idx (Tribble) files: sequential scan
        let mut stream = SeekableStream::open(location, auth_token).await?;
        let ped_clone = pedigree.clone();
        let (sample_names, records, trees) = tokio::task::spawn_blocking(
            move || -> Result<(Vec<String>, Vec<VariantRecord>, HashMap<String, IntervalMap<u64, usize>>)> {
                let mut magic = [0u8; 2];
                let is_bgzf = stream.read_exact(&mut magic).is_ok() && magic == [0x1f, 0x8b];
                stream
                    .seek(SeekFrom::Start(0))
                    .map_err(|e| IgvError::Other(e.to_string()))?;
                if is_bgzf {
                    let mut reader = bcf::io::Reader::new(stream);
                    read_all_bcf_records(&mut reader, ped_clone.as_deref())
                } else {
                    let mut reader = bcf::io::Reader::from(stream);
                    read_all_bcf_records(&mut reader, ped_clone.as_deref())
                }
            },
        )
        .await
        .map_err(|e| IgvError::Other(e.to_string()))??;

        let chrom_names = trees.keys().cloned().collect();
        Ok(Self {
            display,
            sample_names,
            backend: BcfBackend::InMemory {
                records,
                trees,
                chrom_names,
            },
            pedigree,
        })
    }
}

#[async_trait]
impl VcfSource for NoodlesBcfSource {
    async fn fetch(&self, region: &Region) -> Result<Vec<VariantRecord>> {
        match &self.backend {
            BcfBackend::InMemory {
                records,
                trees,
                chrom_names,
            } => {
                let Some(chrom) = resolve_chrom_name(&region.chrom, chrom_names) else {
                    return Ok(Vec::new());
                };
                let Some(tree) = trees.get(chrom) else {
                    return Ok(Vec::new());
                };
                let lo = region.start;
                let hi = region.end.saturating_add(1);
                let mut out: Vec<VariantRecord> = tree
                    .iter(lo..hi)
                    .map(|(_, &idx)| records[idx].clone())
                    .collect();
                out.sort_by_key(|v| v.pos);
                Ok(out)
            }
            BcfBackend::Indexed {
                location,
                remote_stream,
                header,
                index,
                chrom_names,
            } => {
                let Some(resolved) = resolve_chrom_name(&region.chrom, chrom_names) else {
                    return Ok(Vec::new());
                };
                let resolved = resolved.to_string();
                let stream = match (location, remote_stream) {
                    (Location::Local(p), _) => SeekableStream::open_local(p)?,
                    (Location::Remote(_), Some(rs)) => rs
                        .clone_handle()
                        .map_err(|e| IgvError::Other(e.to_string()))?,
                    (Location::Remote(u), None) => {
                        return Err(IgvError::Other(format!("no remote stream for {u}")));
                    }
                };
                let header = Arc::clone(header);
                let index = Arc::clone(index);
                let raw_samples: Vec<String> = header.sample_names().iter().cloned().collect();
                let pedigree = self.pedigree.clone();
                let region_str = format!("{resolved}:{}-{}", region.start, region.end);

                tokio::task::spawn_blocking(move || -> Result<Vec<VariantRecord>> {
                    let r: noodles::core::Region = region_str
                        .parse()
                        .map_err(|_| IgvError::InvalidRegion(region_str.clone()))?;
                    let mut reader = bcf::io::indexed_reader::Builder::default()
                        .set_index((*index).clone())
                        .build_from_reader(stream)
                        .map_err(IgvError::noodles)?;
                    let mut out = Vec::new();
                    for result in reader.query(&header, &r).map_err(IgvError::noodles)? {
                        let rec = result.map_err(IgvError::noodles)?;
                        if let Some(vr) = extract_variant_record(
                            &rec,
                            &header,
                            &raw_samples,
                            pedigree.as_deref(),
                        )? {
                            out.push(vr);
                        }
                    }
                    Ok(out)
                })
                .await
                .map_err(|e| IgvError::Other(e.to_string()))?
            }
        }
    }

    fn sample_names(&self) -> Vec<String> {
        self.sample_names.clone()
    }

    fn display_name(&self) -> &str {
        &self.display
    }
}

fn read_all_bcf_records<R: std::io::Read>(
    reader: &mut bcf::io::Reader<R>,
    pedigree: Option<&Pedigree>,
) -> Result<(
    Vec<String>,
    Vec<VariantRecord>,
    HashMap<String, IntervalMap<u64, usize>>,
)> {
    let header = reader.read_header().map_err(IgvError::noodles)?;
    let raw_samples: Vec<String> = header.sample_names().iter().cloned().collect();
    let ordered = reorder_sample_names(&raw_samples, pedigree);
    let mut records = Vec::new();
    let mut trees: HashMap<String, IntervalMap<u64, usize>> = HashMap::new();
    for result in reader.records() {
        let rec = result.map_err(IgvError::noodles)?;
        if let Some(vr) = extract_variant_record(&rec, &header, &raw_samples, pedigree)? {
            let start = vr.pos;
            let end = start + vr.reference_allele.len().max(1) as u64;
            let chrom = vr.chrom.clone();
            let idx = records.len();
            records.push(vr);
            trees.entry(chrom).or_default().insert(start..end, idx);
        }
    }
    Ok((ordered, records, trees))
}
