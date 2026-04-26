//! placeholder - implemented in Task 2.3.

use async_trait::async_trait;

use crate::error::Result;
use crate::region::Region;

#[derive(Debug, Clone, PartialEq)]
pub struct VariantRecord {
    pub chrom: String,
    pub pos: u64,
    pub reference_allele: String,
    pub alternate_alleles: Vec<String>,
    pub quality: Option<f32>,
    pub passes_filter: bool,
}

#[async_trait]
pub trait VcfSource: Send + Sync {
    async fn fetch(&self, region: &Region) -> Result<Vec<VariantRecord>>;
}

#[derive(Debug)]
pub struct NoodlesVcfSource;
