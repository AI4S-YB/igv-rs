//! Integration test: Loader::dispatch routes signal fetches and emits
//! LoadResult::Signal with the correct track_index for each source.

use std::sync::Arc;

use async_trait::async_trait;
use igv_core::error::Result;
use igv_core::region::Region;
use igv_core::source::{
    FastaSource, FetchOpts, FetchSignalOpts, RefMeta, SignalBin, SignalSource,
};
use igv_tui::app::loader::{LoadRequest, LoadResult, Loader};
use tokio::sync::mpsc;

struct MockFasta;

#[async_trait]
impl FastaSource for MockFasta {
    async fn references(&self) -> Result<Vec<RefMeta>> {
        Ok(vec![RefMeta { name: "chr1".into(), length: 1000 }])
    }
    async fn fetch(&self, _r: &Region) -> Result<Vec<u8>> {
        Ok(Vec::new())
    }
}

struct MockSignal {
    bins: Vec<SignalBin>,
    name: String,
}

#[async_trait]
impl SignalSource for MockSignal {
    async fn fetch(&self, _r: &Region, _o: &FetchSignalOpts) -> Result<Vec<SignalBin>> {
        Ok(self.bins.clone())
    }
    fn display_name(&self) -> &str {
        &self.name
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn dispatch_routes_signals_to_correct_indices() {
    let (tx, mut rx) = mpsc::channel(8);

    let fasta: Arc<dyn FastaSource> = Arc::new(MockFasta);

    let bins_a = vec![SignalBin { start: 1, end: 100, value: 1.0 }];
    let bins_b = vec![SignalBin { start: 1, end: 100, value: 2.0 }];
    let signals: Vec<Arc<dyn SignalSource>> = vec![
        Arc::new(MockSignal { bins: bins_a.clone(), name: "a".into() }),
        Arc::new(MockSignal { bins: bins_b.clone(), name: "b".into() }),
    ];

    let mut loader = Loader::new(fasta, None, vec![], vec![], signals, tx);
    loader.dispatch(LoadRequest {
        generation: 1,
        region: Region::new("chr1", 1, 100).unwrap(),
        fetch_opts: FetchOpts::default(),
        signal_max_bins: 200,
    });

    let mut got_a = false;
    let mut got_b = false;
    while let Ok(Some(r)) =
        tokio::time::timeout(std::time::Duration::from_millis(200), rx.recv()).await
    {
        if let LoadResult::Signal { track_index, bins, generation } = r {
            assert_eq!(generation, 1);
            match track_index {
                0 => {
                    assert_eq!(bins, bins_a);
                    got_a = true;
                }
                1 => {
                    assert_eq!(bins, bins_b);
                    got_b = true;
                }
                _ => panic!("unexpected track_index {track_index}"),
            }
            if got_a && got_b {
                break;
            }
        }
        // Reference / VCF / BAM / Annotation results are also drained but
        // ignored.
    }
    assert!(got_a && got_b, "missing one of the signal results");
}
