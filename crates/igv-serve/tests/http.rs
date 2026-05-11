use std::sync::Arc;

use igv_serve::{spawn, ServerConfig, TrackEntry, ViewEvent};

pub async fn empty_config() -> (ServerConfig, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let fasta_path = dir.path().join("ref.fa");
    std::fs::write(&fasta_path, b">chr1\nACGT\n").unwrap();
    std::fs::write(dir.path().join("ref.fa.fai"), b"chr1\t4\t6\t4\t5\n").unwrap();
    let fasta = igv_core::source::NoodlesFastaSource::open(&fasta_path)
        .await
        .unwrap();
    let cfg = ServerConfig {
        bind: std::net::IpAddr::from([127, 0, 0, 1]),
        port: 0,
        fasta: Arc::new(fasta),
        fasta_path,
        bams: vec![],
        vcfs: vec![],
        annotations: vec![],
        signals: vec![],
        links: vec![],
        initial: ViewEvent { chrom: "chr1".into(), start: 0, end: 4 },
        link_min_score: None,
    };
    (cfg, dir)
}

#[tokio::test]
async fn root_serves_html_with_igv_module() {
    let (cfg, _dir) = empty_config().await;
    let h = spawn(cfg).await.unwrap();
    let body = reqwest::get(format!("http://{}/", h.addr))
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(body.contains("/assets/igv.esm.min.js"));
    h.shutdown().await;
}

#[tokio::test]
async fn assets_serves_igvjs() {
    let (cfg, _dir) = empty_config().await;
    let h = spawn(cfg).await.unwrap();
    let resp = reqwest::get(format!("http://{}/assets/igv.esm.min.js", h.addr))
        .await
        .unwrap();
    assert!(resp.status().is_success());
    let ct = resp.headers().get("content-type").unwrap().to_str().unwrap();
    assert!(ct.starts_with("application/javascript"));
    assert!(resp.bytes().await.unwrap().len() > 1000);
    h.shutdown().await;
}

#[tokio::test]
async fn api_config_emits_reference_and_locus() {
    let (cfg, _dir) = empty_config().await;
    let h = spawn(cfg).await.unwrap();
    let body: serde_json::Value = reqwest::get(format!("http://{}/api/config", h.addr))
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(body["reference"]["fastaURL"], "/file/fasta");
    assert_eq!(body["reference"]["indexURL"], "/file/fasta.fai");
    assert_eq!(body["locus"], "chr1:0-4");
    assert!(body["tracks"].as_array().unwrap().is_empty());
    h.shutdown().await;
}

#[tokio::test]
async fn file_fasta_supports_range() {
    let (cfg, _dir) = empty_config().await;
    let h = spawn(cfg).await.unwrap();
    // empty_config writes ">chr1\nACGT\n" (11 bytes). Request a 4-byte
    // range starting at offset 6 — that's "ACGT".
    let resp = reqwest::Client::new()
        .get(format!("http://{}/file/fasta", h.addr))
        .header("Range", "bytes=6-9")
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), reqwest::StatusCode::PARTIAL_CONTENT);
    let body = resp.bytes().await.unwrap();
    assert_eq!(&body[..], b"ACGT");
    h.shutdown().await;
}

#[tokio::test]
async fn file_fasta_index_returns_200() {
    let (cfg, _dir) = empty_config().await;
    let h = spawn(cfg).await.unwrap();
    let resp = reqwest::get(format!("http://{}/file/fasta.fai", h.addr))
        .await
        .unwrap();
    assert!(resp.status().is_success());
    assert!(resp.bytes().await.unwrap().starts_with(b"chr1\t4"));
    h.shutdown().await;
}

#[tokio::test]
async fn file_unknown_kind_returns_404() {
    let (cfg, _dir) = empty_config().await;
    let h = spawn(cfg).await.unwrap();
    let resp = reqwest::get(format!("http://{}/file/bam/0", h.addr))
        .await
        .unwrap();
    assert_eq!(resp.status(), reqwest::StatusCode::NOT_FOUND);
    h.shutdown().await;
}

async fn config_with_bed(bed_body: &str) -> (ServerConfig, tempfile::TempDir, tempfile::TempDir) {
    let (mut cfg, fa_dir) = empty_config().await;
    let bed_dir = tempfile::tempdir().unwrap();
    let bed = bed_dir.path().join("genes.bed");
    std::fs::write(&bed, bed_body).unwrap();
    let src = igv_core::source::open_annotation(&bed, None).await.unwrap();
    cfg.annotations.push(TrackEntry {
        source: src,
        path: bed.clone(),
        display: "genes.bed".into(),
    });
    (cfg, fa_dir, bed_dir)
}

async fn config_with_bedpe(body: &str, min_score: Option<f64>) -> (ServerConfig, tempfile::TempDir, tempfile::TempDir) {
    let (mut cfg, fa_dir) = empty_config().await;
    let bp_dir = tempfile::tempdir().unwrap();
    let bedpe = bp_dir.path().join("loops.bedpe");
    std::fs::write(&bedpe, body).unwrap();
    let src = igv_core::source::open_link(&bedpe, None).await.unwrap();
    cfg.links.push(TrackEntry {
        source: src,
        path: bedpe.clone(),
        display: "loops.bedpe".into(),
    });
    cfg.link_min_score = min_score;
    (cfg, fa_dir, bp_dir)
}

#[tokio::test]
async fn api_features_annotation_returns_overlapping_records() {
    // BED is 0-based half-open on disk; igv-core converts to 1-based inclusive
    // internally, but the JSON we emit uses the source-native coordinates from
    // the parsed record (start/end as returned by `span()`).
    let (cfg, _fa, _bed) = config_with_bed(
        "chr1\t100\t400\tBRCA1\t0\t+\n\
         chr1\t1000\t2000\tFOO\t0\t-\n",
    )
    .await;
    let h = spawn(cfg).await.unwrap();
    let body: serde_json::Value = reqwest::get(format!(
        "http://{}/api/features/annotation/0?chrom=chr1&start=0&end=500",
        h.addr
    ))
    .await
    .unwrap()
    .json()
    .await
    .unwrap();
    let arr = body.as_array().unwrap();
    // Exactly one record (BRCA1) should overlap chr1:0-500. FOO at 1000-2000 is outside.
    assert_eq!(arr.len(), 1, "expected one record, got: {body}");
    assert_eq!(arr[0]["name"], "BRCA1");
    assert_eq!(arr[0]["chr"], "chr1");
    h.shutdown().await;
}

#[tokio::test]
async fn api_features_link_drops_below_min_score() {
    // BEDPE is 0-based half-open on disk. Two cis loops on chr1; score 1.0 vs 9.0.
    let (cfg, _fa, _bp) = config_with_bedpe(
        "chr1\t100\t200\tchr1\t300\t400\tloop_a\t1.0\n\
         chr1\t500\t600\tchr1\t700\t800\tloop_b\t9.0\n",
        Some(5.0),
    )
    .await;
    let h = spawn(cfg).await.unwrap();
    let body: serde_json::Value = reqwest::get(format!(
        "http://{}/api/features/link/0?chrom=chr1&start=0&end=1000",
        h.addr
    ))
    .await
    .unwrap()
    .json()
    .await
    .unwrap();
    let arr = body.as_array().unwrap();
    assert_eq!(arr.len(), 1, "expected only loop_b (score>=5.0), got: {body}");
    assert_eq!(arr[0]["name"], "loop_b");
    h.shutdown().await;
}
