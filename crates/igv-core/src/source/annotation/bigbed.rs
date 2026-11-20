//! UCSC bigBed annotation source (`.bb` / `.bigbed` / `bigGenePred` / `bigRepMsk`),
//! supporting BED3–BED12 exon/CDS/UTR blocks over local files and remote URLs.

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use bigtools::BigBedRead;

use crate::error::{IgvError, Result};
use crate::genome::resolve_chrom_name;
use crate::io::{Location, SeekableStream};
use crate::region::Region;
use crate::source::annotation::{
    AnnotationBlock, AnnotationSource, AnnotationTranscript, BlockKind, Strand, TranscriptKind,
};

pub struct BigBedAnnotationSource {
    display: String,
    reader: Arc<Mutex<BigBedRead<SeekableStream>>>,
    chroms: Vec<String>,
}

impl std::fmt::Debug for BigBedAnnotationSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BigBedAnnotationSource")
            .field("display", &self.display)
            .field("chroms", &self.chroms.len())
            .finish_non_exhaustive()
    }
}

impl BigBedAnnotationSource {
    pub async fn open_location(location: &Location, auth_token: Option<&str>) -> Result<Self> {
        let display = location.display_name();
        let stream = SeekableStream::open(location, auth_token).await?;
        let reader = tokio::task::spawn_blocking(move || -> Result<BigBedRead<SeekableStream>> {
            BigBedRead::open(stream)
                .map_err(|e| IgvError::Other(format!("bigBed open: {e}")))
        })
        .await
        .map_err(|e| IgvError::Other(e.to_string()))??;
        let chroms = reader.chroms().iter().map(|c| c.name.clone()).collect();
        Ok(Self {
            display,
            reader: Arc::new(Mutex::new(reader)),
            chroms,
        })
    }
}

#[async_trait]
impl AnnotationSource for BigBedAnnotationSource {
    async fn fetch(&self, region: &Region) -> Result<Vec<AnnotationTranscript>> {
        let Some(chrom) = resolve_chrom_name(&region.chrom, &self.chroms).map(str::to_string)
        else {
            return Ok(Vec::new());
        };
        let start = region.start.saturating_sub(1) as u32;
        let end = region.end.min(u32::MAX as u64) as u32;
        let reader = Arc::clone(&self.reader);

        tokio::task::spawn_blocking(move || -> Result<Vec<AnnotationTranscript>> {
            let mut guard = reader
                .lock()
                .map_err(|_| IgvError::Other("bigBed mutex poisoned".into()))?;
            let entries: Vec<_> = guard
                .get_interval(&chrom, start, end)
                .map_err(|e| IgvError::Other(format!("bigBed query: {e}")))?
                .filter_map(|r| r.ok())
                .collect();
            drop(guard);

            let mut out = Vec::with_capacity(entries.len());
            for entry in entries {
                let s1 = u64::from(entry.start) + 1;
                let e1 = u64::from(entry.end);
                let rest = entry.rest.trim();
                if rest.is_empty() {
                    out.push(AnnotationTranscript {
                        name: String::new(),
                        id: format!("bb:{chrom}:{s1}"),
                        gene_id: None,
                        strand: Strand::Unknown,
                        blocks: vec![AnnotationBlock {
                            start: s1,
                            end: e1,
                            kind: BlockKind::BedSegment,
                        }],
                        kind: TranscriptKind::BedFeature,
                    });
                    continue;
                }
                let cols: Vec<&str> = rest.split('\t').collect();
                let name = cols.first().copied().unwrap_or("").to_string();
                let strand = match cols.get(2).copied() {
                    Some("+") => Strand::Forward,
                    Some("-") => Strand::Reverse,
                    _ => Strand::Unknown,
                };
                // BED12 in bigBed `rest` has: [0]=name, [1]=score, [2]=strand,
                // [3]=thickStart, [4]=thickEnd, [5]=itemRgb, [6]=blockCount,
                // [7]=blockSizes, [8]=blockStarts.
                let blocks = if cols.len() >= 9 {
                    let thick_s = cols[3].parse::<u64>().unwrap_or(entry.start as u64) + 1;
                    let thick_e = cols[4].parse::<u64>().unwrap_or(entry.end as u64);
                    let count = cols[6].parse::<usize>().unwrap_or(0);
                    let sizes: Vec<u64> = cols[7]
                        .trim_end_matches(',')
                        .split(',')
                        .filter_map(|s| s.parse().ok())
                        .collect();
                    let starts: Vec<u64> = cols[8]
                        .trim_end_matches(',')
                        .split(',')
                        .filter_map(|s| s.parse().ok())
                        .collect();
                    if count > 0 && sizes.len() == count && starts.len() == count {
                        let mut bvec = Vec::with_capacity(count);
                        for i in 0..count {
                            let bs = u64::from(entry.start) + starts[i] + 1;
                            let be = u64::from(entry.start) + starts[i] + sizes[i];
                            let kind = if thick_e >= thick_s && bs >= thick_s && be <= thick_e {
                                BlockKind::Cds
                            } else {
                                BlockKind::Exon
                            };
                            bvec.push(AnnotationBlock {
                                start: bs,
                                end: be,
                                kind,
                            });
                        }
                        bvec
                    } else {
                        vec![AnnotationBlock {
                            start: s1,
                            end: e1,
                            kind: BlockKind::BedSegment,
                        }]
                    }
                } else {
                    vec![AnnotationBlock {
                        start: s1,
                        end: e1,
                        kind: BlockKind::BedSegment,
                    }]
                };

                let id = if name.is_empty() {
                    format!("bb:{chrom}:{s1}")
                } else {
                    name.clone()
                };
                out.push(AnnotationTranscript {
                    name,
                    id,
                    gene_id: None,
                    strand,
                    blocks,
                    kind: if cols.len() >= 9 {
                        TranscriptKind::Mrna
                    } else {
                        TranscriptKind::BedFeature
                    },
                });
            }
            Ok(out)
        })
        .await
        .map_err(|e| IgvError::Other(e.to_string()))?
    }

    fn display_name(&self) -> &str {
        &self.display
    }
}
