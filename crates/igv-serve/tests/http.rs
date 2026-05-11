use std::sync::Arc;

use igv_serve::{spawn, ServerConfig, ViewEvent};

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
