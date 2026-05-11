use axum::{extract::State, response::Json, routing::get, Router};
use serde_json::{json, Value};

use crate::state::ServerState;

pub fn router() -> Router<ServerState> {
    Router::new().route("/api/config", get(handler))
}

async fn handler(State(state): State<ServerState>) -> Json<Value> {
    let mut tracks: Vec<Value> = Vec::new();

    for (idx, t) in state.bams.iter().enumerate() {
        tracks.push(json!({
            "name": t.display,
            "type": "alignment",
            "format": "bam",
            "url": format!("/file/bam/{idx}"),
            "indexURL": format!("/file/bam/{idx}.bai"),
        }));
    }
    for (idx, t) in state.vcfs.iter().enumerate() {
        tracks.push(json!({
            "name": t.display,
            "type": "variant",
            "format": "vcf",
            "url": format!("/file/vcf/{idx}"),
            "indexURL": format!("/file/vcf/{idx}.tbi"),
        }));
    }
    for (idx, t) in state.signals.iter().enumerate() {
        tracks.push(json!({
            "name": t.display,
            "type": "wig",
            "format": "bigwig",
            "url": format!("/file/signal/{idx}"),
        }));
    }
    for (idx, t) in state.annotations.iter().enumerate() {
        tracks.push(json!({
            "name": t.display,
            "type": "annotation",
            "sourceType": "custom",
            "source": {
                "url": format!("/api/features/annotation/{idx}"),
                "queryable": true,
            }
        }));
    }
    for (idx, t) in state.links.iter().enumerate() {
        tracks.push(json!({
            "name": t.display,
            "type": "interact",
            "sourceType": "custom",
            "source": {
                "url": format!("/api/features/link/{idx}"),
                "queryable": true,
            }
        }));
    }

    Json(json!({
        "reference": {
            "id": "user-fasta",
            "name": "Reference",
            "fastaURL": "/file/fasta",
            "indexURL": "/file/fasta.fai",
            "wholeGenomeView": false,
        },
        "locus": format!("{}:{}-{}", state.initial.chrom, state.initial.start, state.initial.end),
        "tracks": tracks,
    }))
}
