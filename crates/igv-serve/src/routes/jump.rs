use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::{IntoResponse, Json, Response},
    routing::get,
    Router,
};
use serde::Deserialize;
use serde_json::json;

use crate::state::ServerState;

pub fn router() -> Router<ServerState> {
    Router::new().route("/api/jump", get(handler))
}

#[derive(Debug, Deserialize)]
struct Q {
    name: String,
}

fn name_is_valid(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 64
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | ':' | '-'))
}

async fn handler(State(s): State<ServerState>, Query(q): Query<Q>) -> Response {
    if !name_is_valid(&q.name) {
        return (StatusCode::BAD_REQUEST, "bad name").into_response();
    }
    let sources: Vec<_> = s.annotations.iter().map(|t| t.source.clone()).collect();
    match igv_core::source::annotation::find_by_name_union(&sources, &q.name) {
        Some((region, _label)) => Json(json!({
            "chrom": region.chrom,
            "start": region.start,
            "end": region.end,
        }))
        .into_response(),
        None => (StatusCode::NOT_FOUND, "not found").into_response(),
    }
}
