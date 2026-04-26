//! Error types shared across `igv-core`.

use std::io;
use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum IgvError {
    #[error("I/O error on {path:?}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: io::Error,
    },

    #[error("missing index file: {0}")]
    MissingIndex(PathBuf),

    #[error("chromosome not found: {0}")]
    UnknownChromosome(String),

    #[error("invalid region string: {0}")]
    InvalidRegion(String),

    #[error("region out of bounds: {chrom} length {chrom_len}, requested {start}-{end}")]
    OutOfBounds {
        chrom: String,
        chrom_len: u64,
        start: u64,
        end: u64,
    },

    #[error("noodles error: {0}")]
    Noodles(String),

    #[error("unexpected: {0}")]
    Other(String),
}

impl IgvError {
    pub fn io(path: impl Into<PathBuf>, source: io::Error) -> Self {
        Self::Io {
            path: path.into(),
            source,
        }
    }

    pub fn noodles<E: std::fmt::Display>(err: E) -> Self {
        Self::Noodles(err.to_string())
    }
}

pub type Result<T, E = IgvError> = std::result::Result<T, E>;
