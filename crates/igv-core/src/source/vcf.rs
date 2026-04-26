use std::path::{Path, PathBuf};

use async_trait::async_trait;
use noodles::vcf;

use crate::error::{IgvError, Result};
use crate::region::Region;

#[derive(Debug, Clone, PartialEq)]
pub struct VariantRecord {
    pub chrom: String,
    pub pos: u64, // 1-based
    pub reference_allele: String,
    pub alternate_alleles: Vec<String>,
    pub quality: Option<f32>,
    pub passes_filter: bool,
}

#[async_trait]
pub trait VcfSource: Send + Sync {
    async fn fetch(&self, region: &Region) -> Result<Vec<VariantRecord>>;
}

/// Note: noodles' `vcf::io::IndexedReader` stores a `Box<dyn BinningIndex>` which
/// is not `Send`, so we cannot stash it inside an `Arc<Mutex<...>>` shared across
/// `spawn_blocking` boundaries. Instead, we cache the path and re-open per fetch
/// (header + tabix parse is microseconds for typical VCFs).
#[derive(Debug)]
pub struct NoodlesVcfSource {
    path: PathBuf,
}

impl NoodlesVcfSource {
    pub async fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        let p = path.clone();
        // Validate by opening and reading the header.
        tokio::task::spawn_blocking(move || -> Result<()> {
            let mut reader = vcf::io::indexed_reader::Builder::default()
                .build_from_path(&p)
                .map_err(|e| IgvError::io(p.clone(), e))?;
            reader.read_header().map_err(IgvError::noodles)?;
            Ok(())
        })
        .await
        .map_err(|e| IgvError::Other(e.to_string()))??;
        Ok(Self { path })
    }
}

#[async_trait]
impl VcfSource for NoodlesVcfSource {
    async fn fetch(&self, region: &Region) -> Result<Vec<VariantRecord>> {
        use noodles::vcf::variant::record::AlternateBases as _;

        let path = self.path.clone();
        let region = region.clone();

        tokio::task::spawn_blocking(move || -> Result<Vec<VariantRecord>> {
            let mut reader = vcf::io::indexed_reader::Builder::default()
                .build_from_path(&path)
                .map_err(|e| IgvError::io(path.clone(), e))?;
            let header = reader.read_header().map_err(IgvError::noodles)?;

            let region_str = format!("{}:{}-{}", region.chrom, region.start, region.end);
            let r: noodles::core::Region = region_str
                .parse()
                .map_err(|_| IgvError::InvalidRegion(region_str.clone()))?;
            let mut out = Vec::new();
            for result in reader.query(&header, &r).map_err(IgvError::noodles)? {
                let rec = result.map_err(IgvError::noodles)?;
                let chrom = rec.reference_sequence_name().to_string();
                let pos = match rec.variant_start() {
                    Some(p) => p.map_err(IgvError::noodles)?.get() as u64,
                    None => continue,
                };
                let ref_allele = rec.reference_bases().to_string();
                let alts: Vec<String> = rec
                    .alternate_bases()
                    .iter()
                    .filter_map(|a| a.ok())
                    .map(|a| a.to_string())
                    .collect();
                let quality = rec.quality_score().and_then(|q| q.ok());
                let filters = rec.filters();
                let filters_str: &str = filters.as_ref();
                let passes_filter = filters_str.is_empty()
                    || filters_str == "."
                    || filters_str.split(';').all(|s| s == "PASS");
                out.push(VariantRecord {
                    chrom,
                    pos,
                    reference_allele: ref_allele,
                    alternate_alleles: alts,
                    quality,
                    passes_filter,
                });
            }
            Ok(out)
        })
        .await
        .map_err(|e| IgvError::Other(e.to_string()))?
    }
}
