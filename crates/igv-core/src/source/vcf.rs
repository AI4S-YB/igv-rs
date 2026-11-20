//! VCF variant source (indexed `.vcf.gz` with cached `.tbi`/`.csi`, plus
//! automatic fallback to unindexed `.vcf`/`.vcf.gz` in-memory parsing),
//! full per-sample genotype extraction, and `PED` Mendelian violation checking.

use std::collections::HashMap;
use std::io::{BufReader, Cursor};
use std::path::Path;
use std::sync::Arc;

use async_trait::async_trait;
use iset::IntervalMap;
use noodles::bgzf;
use noodles::csi::{self as csi, BinningIndex};
use noodles::tabix;
use noodles::vcf;
use noodles::vcf::variant::record::info::field::Value as InfoValue;
use noodles::vcf::variant::record::samples::series::value::genotype::Phasing;
use noodles::vcf::variant::record::samples::series::Value as SampleValue;
use noodles::vcf::variant::record::Record as VariantRecordTrait;

use crate::error::{IgvError, Result};
use crate::genome::resolve_chrom_name;
use crate::io::{fetch_bytes, location_exists, maybe_gunzip, Location, SeekableStream};
use crate::pedigree::Pedigree;
use crate::region::Region;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GenotypeCall {
    HomRef,
    Het,
    HomAlt,
    OtherAlt,
    #[default]
    NoCall,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SampleGenotype {
    pub sample: String,
    pub call: GenotypeCall,
    pub alleles: Vec<Option<usize>>,
    pub phased: bool,
    pub dp: Option<u32>,
    pub gq: Option<u32>,
    pub ad: Option<(u32, u32)>,
    pub mendelian_violation: bool,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct VariantRecord {
    pub chrom: String,
    pub pos: u64, // 1-based
    pub id: Option<String>,
    pub reference_allele: String,
    pub alternate_alleles: Vec<String>,
    pub quality: Option<f32>,
    pub passes_filter: bool,
    pub allele_freq: Option<f32>,
    pub info_raw: String,
    pub genotypes: Vec<SampleGenotype>,
}

#[async_trait]
pub trait VcfSource: Send + Sync {
    async fn fetch(&self, region: &Region) -> Result<Vec<VariantRecord>>;
    fn sample_names(&self) -> Vec<String> {
        Vec::new()
    }
    fn display_name(&self) -> &str {
        "variants"
    }
}

#[derive(Debug, Clone)]
enum CachedVcfIndex {
    Tabix(tabix::Index),
    Csi(csi::Index),
}

/// VCF source supporting indexed `.vcf.gz` (local or remote) and unindexed `.vcf`/`.vcf.gz`.
#[derive(Debug)]
pub struct NoodlesVcfSource {
    display: String,
    sample_names: Vec<String>,
    backend: VcfBackend,
    pedigree: Option<Arc<Pedigree>>,
}

#[derive(Debug)]
enum VcfBackend {
    Indexed {
        location: Location,
        remote_stream: Option<SeekableStream>,
        header: Arc<vcf::Header>,
        index: Arc<CachedVcfIndex>,
        chrom_names: Vec<String>,
    },
    InMemory {
        records: Vec<VariantRecord>,
        trees: HashMap<String, IntervalMap<u64, usize>>,
        chrom_names: Vec<String>,
    },
}

impl NoodlesVcfSource {
    pub async fn open(path: impl AsRef<Path>) -> Result<Self> {
        let loc = Location::Local(path.as_ref().to_path_buf());
        Self::open_location(&loc, None, None).await
    }

    pub async fn open_location(
        location: &Location,
        pedigree: Option<Arc<Pedigree>>,
        auth_token: Option<&str>,
    ) -> Result<Self> {
        let display = location.display_name();
        let tbi_loc = location.sibling("tbi");
        let csi_loc = location.sibling("csi");

        let index_opt = if location_exists(&tbi_loc, auth_token).await {
            let bytes = fetch_bytes(&tbi_loc, auth_token).await?;
            let idx = tokio::task::spawn_blocking(move || {
                let mut r = tabix::io::Reader::new(Cursor::new(bytes));
                r.read_index().map_err(IgvError::noodles)
            })
            .await
            .map_err(|e| IgvError::Other(e.to_string()))??;
            Some(CachedVcfIndex::Tabix(idx))
        } else if location_exists(&csi_loc, auth_token).await {
            let bytes = fetch_bytes(&csi_loc, auth_token).await?;
            let idx = tokio::task::spawn_blocking(move || {
                let mut r = csi::io::Reader::new(Cursor::new(bytes));
                r.read_index().map_err(IgvError::noodles)
            })
            .await
            .map_err(|e| IgvError::Other(e.to_string()))??;
            Some(CachedVcfIndex::Csi(idx))
        } else {
            None
        };

        if let Some(index) = index_opt {
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
                let mut reader = vcf::io::Reader::new(bgzf::Reader::new(stream));
                reader.read_header().map_err(IgvError::noodles)
            })
            .await
            .map_err(|e| IgvError::Other(e.to_string()))??;

            let chrom_names = match &index {
                CachedVcfIndex::Tabix(t) => t
                    .header()
                    .map(|h| {
                        h.reference_sequence_names()
                            .iter()
                            .map(|s| String::from_utf8_lossy(s.as_ref()).into_owned())
                            .collect()
                    })
                    .unwrap_or_default(),
                CachedVcfIndex::Csi(c) => c
                    .header()
                    .map(|h| {
                        h.reference_sequence_names()
                            .iter()
                            .map(|s| String::from_utf8_lossy(s.as_ref()).into_owned())
                            .collect()
                    })
                    .unwrap_or_default(),
            };
            let raw_samples: Vec<String> = header.sample_names().iter().cloned().collect();
            let sample_names = reorder_sample_names(&raw_samples, pedigree.as_deref());

            return Ok(Self {
                display,
                sample_names,
                backend: VcfBackend::Indexed {
                    location: location.clone(),
                    remote_stream,
                    header: Arc::new(header),
                    index: Arc::new(index),
                    chrom_names,
                },
                pedigree,
            });
        }

        // Unindexed `.vcf` or `.vcf.gz` fallback: load into memory with IntervalMap.
        let raw = fetch_bytes(location, auth_token).await?;
        let plain = tokio::task::spawn_blocking(move || maybe_gunzip(raw))
            .await
            .map_err(|e| IgvError::Other(e.to_string()))??;
        let ped_clone = pedigree.clone();
        let (sample_names, records, trees) = tokio::task::spawn_blocking(
            move || -> Result<(Vec<String>, Vec<VariantRecord>, HashMap<String, IntervalMap<u64, usize>>)> {
                let mut reader = vcf::io::Reader::new(BufReader::new(Cursor::new(plain)));
                let header = reader.read_header().map_err(IgvError::noodles)?;
                let raw_samples: Vec<String> = header.sample_names().iter().cloned().collect();
                let ordered_names = reorder_sample_names(&raw_samples, ped_clone.as_deref());
                let mut records = Vec::new();
                let mut trees: HashMap<String, IntervalMap<u64, usize>> = HashMap::new();
                for result in reader.records() {
                    let rec = result.map_err(IgvError::noodles)?;
                    if let Some(vr) =
                        extract_variant_record(&rec, &header, &raw_samples, ped_clone.as_deref())?
                    {
                        let pos = vr.pos;
                        let end = pos + vr.reference_allele.len().max(1) as u64;
                        let chrom = vr.chrom.clone();
                        let idx = records.len();
                        records.push(vr);
                        trees.entry(chrom).or_default().insert(pos..end, idx);
                    }
                }
                Ok((ordered_names, records, trees))
            },
        )
        .await
        .map_err(|e| IgvError::Other(e.to_string()))??;

        let chrom_names = trees.keys().cloned().collect();
        Ok(Self {
            display,
            sample_names,
            backend: VcfBackend::InMemory {
                records,
                trees,
                chrom_names,
            },
            pedigree,
        })
    }
}

pub(crate) fn reorder_sample_names(
    raw_samples: &[String],
    pedigree: Option<&Pedigree>,
) -> Vec<String> {
    match pedigree {
        Some(ped) => ped
            .order_samples(raw_samples)
            .into_iter()
            .map(|i| raw_samples[i].clone())
            .collect(),
        None => raw_samples.to_vec(),
    }
}

pub(crate) fn extract_variant_record<R: VariantRecordTrait>(
    rec: &R,
    header: &vcf::Header,
    raw_samples: &[String],
    pedigree: Option<&Pedigree>,
) -> Result<Option<VariantRecord>> {
    let chrom = rec
        .reference_sequence_name(header)
        .map_err(IgvError::noodles)?
        .to_string();
    let pos = match rec.variant_start() {
        Some(p) => p.map_err(IgvError::noodles)?.get() as u64,
        None => return Ok(None),
    };

    let ids_box = rec.ids();
    let ids: Vec<&str> = ids_box.iter().collect();
    let id = if ids.is_empty() || ids == ["."] {
        None
    } else {
        Some(ids.join(";"))
    };

    let ref_bytes: Vec<u8> = rec
        .reference_bases()
        .iter()
        .filter_map(|b| b.ok())
        .collect();
    let reference_allele = String::from_utf8_lossy(&ref_bytes).into_owned();

    let alternate_alleles: Vec<String> = rec
        .alternate_bases()
        .iter()
        .filter_map(|a| a.ok())
        .map(str::to_string)
        .collect();

    let quality = rec.quality_score().and_then(|q| q.ok());

    let filters = rec.filters();
    let passes_filter = if filters.is_empty() {
        true
    } else {
        filters.is_pass(header).unwrap_or(false)
    };

    let info = rec.info();
    let mut info_parts = Vec::new();
    let mut allele_freq: Option<f32> = None;
    for entry in info.iter(header) {
        if let Ok((k, val_opt)) = entry {
            let val_s = match val_opt {
                None | Some(InfoValue::Flag) => String::new(),
                Some(InfoValue::Integer(n)) => n.to_string(),
                Some(InfoValue::Float(f)) => {
                    if k == "AF" && allele_freq.is_none() {
                        allele_freq = Some(f);
                    }
                    format!("{f:.4}")
                }
                Some(InfoValue::Character(c)) => c.to_string(),
                Some(InfoValue::String(s)) => s.into_owned(),
                Some(InfoValue::Array(arr)) => {
                    use noodles::vcf::variant::record::info::field::value::Array as InfoArray;
                    match arr {
                        InfoArray::Float(vs) => {
                            let collected: Vec<Option<f32>> = vs.iter().filter_map(|r| r.ok()).collect();
                            if k == "AF" && allele_freq.is_none() {
                                allele_freq = collected.iter().flatten().copied().next();
                            }
                            collected
                                .iter()
                                .map(|o| o.map(|x| format!("{x:.4}")).unwrap_or_else(|| ".".into()))
                                .collect::<Vec<_>>()
                                .join(",")
                        }
                        InfoArray::Integer(vs) => vs
                            .iter()
                            .filter_map(|r| r.ok())
                            .map(|o| o.map(|x| x.to_string()).unwrap_or_else(|| ".".into()))
                            .collect::<Vec<_>>()
                            .join(","),
                        InfoArray::String(vs) => vs
                            .iter()
                            .filter_map(|r| r.ok())
                            .map(|o| o.map(|s| s.into_owned()).unwrap_or_else(|| ".".into()))
                            .collect::<Vec<_>>()
                            .join(","),
                        InfoArray::Character(vs) => vs
                            .iter()
                            .filter_map(|r| r.ok())
                            .map(|o| o.map(|c| c.to_string()).unwrap_or_else(|| ".".into()))
                            .collect::<Vec<_>>()
                            .join(","),
                    }
                }
            };
            if val_s.is_empty() {
                info_parts.push(k.to_string());
            } else {
                info_parts.push(format!("{k}={val_s}"));
            }
        }
    }
    let info_raw = info_parts.join(";");

    let samples = rec.samples().map_err(IgvError::noodles)?;
    let mut raw_genotypes = Vec::with_capacity(raw_samples.len());
    for (idx, sample) in samples.iter().enumerate() {
        let name = raw_samples
            .get(idx)
            .cloned()
            .unwrap_or_else(|| format!("S{}", idx + 1));

        let mut alleles = Vec::new();
        let mut phased = false;
        if let Some(Ok(Some(SampleValue::Genotype(gt)))) = sample.get(header, "GT") {
            for item in gt.iter() {
                if let Ok((a_opt, p)) = item {
                    alleles.push(a_opt);
                    if p == Phasing::Phased {
                        phased = true;
                    }
                }
            }
        }
        let call = classify_genotype(&alleles);

        let dp = match sample.get(header, "DP") {
            Some(Ok(Some(SampleValue::Integer(n)))) if n >= 0 => Some(n as u32),
            _ => None,
        };
        let gq = match sample.get(header, "GQ") {
            Some(Ok(Some(SampleValue::Integer(n)))) if n >= 0 => Some(n as u32),
            _ => None,
        };
        let ad = match sample.get(header, "AD") {
            Some(Ok(Some(SampleValue::Array(
                noodles::vcf::variant::record::samples::series::value::Array::Integer(vs),
            )))) => {
                let vals: Vec<u32> = vs
                    .iter()
                    .filter_map(|r| r.ok().flatten())
                    .map(|n| n.max(0) as u32)
                    .collect();
                if vals.len() >= 2 {
                    Some((vals[0], vals[1]))
                } else {
                    None
                }
            }
            _ => None,
        };

        raw_genotypes.push(SampleGenotype {
            sample: name,
            call,
            alleles,
            phased,
            dp,
            gq,
            ad,
            mendelian_violation: false,
        });
    }

    // Reorder genotypes by pedigree and compute Mendelian violations.
    let genotypes = if let Some(ped) = pedigree {
        let by_name: HashMap<String, usize> = raw_genotypes
            .iter()
            .enumerate()
            .map(|(i, g)| (g.sample.clone(), i))
            .collect();
        let violations: Vec<bool> = raw_genotypes
            .iter()
            .map(|g| {
                if let Some(p) = ped.get(&g.sample) {
                    if let (Some(fid), Some(mid)) = (&p.father_id, &p.mother_id) {
                        if let (Some(&fi), Some(&mi)) = (by_name.get(fid), by_name.get(mid)) {
                            return Pedigree::is_mendelian_violation(
                                &g.alleles,
                                &raw_genotypes[fi].alleles,
                                &raw_genotypes[mi].alleles,
                            );
                        }
                    }
                }
                false
            })
            .collect();
        for (g, v) in raw_genotypes.iter_mut().zip(violations) {
            g.mendelian_violation = v;
        }
        ped.order_samples(raw_samples)
            .into_iter()
            .filter_map(|i| raw_genotypes.get(i).cloned())
            .collect()
    } else {
        raw_genotypes
    };

    // Fallback AF computation from genotypes when INFO/AF is absent.
    if allele_freq.is_none() && !genotypes.is_empty() {
        let mut alt_count = 0u32;
        let mut total_alleles = 0u32;
        for g in &genotypes {
            for a in &g.alleles {
                if let Some(idx) = a {
                    total_alleles += 1;
                    if *idx > 0 {
                        alt_count += 1;
                    }
                }
            }
        }
        if total_alleles > 0 {
            allele_freq = Some(alt_count as f32 / total_alleles as f32);
        }
    }

    Ok(Some(VariantRecord {
        chrom,
        pos,
        id,
        reference_allele,
        alternate_alleles,
        quality,
        passes_filter,
        allele_freq,
        info_raw,
        genotypes,
    }))
}

fn classify_genotype(alleles: &[Option<usize>]) -> GenotypeCall {
    if alleles.is_empty() || alleles.iter().all(Option::is_none) {
        return GenotypeCall::NoCall;
    }
    let present: Vec<usize> = alleles.iter().flatten().copied().collect();
    if present.iter().all(|&a| a == 0) {
        GenotypeCall::HomRef
    } else if present.contains(&0) && present.iter().any(|&a| a > 0) {
        GenotypeCall::Het
    } else if present.iter().all(|&a| a == 1) {
        GenotypeCall::HomAlt
    } else {
        GenotypeCall::OtherAlt
    }
}

#[async_trait]
impl VcfSource for NoodlesVcfSource {
    async fn fetch(&self, region: &Region) -> Result<Vec<VariantRecord>> {
        match &self.backend {
            VcfBackend::InMemory {
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
            VcfBackend::Indexed {
                location,
                remote_stream,
                header,
                index,
                chrom_names,
            } => {
                let resolved_chrom = if chrom_names.is_empty() {
                    region.chrom.clone()
                } else {
                    match resolve_chrom_name(&region.chrom, chrom_names) {
                        Some(c) => c.to_string(),
                        None => return Ok(Vec::new()),
                    }
                };

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
                let region_str = format!("{resolved_chrom}:{}-{}", region.start, region.end);

                tokio::task::spawn_blocking(move || -> Result<Vec<VariantRecord>> {
                    let r: noodles::core::Region = region_str
                        .parse()
                        .map_err(|_| IgvError::InvalidRegion(region_str.clone()))?;

                    let mut out = Vec::new();
                    match index.as_ref() {
                        CachedVcfIndex::Tabix(tbi) => {
                            let mut reader = vcf::io::indexed_reader::Builder::default()
                                .set_index(tbi.clone())
                                .build_from_reader(stream)
                                .map_err(IgvError::noodles)?;
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
                        }
                        CachedVcfIndex::Csi(idx) => {
                            let mut reader = vcf::io::indexed_reader::Builder::default()
                                .set_index(idx.clone())
                                .build_from_reader(stream)
                                .map_err(IgvError::noodles)?;
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

/// Open a VCF or BCF variant source (indexed or unindexed, local or remote).
pub async fn open_variant(
    spec: &str,
    pedigree: Option<Arc<Pedigree>>,
    auth_token: Option<&str>,
) -> Result<Arc<dyn VcfSource>> {
    let loc = Location::parse(spec);
    let lower = loc.lowercase_name();
    if lower.ends_with(".bcf") || lower.ends_with(".bcf.gz") {
        let src =
            crate::source::bcf::NoodlesBcfSource::open_location(&loc, pedigree, auth_token).await?;
        Ok(Arc::new(src))
    } else {
        let src = NoodlesVcfSource::open_location(&loc, pedigree, auth_token).await?;
        Ok(Arc::new(src))
    }
}
