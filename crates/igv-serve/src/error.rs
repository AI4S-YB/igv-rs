use std::io;

#[derive(Debug, thiserror::Error)]
pub enum ServeError {
    #[error("failed to bind to {addr}: {source}")]
    BindFailed {
        addr: std::net::SocketAddr,
        #[source]
        source: io::Error,
    },
    #[error("missing index sibling for {path}")]
    MissingIndex { path: std::path::PathBuf },
    #[error("io error: {0}")]
    Io(#[from] io::Error),
}
