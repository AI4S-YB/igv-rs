use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Json, Response},
    routing::get,
    Router,
};
use serde::Deserialize;
use serde_json::Value;

use igv_core::region::Region;

use crate::feature_json::{annotation_to_json, link_to_json};
use crate::state::ServerState;

pub fn router() -> Router<ServerState> {
    Router::new()
        .route("/api/features/annotation/:idx", get(annotation))
        .route("/api/features/link/:idx", get(link))
}

#[derive(Debug, Deserialize)]
struct Window {
    chrom: String,
    start: u64,
    end: u64,
}

async fn annotation(
    State(s): State<ServerState>,
    Path(idx): Path<usize>,
    Query(w): Query<Window>,
) -> Response {
    let Some(t) = s.annotations.get(idx) else {
        return (StatusCode::NOT_FOUND, "no such annotation").into_response();
    };
    // Callers may pass start=0 (0-based); clamp to 1 for the 1-based Region.
    let start = w.start.max(1);
    let Ok(region) = Region::new(w.chrom.clone(), start, w.end) else {
        return (StatusCode::BAD_REQUEST, "bad region").into_response();
    };
    match t.source.fetch(&region).await {
        Ok(records) => {
            let arr: Vec<Value> = records
                .iter()
                .map(|tx| annotation_to_json(&w.chrom, tx))
                .collect();
            Json(arr).into_response()
        }
        Err(err) => {
            tracing::error!(?err, "annotation fetch failed");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": format!("{err}") })),
            )
                .into_response()
        }
    }
}

async fn link(
    State(s): State<ServerState>,
    Path(idx): Path<usize>,
    Query(w): Query<Window>,
) -> Response {
    let Some(t) = s.links.get(idx) else {
        return (StatusCode::NOT_FOUND, "no such link").into_response();
    };
    // Callers may pass start=0 (0-based); clamp to 1 for the 1-based Region.
    let start = w.start.max(1);
    let Ok(region) = Region::new(w.chrom.clone(), start, w.end) else {
        return (StatusCode::BAD_REQUEST, "bad region").into_response();
    };
    let opts = igv_core::source::FetchLinkOpts {
        min_score: s.link_min_score,
    };
    match t.source.query(&region, &opts).await {
        Ok(links) => {
            let arr: Vec<Value> = links.iter().map(|vl| link_to_json(&vl.record)).collect();
            Json(arr).into_response()
        }
        Err(err) => {
            tracing::error!(?err, "link query failed");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": format!("{err}") })),
            )
                .into_response()
        }
    }
}
