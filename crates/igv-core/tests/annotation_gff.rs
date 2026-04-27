use std::path::Path;

use igv_core::region::Region;
use igv_core::source::annotation::AnnotationFormat;
use igv_core::source::annotation::gff::NoodlesGffSource;
use igv_core::source::{AnnotationSource, BlockKind, Strand, TranscriptKind};

#[tokio::test]
async fn gff3_returns_two_mrna_transcripts() {
    let src = NoodlesGffSource::open(
        Path::new("tests/data/sample.gff3"),
        AnnotationFormat::Gff3,
    )
    .await
    .unwrap();
    let region = Region::new("chr1", 1, 1000).unwrap();
    let txs = src.fetch(&region).await.unwrap();
    let names: Vec<_> = txs.iter().map(|t| t.id.as_str()).collect();
    assert!(names.contains(&"tx1"));
    assert!(names.contains(&"tx2"));
    assert_eq!(txs.iter().filter(|t| t.kind == TranscriptKind::Mrna).count(), 2);
}

#[tokio::test]
async fn gff3_classifies_cds_and_utrs_for_first_transcript() {
    let src = NoodlesGffSource::open(
        Path::new("tests/data/sample.gff3"),
        AnnotationFormat::Gff3,
    )
    .await
    .unwrap();
    let region = Region::new("chr1", 1, 1000).unwrap();
    let txs = src.fetch(&region).await.unwrap();
    let tx1 = txs.iter().find(|t| t.id == "tx1").expect("tx1 missing");
    let cds = tx1.blocks.iter().filter(|b| b.kind == BlockKind::Cds).count();
    let utr5 = tx1.blocks.iter().filter(|b| b.kind == BlockKind::Utr5).count();
    let utr3 = tx1.blocks.iter().filter(|b| b.kind == BlockKind::Utr3).count();
    assert_eq!(cds, 3);
    assert_eq!(utr5, 1);
    assert_eq!(utr3, 1);
    assert_eq!(tx1.strand, Strand::Forward);
}

#[tokio::test]
async fn gff3_uses_exon_when_no_cds_in_transcript() {
    let src = NoodlesGffSource::open(
        Path::new("tests/data/sample.gff3"),
        AnnotationFormat::Gff3,
    )
    .await
    .unwrap();
    let region = Region::new("chr1", 1, 1000).unwrap();
    let txs = src.fetch(&region).await.unwrap();
    let tx2 = txs.iter().find(|t| t.id == "tx2").expect("tx2 missing");
    let exons = tx2.blocks.iter().filter(|b| b.kind == BlockKind::Exon).count();
    assert_eq!(exons, 2);
}

#[tokio::test]
async fn gtf_returns_one_transcript_with_three_cds() {
    let src = NoodlesGffSource::open(
        Path::new("tests/data/sample.gtf"),
        AnnotationFormat::Gtf,
    )
    .await
    .unwrap();
    let region = Region::new("chr1", 1, 1000).unwrap();
    let txs = src.fetch(&region).await.unwrap();
    let tx = txs.iter().find(|t| t.id == "tx1").expect("tx1 missing");
    assert_eq!(tx.blocks.iter().filter(|b| b.kind == BlockKind::Cds).count(), 3);
    assert_eq!(tx.kind, TranscriptKind::Mrna);
}

#[tokio::test]
async fn gff3_returns_empty_outside_chrom() {
    let src = NoodlesGffSource::open(
        Path::new("tests/data/sample.gff3"),
        AnnotationFormat::Gff3,
    )
    .await
    .unwrap();
    let region = Region::new("chrZ", 1, 1000).unwrap();
    let txs = src.fetch(&region).await.unwrap();
    assert!(txs.is_empty());
}

#[tokio::test]
async fn gff3_find_by_gene_name_matches_all_isoforms() {
    let src = NoodlesGffSource::open(
        Path::new("tests/data/sample.gff3"),
        AnnotationFormat::Gff3,
    )
    .await
    .unwrap();
    let hits = src.find_by_name("GENE1");
    assert_eq!(hits.len(), 2, "expected both tx1 and tx2");
    assert!(hits.iter().all(|(c, _)| c == "chr1"));
    let mut ids: Vec<&str> = hits.iter().map(|(_, t)| t.id.as_str()).collect();
    ids.sort();
    assert_eq!(ids, vec!["tx1", "tx2"]);
}

#[tokio::test]
async fn gff3_find_by_name_is_case_insensitive() {
    let src = NoodlesGffSource::open(
        Path::new("tests/data/sample.gff3"),
        AnnotationFormat::Gff3,
    )
    .await
    .unwrap();
    assert_eq!(src.find_by_name("gene1").len(), 2);
    assert_eq!(src.find_by_name("Gene1").len(), 2);
}

#[tokio::test]
async fn gff3_find_by_transcript_id_matches_one() {
    let src = NoodlesGffSource::open(
        Path::new("tests/data/sample.gff3"),
        AnnotationFormat::Gff3,
    )
    .await
    .unwrap();
    let hits = src.find_by_name("tx1");
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].1.id, "tx1");
}

#[tokio::test]
async fn gff3_find_by_unknown_returns_empty() {
    let src = NoodlesGffSource::open(
        Path::new("tests/data/sample.gff3"),
        AnnotationFormat::Gff3,
    )
    .await
    .unwrap();
    assert!(src.find_by_name("BRCA1").is_empty());
    assert!(src.find_by_name("").is_empty());
    assert!(src.find_by_name("   ").is_empty());
}

#[tokio::test]
async fn gtf_find_by_gene_id_matches() {
    let src = NoodlesGffSource::open(
        Path::new("tests/data/sample.gtf"),
        AnnotationFormat::Gtf,
    )
    .await
    .unwrap();
    // sample.gtf has gene_id="gene1", gene_name="GENE1", transcript_id="tx1".
    // Both gene_id and gene_name should resolve.
    let by_gene_id = src.find_by_name("gene1");
    assert_eq!(by_gene_id.len(), 1);
    assert_eq!(by_gene_id[0].1.id, "tx1");
    assert_eq!(by_gene_id[0].1.gene_id.as_deref(), Some("gene1"));

    let by_gene_name = src.find_by_name("GENE1");
    assert_eq!(by_gene_name.len(), 1);
    assert_eq!(by_gene_name[0].1.name, "GENE1");
}
