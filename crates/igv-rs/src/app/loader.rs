use std::sync::Arc;

use igv_core::region::Region;
use igv_core::render::RenderMode;
use igv_core::source::bam::AlignmentRow;
use igv_core::source::gwas::{GwasPoint, GwasSource};
use igv_core::source::link::{FetchLinkOpts, LinkSource, VisibleLink};
use igv_core::source::seg::{SegSource, SegTrackData};
use igv_core::source::vcf::VariantRecord;
use igv_core::source::{BamSource, FastaSource, FetchOpts, SignalSummary, VcfSource};
use igv_core::source::{FetchSignalOpts, SignalBin, SignalSource};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tracing::warn;

#[derive(Debug, Clone)]
pub struct LoadRequest {
    pub generation: u64,
    pub region: Region,
    pub fetch_opts: FetchOpts,
    pub signal_max_bins: u32,
    pub link_min_score: Option<f64>,
    pub render_mode: RenderMode,
}

#[derive(Debug)]
pub enum LoadResult {
    Reference {
        generation: u64,
        region: Region,
        bytes: Vec<u8>,
    },
    Variants {
        generation: u64,
        records: Vec<VariantRecord>,
    },
    VcfTrack {
        generation: u64,
        track_index: usize,
        records: Vec<VariantRecord>,
    },
    Bam {
        generation: u64,
        bam_index: usize,
        rows: Vec<AlignmentRow>,
    },
    Annotation {
        generation: u64,
        track_index: usize,
        transcripts: Vec<igv_core::source::AnnotationTranscript>,
    },
    Signal {
        generation: u64,
        track_index: usize,
        bins: Vec<SignalBin>,
    },
    Link {
        generation: u64,
        track_index: usize,
        visible: Vec<VisibleLink>,
        total_record_count: usize,
    },
    Seg {
        generation: u64,
        track_index: usize,
        data: SegTrackData,
    },
    Gwas {
        generation: u64,
        track_index: usize,
        points: Vec<GwasPoint>,
    },
    Error {
        generation: u64,
        message: String,
    },
}

impl LoadResult {
    pub fn generation(&self) -> u64 {
        match self {
            LoadResult::Reference { generation, .. }
            | LoadResult::Variants { generation, .. }
            | LoadResult::VcfTrack { generation, .. }
            | LoadResult::Bam { generation, .. }
            | LoadResult::Annotation { generation, .. }
            | LoadResult::Signal { generation, .. }
            | LoadResult::Link { generation, .. }
            | LoadResult::Seg { generation, .. }
            | LoadResult::Gwas { generation, .. }
            | LoadResult::Error { generation, .. } => *generation,
        }
    }
}

pub struct Loader {
    pub fasta: Arc<dyn FastaSource>,
    pub vcf: Option<Arc<dyn VcfSource>>,
    pub vcfs: Vec<Arc<dyn VcfSource>>,
    pub bams: Vec<Arc<dyn BamSource>>,
    pub annotations: Vec<Arc<dyn igv_core::source::AnnotationSource>>,
    pub signals: Vec<Arc<dyn SignalSource>>,
    pub links: Vec<Arc<dyn LinkSource>>,
    pub segs: Vec<Arc<dyn SegSource>>,
    pub gwas: Vec<Arc<dyn GwasSource>>,
    pub signal_summary: SignalSummary,
    pub tx: mpsc::Sender<LoadResult>,
    pub current: Vec<JoinHandle<()>>,
}

impl Loader {
    /// Signature retained for integration tests; extra track collections default
    /// to empty and can be configured via builder/mutators.
    pub fn new(
        fasta: Arc<dyn igv_core::source::FastaSource>,
        vcf: Option<Arc<dyn igv_core::source::VcfSource>>,
        bams: Vec<Arc<dyn igv_core::source::BamSource>>,
        annotations: Vec<Arc<dyn igv_core::source::AnnotationSource>>,
        signals: Vec<Arc<dyn SignalSource>>,
        links: Vec<Arc<dyn LinkSource>>,
        tx: tokio::sync::mpsc::Sender<LoadResult>,
    ) -> Self {
        Self {
            fasta,
            vcf,
            vcfs: Vec::new(),
            bams,
            annotations,
            signals,
            links,
            segs: Vec::new(),
            gwas: Vec::new(),
            signal_summary: SignalSummary::Max,
            tx,
            current: Vec::new(),
        }
    }

    pub fn set_extra_tracks(
        &mut self,
        vcfs: Vec<Arc<dyn VcfSource>>,
        segs: Vec<Arc<dyn SegSource>>,
        gwas: Vec<Arc<dyn GwasSource>>,
        signal_summary: SignalSummary,
    ) {
        self.vcfs = vcfs;
        self.segs = segs;
        self.gwas = gwas;
        self.signal_summary = signal_summary;
    }

    /// Cancel any in-flight tasks and dispatch fresh ones for `req`.
    pub fn dispatch(&mut self, req: LoadRequest) {
        for h in self.current.drain(..) {
            h.abort();
        }

        let needs_per_base = matches!(
            req.render_mode,
            RenderMode::PerBase | RenderMode::DetailedReads
        );
        let suppress_overview = matches!(req.render_mode, RenderMode::OverviewOnly);

        if needs_per_base {
            let fasta = Arc::clone(&self.fasta);
            let tx = self.tx.clone();
            let r = req.clone();
            self.current.push(tokio::spawn(async move {
                match fasta.fetch(&r.region).await {
                    Ok(bytes) => {
                        let _ = tx
                            .send(LoadResult::Reference {
                                generation: r.generation,
                                region: r.region,
                                bytes,
                            })
                            .await;
                    }
                    Err(e) => {
                        let _ = tx
                            .send(LoadResult::Error {
                                generation: r.generation,
                                message: e.to_string(),
                            })
                            .await;
                    }
                }
            }));
        } else {
            let tx = self.tx.clone();
            let r = req.clone();
            self.current.push(tokio::spawn(async move {
                let _ = tx
                    .send(LoadResult::Reference {
                        generation: r.generation,
                        region: r.region,
                        bytes: Vec::new(),
                    })
                    .await;
            }));
        }

        if !suppress_overview {
            if let Some(vcf) = &self.vcf {
                let vcf = Arc::clone(vcf);
                let tx = self.tx.clone();
                let r = req.clone();
                self.current.push(tokio::spawn(async move {
                    let records = vcf.fetch(&r.region).await.unwrap_or_else(|e| {
                        warn!("vcf fetch failed: {e}");
                        Vec::new()
                    });
                    let _ = tx
                        .send(LoadResult::Variants {
                            generation: r.generation,
                            records,
                        })
                        .await;
                }));
            }
            for (idx, vcf) in self.vcfs.iter().enumerate() {
                let vcf = Arc::clone(vcf);
                let tx = self.tx.clone();
                let r = req.clone();
                self.current.push(tokio::spawn(async move {
                    let records = vcf.fetch(&r.region).await.unwrap_or_else(|e| {
                        warn!("vcf[{idx}] fetch failed: {e}");
                        Vec::new()
                    });
                    let _ = tx
                        .send(LoadResult::VcfTrack {
                            generation: r.generation,
                            track_index: idx,
                            records,
                        })
                        .await;
                }));
            }
        }

        if needs_per_base {
            for (idx, bam) in self.bams.iter().enumerate() {
                let bam = Arc::clone(bam);
                let tx = self.tx.clone();
                let r = req.clone();
                self.current.push(tokio::spawn(async move {
                    let rows = bam.fetch(&r.region, &r.fetch_opts).await.unwrap_or_else(|e| {
                        warn!("bam fetch failed: {e}");
                        Vec::new()
                    });
                    let _ = tx
                        .send(LoadResult::Bam {
                            generation: r.generation,
                            bam_index: idx,
                            rows,
                        })
                        .await;
                }));
            }
        } else {
            for idx in 0..self.bams.len() {
                let tx = self.tx.clone();
                let r = req.clone();
                self.current.push(tokio::spawn(async move {
                    let _ = tx
                        .send(LoadResult::Bam {
                            generation: r.generation,
                            bam_index: idx,
                            rows: Vec::new(),
                        })
                        .await;
                }));
            }
        }

        for (idx, ann) in self.annotations.iter().enumerate() {
            let ann = Arc::clone(ann);
            let tx = self.tx.clone();
            let r = req.clone();
            self.current.push(tokio::spawn(async move {
                let transcripts = ann.fetch(&r.region).await.unwrap_or_else(|e| {
                    warn!("annotation fetch failed: {e}");
                    Vec::new()
                });
                let _ = tx
                    .send(LoadResult::Annotation {
                        generation: r.generation,
                        track_index: idx,
                        transcripts,
                    })
                    .await;
            }));
        }

        let summary = self.signal_summary;
        for (idx, sig) in self.signals.iter().enumerate() {
            let sig = Arc::clone(sig);
            let tx = self.tx.clone();
            let r = req.clone();
            self.current.push(tokio::spawn(async move {
                let opts = FetchSignalOpts {
                    max_bins: r.signal_max_bins.max(1),
                    summary,
                };
                let bins = sig.fetch(&r.region, &opts).await.unwrap_or_else(|e| {
                    warn!("signal fetch failed: {e}");
                    Vec::new()
                });
                let _ = tx
                    .send(LoadResult::Signal {
                        generation: r.generation,
                        track_index: idx,
                        bins,
                    })
                    .await;
            }));
        }

        for (idx, lk) in self.links.iter().enumerate() {
            let lk = Arc::clone(lk);
            let tx = self.tx.clone();
            let r = req.clone();
            self.current.push(tokio::spawn(async move {
                let opts = FetchLinkOpts {
                    min_score: r.link_min_score,
                };
                let visible = lk.query(&r.region, &opts).await.unwrap_or_else(|e| {
                    warn!("link query failed: {e}");
                    Vec::new()
                });
                let count = lk.record_count();
                let _ = tx
                    .send(LoadResult::Link {
                        generation: r.generation,
                        track_index: idx,
                        visible,
                        total_record_count: count,
                    })
                    .await;
            }));
        }

        for (idx, seg) in self.segs.iter().enumerate() {
            let seg = Arc::clone(seg);
            let tx = self.tx.clone();
            let r = req.clone();
            self.current.push(tokio::spawn(async move {
                let data = seg.fetch(&r.region).await.unwrap_or_else(|e| {
                    warn!("seg fetch failed: {e}");
                    SegTrackData::default()
                });
                let _ = tx
                    .send(LoadResult::Seg {
                        generation: r.generation,
                        track_index: idx,
                        data,
                    })
                    .await;
            }));
        }

        for (idx, gw) in self.gwas.iter().enumerate() {
            let gw = Arc::clone(gw);
            let tx = self.tx.clone();
            let r = req.clone();
            self.current.push(tokio::spawn(async move {
                let points = gw.fetch(&r.region).await.unwrap_or_else(|e| {
                    warn!("gwas fetch failed: {e}");
                    Vec::new()
                });
                let _ = tx
                    .send(LoadResult::Gwas {
                        generation: r.generation,
                        track_index: idx,
                        points,
                    })
                    .await;
            }));
        }
    }
}
