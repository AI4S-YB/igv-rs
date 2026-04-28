//! Library surface of `igv-tui` so integration tests can reach widgets,
//! state, and the loader. The binary entrypoint lives in `main.rs` and
//! consumes this same module tree.

pub mod app;
pub mod cli;
pub mod command;
pub mod input;
pub mod logging;
pub mod snapshot;
pub mod ui;
