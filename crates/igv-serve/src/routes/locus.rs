use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Json, Response},
    routing::post,
    Router,
};
use serde::Deserialize;
use serde_json::json;

use igv_core::region::Region;

use crate::state::{BrowserCommand, ServerState};

pub fn router() -> Router<ServerState> {
    Router::new().route("/api/locus", post(handler))
}

#[derive(Debug, Deserialize)]
struct LocusBody {
    locus: String,
}

async fn handler(State(s): State<ServerState>, Json(body): Json<LocusBody>) -> Response {
    let trimmed = body.locus.trim();
    if trimmed.is_empty() || trimmed.len() > 128 {
        return (StatusCode::BAD_REQUEST, "invalid locus").into_response();
    }
    if Region::parse(trimmed).is_err() {
        return (StatusCode::BAD_REQUEST, "unparseable region").into_response();
    }
    let _ = s.commands.send(BrowserCommand::Goto(trimmed.to_string()));
    Json(json!({ "ok": true, "locus": trimmed })).into_response()
}
