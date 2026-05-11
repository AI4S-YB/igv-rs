//! Local HTTP server that mirrors the TUI's view in igv.js.
//!
//! See `docs/superpowers/specs/2026-05-11-browser-serve-design.md`.

#![forbid(unsafe_code)]
#![warn(rust_2018_idioms, missing_debug_implementations)]

pub mod error;
pub mod state;
pub mod view;

pub use error::ServeError;
pub use state::{ServerState, TrackEntry};
pub use view::{ViewEvent, ViewSnapshot};
