use axum::{
    http::header,
    response::{IntoResponse, Response},
    routing::get,
    Router,
};

use crate::state::ServerState;

const IGV_JS: &[u8] = include_bytes!("../../assets/igv.esm.min.js");

pub fn router() -> Router<ServerState> {
    Router::new().route("/assets/igv.esm.min.js", get(igvjs))
}

async fn igvjs() -> Response {
    (
        [
            (
                header::CONTENT_TYPE,
                "application/javascript; charset=utf-8",
            ),
            (header::CACHE_CONTROL, "public, max-age=31536000, immutable"),
        ],
        IGV_JS,
    )
        .into_response()
}
