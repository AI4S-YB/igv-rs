use std::path::{Path, PathBuf};

use axum::{
    extract::{Path as AxumPath, State},
    http::{Request, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
    Router,
};
use tower::ServiceExt; // for `.oneshot`
use tower_http::services::ServeFile;

use crate::state::ServerState;

pub fn router() -> Router<ServerState> {
    Router::new()
        .route("/file/fasta", get(serve_fasta))
        .route("/file/fasta.fai", get(serve_fasta_fai))
        .route("/file/bam/:idx_with_suffix", get(serve_bam_or_bai))
        .route("/file/vcf/:idx_with_suffix", get(serve_vcf_or_tbi))
        .route("/file/signal/:idx", get(serve_signal))
}

async fn serve_path(path: PathBuf, req: Request<axum::body::Body>) -> Response {
    if !path.exists() {
        return (StatusCode::NOT_FOUND, "not found").into_response();
    }
    match ServeFile::new(&path).oneshot(req).await {
        Ok(resp) => resp.into_response(),
        Err(err) => {
            tracing::error!(?err, ?path, "ServeFile error");
            (StatusCode::INTERNAL_SERVER_ERROR, "io error").into_response()
        }
    }
}

fn sibling(path: &Path, ext: &str) -> PathBuf {
    let mut s = path.as_os_str().to_owned();
    s.push(format!(".{ext}"));
    PathBuf::from(s)
}

async fn serve_fasta(State(s): State<ServerState>, req: Request<axum::body::Body>) -> Response {
    serve_path(s.fasta_path.clone(), req).await
}
async fn serve_fasta_fai(State(s): State<ServerState>, req: Request<axum::body::Body>) -> Response {
    serve_path(sibling(&s.fasta_path, "fai"), req).await
}

/// Parse a path segment that may be plain `idx` or `idx.ext`.
/// Returns `(index, Some("ext"))` or `(index, None)`.
fn parse_idx_suffix(raw: &str) -> Option<(usize, Option<&str>)> {
    if let Some((num, ext)) = raw.split_once('.') {
        let idx = num.parse::<usize>().ok()?;
        Some((idx, Some(ext)))
    } else {
        let idx = raw.parse::<usize>().ok()?;
        Some((idx, None))
    }
}

async fn serve_bam_or_bai(
    State(s): State<ServerState>,
    AxumPath(raw): AxumPath<String>,
    req: Request<axum::body::Body>,
) -> Response {
    let Some((idx, suffix)) = parse_idx_suffix(&raw) else {
        return (StatusCode::NOT_FOUND, "no such bam").into_response();
    };
    let Some(t) = s.bams.get(idx) else {
        return (StatusCode::NOT_FOUND, "no such bam").into_response();
    };
    let path = match suffix {
        Some("bai") => sibling(&t.path, "bai"),
        None => t.path.clone(),
        _ => return (StatusCode::NOT_FOUND, "not found").into_response(),
    };
    serve_path(path, req).await
}

async fn serve_vcf_or_tbi(
    State(s): State<ServerState>,
    AxumPath(raw): AxumPath<String>,
    req: Request<axum::body::Body>,
) -> Response {
    let Some((idx, suffix)) = parse_idx_suffix(&raw) else {
        return (StatusCode::NOT_FOUND, "no such vcf").into_response();
    };
    let Some(t) = s.vcfs.get(idx) else {
        return (StatusCode::NOT_FOUND, "no such vcf").into_response();
    };
    let path = match suffix {
        Some("tbi") => sibling(&t.path, "tbi"),
        None => t.path.clone(),
        _ => return (StatusCode::NOT_FOUND, "not found").into_response(),
    };
    serve_path(path, req).await
}

async fn serve_signal(
    State(s): State<ServerState>,
    AxumPath(idx): AxumPath<usize>,
    req: Request<axum::body::Body>,
) -> Response {
    let Some(t) = s.signals.get(idx) else {
        return (StatusCode::NOT_FOUND, "no such signal").into_response();
    };
    serve_path(t.path.clone(), req).await
}
