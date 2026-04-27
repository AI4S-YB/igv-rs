use std::sync::Arc;

use igv_core::region::Region;
use igv_core::source::bam::AlignmentRow;
use igv_core::source::vcf::VariantRecord;
use igv_core::source::{BamSource, FastaSource, FetchOpts, VcfSource};
use igv_core::source::{FetchSignalOpts, SignalBin, SignalSource};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tracing::warn;

#[derive(Debug, Clone)]
pub struct LoadRequest {
    pub generation: u64,
    pub region: Region,
    pub fetch_opts: FetchOpts,
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
    Error {
        generation: u64,
        message: String,
    },
}

pub struct Loader {
    pub fasta: Arc<dyn FastaSource>,
    pub vcf: Option<Arc<dyn VcfSource>>,
    pub bams: Vec<Arc<dyn BamSource>>,
    pub annotations: Vec<std::sync::Arc<dyn igv_core::source::AnnotationSource>>,
    pub signals: Vec<Arc<dyn SignalSource>>,
    pub tx: mpsc::Sender<LoadResult>,
    pub current: Vec<JoinHandle<()>>,
}

impl Loader {
    pub fn new(
        fasta: Arc<dyn igv_core::source::FastaSource>,
        vcf: Option<Arc<dyn igv_core::source::VcfSource>>,
        bams: Vec<Arc<dyn igv_core::source::BamSource>>,
        annotations: Vec<Arc<dyn igv_core::source::AnnotationSource>>,
        signals: Vec<Arc<dyn SignalSource>>,
        tx: tokio::sync::mpsc::Sender<LoadResult>,
    ) -> Self {
        Self {
            fasta,
            vcf,
            bams,
            annotations,
            signals,
            tx,
            current: Vec::new(),
        }
    }

    /// Cancel any in-flight tasks and dispatch fresh ones for `req`.
    pub fn dispatch(&mut self, req: LoadRequest) {
        for h in self.current.drain(..) {
            h.abort();
        }

        // Reference fetch
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

        // VCF fetch
        if let Some(vcf) = &self.vcf {
            let vcf = Arc::clone(vcf);
            let tx = self.tx.clone();
            let r = req.clone();
            self.current.push(tokio::spawn(async move {
                match vcf.fetch(&r.region).await {
                    Ok(records) => {
                        let _ = tx
                            .send(LoadResult::Variants {
                                generation: r.generation,
                                records,
                            })
                            .await;
                    }
                    Err(e) => {
                        warn!("vcf fetch failed: {e}");
                        let _ = tx
                            .send(LoadResult::Variants {
                                generation: r.generation,
                                records: Vec::new(),
                            })
                            .await;
                    }
                }
            }));
        }

        // BAM fetches
        for (idx, bam) in self.bams.iter().enumerate() {
            let bam = Arc::clone(bam);
            let tx = self.tx.clone();
            let r = req.clone();
            self.current.push(tokio::spawn(async move {
                match bam.fetch(&r.region, &r.fetch_opts).await {
                    Ok(rows) => {
                        let _ = tx
                            .send(LoadResult::Bam {
                                generation: r.generation,
                                bam_index: idx,
                                rows,
                            })
                            .await;
                    }
                    Err(e) => {
                        warn!("bam fetch failed: {e}");
                        let _ = tx
                            .send(LoadResult::Bam {
                                generation: r.generation,
                                bam_index: idx,
                                rows: Vec::new(),
                            })
                            .await;
                    }
                }
            }));
        }

        for (idx, ann) in self.annotations.iter().enumerate() {
            let ann = std::sync::Arc::clone(ann);
            let tx = self.tx.clone();
            let r = req.clone();
            self.current.push(tokio::spawn(async move {
                match ann.fetch(&r.region).await {
                    Ok(transcripts) => {
                        let _ = tx
                            .send(LoadResult::Annotation {
                                generation: r.generation,
                                track_index: idx,
                                transcripts,
                            })
                            .await;
                    }
                    Err(e) => {
                        tracing::warn!("annotation fetch failed: {e}");
                        let _ = tx
                            .send(LoadResult::Annotation {
                                generation: r.generation,
                                track_index: idx,
                                transcripts: Vec::new(),
                            })
                            .await;
                    }
                }
            }));
        }

        for (idx, sig) in self.signals.iter().enumerate() {
            let sig = Arc::clone(sig);
            let tx = self.tx.clone();
            let r = req.clone();
            self.current.push(tokio::spawn(async move {
                let opts = FetchSignalOpts::default();
                match sig.fetch(&r.region, &opts).await {
                    Ok(bins) => {
                        let _ = tx
                            .send(LoadResult::Signal {
                                generation: r.generation,
                                track_index: idx,
                                bins,
                            })
                            .await;
                    }
                    Err(e) => {
                        tracing::warn!("signal fetch failed: {e}");
                        let _ = tx
                            .send(LoadResult::Signal {
                                generation: r.generation,
                                track_index: idx,
                                bins: Vec::new(),
                            })
                            .await;
                    }
                }
            }));
        }
    }
}
