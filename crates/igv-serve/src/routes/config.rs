use axum::{extract::State, response::Json, routing::get, Router};
use serde_json::{json, Value};

use crate::state::ServerState;

pub fn router() -> Router<ServerState> {
    Router::new().route("/api/config", get(handler))
}

async fn handler(State(state): State<ServerState>) -> Json<Value> {
    let mut tracks: Vec<Value> = Vec::new();

    for (idx, t) in state.bams.iter().enumerate() {
        let lower = t.path.to_string_lossy().to_ascii_lowercase();
        let (fmt, idx_ext) = if lower.ends_with(".cram") {
            ("cram", "crai")
        } else {
            ("bam", "bai")
        };
        tracks.push(json!({
            "name": t.display,
            "type": "alignment",
            "format": fmt,
            "url": format!("/file/bam/{idx}"),
            "indexURL": format!("/file/bam/{idx}.{idx_ext}"),
        }));
    }
    for (idx, t) in state.vcfs.iter().enumerate() {
        let lower = t.path.to_string_lossy().to_ascii_lowercase();
        let (fmt, idx_ext) = if lower.ends_with(".bcf") {
            ("bcf", "csi")
        } else {
            ("vcf", "tbi")
        };
        tracks.push(json!({
            "name": t.display,
            "type": "variant",
            "format": fmt,
            "url": format!("/file/vcf/{idx}"),
            "indexURL": format!("/file/vcf/{idx}.{idx_ext}"),
        }));
    }
    for (idx, t) in state.signals.iter().enumerate() {
        let lower = t.path.to_string_lossy().to_ascii_lowercase();
        let fmt = if lower.ends_with(".bw") || lower.ends_with(".bigwig") {
            "bigwig"
        } else {
            "wig"
        };
        tracks.push(json!({
            "name": t.display,
            "type": "wig",
            "format": fmt,
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
    for (idx, t) in state.segs.iter().enumerate() {
        tracks.push(json!({
            "name": t.display,
            "type": "seg",
            "sourceType": "custom",
            "source": {
                "url": format!("/api/features/seg/{idx}"),
                "queryable": true,
            }
        }));
    }
    for (idx, t) in state.gwas.iter().enumerate() {
        tracks.push(json!({
            "name": t.display,
            "type": "gwas",
            "sourceType": "custom",
            "source": {
                "url": format!("/api/features/gwas/{idx}"),
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
