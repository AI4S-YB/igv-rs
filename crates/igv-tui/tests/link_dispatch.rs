use std::sync::Arc;

use async_trait::async_trait;
use igv_core::region::Region;
use igv_core::render::RenderMode;
use igv_core::source::link::{
    FetchLinkOpts, LinkRecord, LinkScope, LinkSource, VisibleLink,
};
use igv_core::source::{FetchOpts, RefMeta};
use igv_tui::app::loader::{LoadRequest, LoadResult, Loader};

#[derive(Debug)]
struct StubLink {
    name: String,
    out: Vec<VisibleLink>,
    count: usize,
}

#[async_trait]
impl LinkSource for StubLink {
    async fn query(
        &self,
        _region: &Region,
        _opts: &FetchLinkOpts,
    ) -> igv_core::error::Result<Vec<VisibleLink>> {
        Ok(self.out.clone())
    }
    fn display_name(&self) -> &str {
        &self.name
    }
    fn record_count(&self) -> usize {
        self.count
    }
}

#[derive(Debug)]
struct StubFasta;
#[async_trait]
impl igv_core::source::FastaSource for StubFasta {
    async fn references(&self) -> igv_core::error::Result<Vec<RefMeta>> {
        Ok(vec![RefMeta { name: "chr1".into(), length: 1_000_000 }])
    }
    async fn fetch(&self, _r: &Region) -> igv_core::error::Result<Vec<u8>> {
        Ok(Vec::new())
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn dispatch_emits_link_results_per_track() {
    let r = LinkRecord {
        chrom_a: Arc::from("chr1"),
        start_a: 100,
        end_a: 200,
        chrom_b: Arc::from("chr1"),
        start_b: 300,
        end_b: 400,
        name: Some("loop".into()),
        score: Some(1.0),
        strand_a: igv_core::source::annotation::Strand::Forward,
        strand_b: igv_core::source::annotation::Strand::Reverse,
    };
    let v = vec![VisibleLink { record: r, scope: LinkScope::BothIn }];
    let link_a: Arc<dyn LinkSource> = Arc::new(StubLink {
        name: "a".into(),
        out: v.clone(),
        count: 1,
    });
    let link_b: Arc<dyn LinkSource> = Arc::new(StubLink {
        name: "b".into(),
        out: vec![],
        count: 0,
    });

    let (tx, mut rx) = tokio::sync::mpsc::channel::<LoadResult>(16);
    let mut loader = Loader::new(
        Arc::new(StubFasta),
        None,
        vec![],
        vec![],
        vec![],
        vec![link_a, link_b],
        tx,
    );

    loader.dispatch(LoadRequest {
        generation: 1,
        region: Region::new("chr1", 1, 1000).unwrap(),
        fetch_opts: FetchOpts::default(),
        signal_max_bins: 100,
        link_min_score: None,
        render_mode: RenderMode::DetailedReads,
    });

    let mut got_a = false;
    let mut got_b = false;
    while let Some(msg) =
        tokio::time::timeout(std::time::Duration::from_secs(2), rx.recv())
            .await
            .ok()
            .flatten()
    {
        if let LoadResult::Link { generation, track_index, visible, total_record_count } = msg {
            assert_eq!(generation, 1);
            match track_index {
                0 => {
                    assert_eq!(visible.len(), 1);
                    assert_eq!(total_record_count, 1);
                    got_a = true;
                }
                1 => {
                    assert!(visible.is_empty());
                    assert_eq!(total_record_count, 0);
                    got_b = true;
                }
                _ => panic!("unexpected track_index {track_index}"),
            }
        }
        if got_a && got_b {
            break;
        }
    }
    assert!(got_a && got_b, "missing link result(s)");
}
