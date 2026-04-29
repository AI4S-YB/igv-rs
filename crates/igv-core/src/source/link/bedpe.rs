//! BEDPE backend stub — full implementation in Tasks 5 and 6.
use std::path::Path;
use async_trait::async_trait;
use crate::error::Result;
use crate::region::Region;
use super::{FetchLinkOpts, LinkSource, VisibleLink};

#[derive(Debug)]
pub struct BedpeLinkSource;

impl BedpeLinkSource {
    pub async fn open(_path: &Path) -> Result<Self> {
        unimplemented!("BedpeLinkSource::open — implemented in Task 5")
    }
}

#[async_trait]
impl LinkSource for BedpeLinkSource {
    async fn query(&self, _region: &Region, _opts: &FetchLinkOpts) -> Result<Vec<VisibleLink>> {
        unimplemented!("BedpeLinkSource::query — implemented in Task 5")
    }

    fn display_name(&self) -> &str {
        unimplemented!("BedpeLinkSource::display_name — implemented in Task 5")
    }

    fn record_count(&self) -> usize {
        unimplemented!("BedpeLinkSource::record_count — implemented in Task 5")
    }
}
