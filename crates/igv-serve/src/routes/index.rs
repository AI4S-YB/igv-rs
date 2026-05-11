use axum::{response::Html, routing::get, Router};

use crate::state::ServerState;

pub fn router() -> Router<ServerState> {
    Router::new().route("/", get(handler))
}

async fn handler() -> Html<&'static str> {
    Html(include_str!("../../assets/index.html"))
}
