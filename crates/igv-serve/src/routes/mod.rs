use axum::Router;

use crate::state::ServerState;

pub mod assets;
pub mod config;
pub mod features;
pub mod file;
pub mod index;
pub mod jump;

pub fn build(state: ServerState) -> Router {
    Router::new()
        .merge(index::router())
        .merge(assets::router())
        .merge(config::router())
        .merge(features::router())
        .merge(file::router())
        .merge(jump::router())
        .with_state(state)
}
