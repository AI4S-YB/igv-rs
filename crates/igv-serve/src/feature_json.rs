//! Conversions from igv-core record types to the JSON shapes igv.js
//! expects for custom feature sources.

use serde_json::{json, Value};

use igv_core::source::{AnnotationTranscript, BlockKind, LinkRecord, Strand};

pub fn annotation_to_json(chrom: &str, tx: &AnnotationTranscript) -> Value {
    let (start, end) = tx.span().unwrap_or((0, 0));
    let exons: Vec<Value> = tx
        .blocks
        .iter()
        .filter(|b| matches!(b.kind, BlockKind::Exon | BlockKind::Cds))
        .map(|b| json!({ "start": b.start, "end": b.end }))
        .collect();
    json!({
        "chr": chrom,
        "start": start,
        "end": end,
        "name": tx.name,
        "strand": match tx.strand {
            Strand::Forward => "+",
            Strand::Reverse => "-",
            Strand::Unknown => ".",
        },
        "type": "transcript",
        "exons": exons,
    })
}

pub fn link_to_json(rec: &LinkRecord) -> Value {
    let mut v = json!({
        "chr1": &*rec.chrom_a,
        "start1": rec.start_a,
        "end1": rec.end_a,
        "chr2": &*rec.chrom_b,
        "start2": rec.start_b,
        "end2": rec.end_b,
    });
    if let Some(score) = rec.score {
        v["score"] = json!(score);
        v["value"] = json!(score);
    }
    if let Some(name) = &rec.name {
        v["name"] = json!(name);
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;
    use igv_core::source::{AnnotationBlock, TranscriptKind};

    fn tx() -> AnnotationTranscript {
        AnnotationTranscript {
            id: "tx1".into(),
            name: "BRCA1".into(),
            gene_id: None,
            kind: TranscriptKind::Other,
            strand: Strand::Forward,
            blocks: vec![
                AnnotationBlock {
                    start: 100,
                    end: 200,
                    kind: BlockKind::Exon,
                },
                AnnotationBlock {
                    start: 300,
                    end: 400,
                    kind: BlockKind::Exon,
                },
            ],
        }
    }

    #[test]
    fn annotation_to_json_includes_chr_span_strand_exons() {
        let v = annotation_to_json("chr1", &tx());
        assert_eq!(v["chr"], "chr1");
        assert_eq!(v["start"], 100);
        assert_eq!(v["end"], 400);
        assert_eq!(v["strand"], "+");
        assert_eq!(v["exons"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn link_to_json_includes_both_anchors_and_score() {
        let rec = LinkRecord {
            chrom_a: "chr1".into(),
            start_a: 100,
            end_a: 200,
            chrom_b: "chr1".into(),
            start_b: 500,
            end_b: 600,
            name: Some("loop_a".into()),
            score: Some(7.5),
            strand_a: Strand::Unknown,
            strand_b: Strand::Unknown,
        };
        let v = link_to_json(&rec);
        assert_eq!(v["chr1"], "chr1");
        assert_eq!(v["start1"], 100);
        assert_eq!(v["end2"], 600);
        assert_eq!(v["score"], 7.5);
        assert_eq!(v["value"], 7.5);
        assert_eq!(v["name"], "loop_a");
    }
}
