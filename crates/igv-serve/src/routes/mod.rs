use axum::Router;

use crate::state::ServerState;

pub mod assets;
pub mod config;
pub mod index;

pub fn build(state: ServerState) -> Router {
    Router::new()
        .merge(index::router())
        .merge(assets::router())
        .merge(config::router())
        .with_state(state)
}
