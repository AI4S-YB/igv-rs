//! Unified local and remote byte-range I/O layer with LRU page caching.
//!
//! Supports local paths, `http://` / `https://`, `s3://` (virtual-hosted HTTPS),
//! and `gs://` (Google Cloud Storage XML/JSON HTTPS public endpoints).

use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use flate2::read::MultiGzDecoder;
use lru::LruCache;

use crate::error::{IgvError, Result};

const PAGE_SIZE: u64 = 64 * 1024;
const PAGE_CACHE_CAP: usize = 256;

/// Parsed data resource location (local filesystem path or remote URL).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Location {
    Local(PathBuf),
    Remote(String),
}

impl Location {
    /// Parse a user-supplied path or URI, normalizing `s3://`, `gs://`, and `file://`.
    pub fn parse(raw: &str) -> Self {
        let trimmed = raw.trim();
        if let Some(rest) = trimmed.strip_prefix("s3://") {
            let (bucket, key) = rest.split_once('/').unwrap_or((rest, ""));
            return Self::Remote(format!("https://{bucket}.s3.amazonaws.com/{key}"));
        }
        if let Some(rest) = trimmed.strip_prefix("gs://") {
            let (bucket, key) = rest.split_once('/').unwrap_or((rest, ""));
            return Self::Remote(format!("https://storage.googleapis.com/{bucket}/{key}"));
        }
        if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
            return Self::Remote(trimmed.to_string());
        }
        if let Some(path) = trimmed.strip_prefix("file://") {
            return Self::Local(PathBuf::from(path));
        }
        Self::Local(PathBuf::from(trimmed))
    }

    pub fn is_remote(&self) -> bool {
        matches!(self, Self::Remote(_))
    }

    pub fn as_str(&self) -> String {
        match self {
            Self::Local(p) => p.display().to_string(),
            Self::Remote(u) => u.clone(),
        }
    }

    pub fn display_name(&self) -> String {
        match self {
            Self::Local(p) => p
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("track")
                .to_string(),
            Self::Remote(u) => {
                let no_q = u.split('?').next().unwrap_or(u);
                no_q.rsplit('/')
                    .find(|s| !s.is_empty())
                    .unwrap_or("remote")
                    .to_string()
            }
        }
    }

    pub fn lowercase_name(&self) -> String {
        match self {
            Self::Local(p) => p
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("")
                .to_ascii_lowercase(),
            Self::Remote(u) => {
                let no_q = u.split('?').next().unwrap_or(u);
                no_q.rsplit('/')
                    .next()
                    .unwrap_or("")
                    .to_ascii_lowercase()
            }
        }
    }

    /// Construct a sibling file path/URL by appending `.{ext}` before any query string.
    pub fn sibling(&self, ext: &str) -> Self {
        match self {
            Self::Local(p) => {
                let mut s = p.as_os_str().to_owned();
                s.push(format!(".{ext}"));
                Self::Local(PathBuf::from(s))
            }
            Self::Remote(u) => {
                if let Some((base, q)) = u.split_once('?') {
                    Self::Remote(format!("{base}.{ext}?{q}"))
                } else {
                    Self::Remote(format!("{u}.{ext}"))
                }
            }
        }
    }

    /// Replace the final path component with `relative` (or resolve relative to parent).
    pub fn resolve_relative(&self, relative: &str) -> Self {
        if relative.starts_with("http://")
            || relative.starts_with("https://")
            || relative.starts_with("s3://")
            || relative.starts_with("gs://")
        {
            return Self::parse(relative);
        }
        match self {
            Self::Local(p) => {
                let parent = p.parent().unwrap_or_else(|| Path::new("."));
                Self::Local(parent.join(relative))
            }
            Self::Remote(u) => {
                let (base, q) = u.split_once('?').unwrap_or((u, ""));
                let dir = match base.rsplit_once('/') {
                    Some((d, _)) => d,
                    None => base,
                };
                let joined = format!("{}/{}", dir, relative.trim_start_matches("./"));
                if q.is_empty() {
                    Self::Remote(joined)
                } else {
                    Self::Remote(format!("{joined}?{q}"))
                }
            }
        }
    }
}

/// Shared HTTP byte-range backend with 64 KiB LRU page caching.
pub struct HttpRangeBackend {
    url: String,
    client: reqwest::Client,
    handle: tokio::runtime::Handle,
    auth_token: Option<String>,
    len: u64,
    fallback_body: Option<Vec<u8>>,
    cache: Mutex<LruCache<u64, Arc<Vec<u8>>>>,
}

impl std::fmt::Debug for HttpRangeBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HttpRangeBackend")
            .field("url", &self.url)
            .field("len", &self.len)
            .finish_non_exhaustive()
    }
}

impl HttpRangeBackend {
    pub async fn open(url: String, auth_token: Option<String>) -> Result<Arc<Self>> {
        let handle = tokio::runtime::Handle::current();
        let client = reqwest::Client::builder()
            .user_agent("igv-rs/0.8")
            .build()
            .map_err(|e| IgvError::Other(format!("http client: {e}")))?;

        let mut head_req = client.head(&url);
        if let Some(tok) = &auth_token {
            head_req = head_req.bearer_auth(tok);
        }

        let mut len_opt: Option<u64> = None;
        let mut fallback_body: Option<Vec<u8>> = None;

        if let Ok(resp) = head_req.send().await {
            if resp.status().is_success() {
                if let Some(cl) = resp
                    .headers()
                    .get(reqwest::header::CONTENT_LENGTH)
                    .and_then(|v| v.to_str().ok())
                    .and_then(|s| s.parse::<u64>().ok())
                {
                    len_opt = Some(cl);
                }
            }
        }

        if len_opt.is_none() {
            let mut probe = client
                .get(&url)
                .header(reqwest::header::RANGE, "bytes=0-0");
            if let Some(tok) = &auth_token {
                probe = probe.bearer_auth(tok);
            }
            let resp = probe
                .send()
                .await
                .map_err(|e| IgvError::Other(format!("http range probe {url}: {e}")))?;
            if !resp.status().is_success() && resp.status() != reqwest::StatusCode::PARTIAL_CONTENT {
                return Err(IgvError::Other(format!(
                    "http status {} probing {url}",
                    resp.status()
                )));
            }
            if let Some(cr) = resp
                .headers()
                .get(reqwest::header::CONTENT_RANGE)
                .and_then(|v| v.to_str().ok())
            {
                if let Some((_, total_str)) = cr.rsplit_once('/') {
                    if let Ok(total) = total_str.trim().parse::<u64>() {
                        len_opt = Some(total);
                    }
                }
            }
            if len_opt.is_none() {
                // Server ignored Range and sent full body (or small resource).
                let bytes = resp
                    .bytes()
                    .await
                    .map_err(|e| IgvError::Other(format!("http body {url}: {e}")))?
                    .to_vec();
                len_opt = Some(bytes.len() as u64);
                fallback_body = Some(bytes);
            }
        }

        let len = len_opt.ok_or_else(|| {
            IgvError::Other(format!("could not determine Content-Length for {url}"))
        })?;

        Ok(Arc::new(Self {
            url,
            client,
            handle,
            auth_token,
            len,
            fallback_body,
            cache: Mutex::new(LruCache::new(
                NonZeroUsize::new(PAGE_CACHE_CAP).expect("non-zero cache"),
            )),
        }))
    }

    pub fn len(&self) -> u64 {
        self.len
    }

    fn fetch_page(&self, page_idx: u64) -> io::Result<Arc<Vec<u8>>> {
        {
            let mut guard = self
                .cache
                .lock()
                .map_err(|_| io::Error::other("cache poisoned"))?;
            if let Some(cached) = guard.get(&page_idx) {
                return Ok(Arc::clone(cached));
            }
        }

        let start = page_idx.saturating_mul(PAGE_SIZE);
        if start >= self.len {
            return Ok(Arc::new(Vec::new()));
        }
        let end = (start + PAGE_SIZE - 1).min(self.len.saturating_sub(1));

        let data: Vec<u8> = if let Some(body) = &self.fallback_body {
            let s = start as usize;
            let e = (end as usize + 1).min(body.len());
            body[s..e].to_vec()
        } else {
            let client = self.client.clone();
            let url = self.url.clone();
            let auth = self.auth_token.clone();
            self.handle.block_on(async move {
                let mut req = client
                    .get(&url)
                    .header(reqwest::header::RANGE, format!("bytes={start}-{end}"));
                if let Some(tok) = &auth {
                    req = req.bearer_auth(tok);
                }
                let resp = req
                    .send()
                    .await
                    .map_err(|e| io::Error::other(format!("http range get: {e}")))?;
                if !resp.status().is_success()
                    && resp.status() != reqwest::StatusCode::PARTIAL_CONTENT
                {
                    return Err(io::Error::other(format!(
                        "http status {} on range {start}-{end}",
                        resp.status()
                    )));
                }
                let bytes = resp
                    .bytes()
                    .await
                    .map_err(|e| io::Error::other(format!("http read bytes: {e}")))?;
                Ok::<Vec<u8>, io::Error>(bytes.to_vec())
            })?
        };

        let arc = Arc::new(data);
        let mut guard = self
            .cache
            .lock()
            .map_err(|_| io::Error::other("cache poisoned"))?;
        guard.put(page_idx, Arc::clone(&arc));
        Ok(arc)
    }

    pub fn read_at(&self, offset: u64, dst: &mut [u8]) -> io::Result<usize> {
        if offset >= self.len || dst.is_empty() {
            return Ok(0);
        }
        let mut written = 0usize;
        let max_to_read = dst.len().min((self.len - offset) as usize);
        while written < max_to_read {
            let cur = offset + written as u64;
            let page_idx = cur / PAGE_SIZE;
            let in_page = (cur % PAGE_SIZE) as usize;
            let page = self.fetch_page(page_idx)?;
            if in_page >= page.len() {
                break;
            }
            let avail = (page.len() - in_page).min(max_to_read - written);
            dst[written..written + avail].copy_from_slice(&page[in_page..in_page + avail]);
            written += avail;
            if avail == 0 {
                break;
            }
        }
        Ok(written)
    }
}

/// Synchronous `Read + Seek` stream backed by either a local `File`, an in-memory
/// buffer, or an `HttpRangeBackend` (safe to use inside `tokio::task::spawn_blocking`).
pub enum SeekableStream {
    Local {
        file: File,
        path: PathBuf,
        len: u64,
        pos: u64,
    },
    Remote {
        backend: Arc<HttpRangeBackend>,
        pos: u64,
    },
    Memory {
        data: Arc<Vec<u8>>,
        name: String,
        pos: u64,
    },
}

impl std::fmt::Debug for SeekableStream {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Local { path, len, pos, .. } => f
                .debug_struct("SeekableStream::Local")
                .field("path", path)
                .field("len", len)
                .field("pos", pos)
                .finish(),
            Self::Remote { backend, pos } => f
                .debug_struct("SeekableStream::Remote")
                .field("backend", backend)
                .field("pos", pos)
                .finish(),
            Self::Memory { name, data, pos } => f
                .debug_struct("SeekableStream::Memory")
                .field("name", name)
                .field("len", &data.len())
                .field("pos", pos)
                .finish(),
        }
    }
}

impl SeekableStream {
    pub async fn open(loc: &Location, auth_token: Option<&str>) -> Result<Self> {
        match loc {
            Location::Local(p) => {
                let p_clone = p.clone();
                tokio::task::spawn_blocking(move || Self::open_local(&p_clone))
                    .await
                    .map_err(|e| IgvError::Other(e.to_string()))?
            }
            Location::Remote(url) => {
                let backend =
                    HttpRangeBackend::open(url.clone(), auth_token.map(str::to_string)).await?;
                Ok(Self::Remote { backend, pos: 0 })
            }
        }
    }

    pub fn open_local(path: &Path) -> Result<Self> {
        let file = File::open(path).map_err(|e| IgvError::io(path, e))?;
        let len = file.metadata().map_err(|e| IgvError::io(path, e))?.len();
        Ok(Self::Local {
            file,
            path: path.to_path_buf(),
            len,
            pos: 0,
        })
    }

    pub fn from_bytes(name: impl Into<String>, data: Vec<u8>) -> Self {
        Self::Memory {
            data: Arc::new(data),
            name: name.into(),
            pos: 0,
        }
    }

    pub fn total_len(&self) -> u64 {
        match self {
            Self::Local { len, .. } => *len,
            Self::Remote { backend, .. } => backend.len(),
            Self::Memory { data, .. } => data.len() as u64,
        }
    }

    pub fn clone_handle(&self) -> io::Result<Self> {
        match self {
            Self::Local { path, len, .. } => Ok(Self::Local {
                file: File::open(path)?,
                path: path.clone(),
                len: *len,
                pos: 0,
            }),
            Self::Remote { backend, .. } => Ok(Self::Remote {
                backend: Arc::clone(backend),
                pos: 0,
            }),
            Self::Memory { data, name, .. } => Ok(Self::Memory {
                data: Arc::clone(data),
                name: name.clone(),
                pos: 0,
            }),
        }
    }
}

impl Read for SeekableStream {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match self {
            Self::Local { file, pos, .. } => {
                let n = file.read(buf)?;
                *pos += n as u64;
                Ok(n)
            }
            Self::Remote { backend, pos } => {
                let n = backend.read_at(*pos, buf)?;
                *pos += n as u64;
                Ok(n)
            }
            Self::Memory { data, pos, .. } => {
                let off = (*pos as usize).min(data.len());
                let avail = (data.len() - off).min(buf.len());
                buf[..avail].copy_from_slice(&data[off..off + avail]);
                *pos += avail as u64;
                Ok(avail)
            }
        }
    }
}

impl Seek for SeekableStream {
    fn seek(&mut self, style: SeekFrom) -> io::Result<u64> {
        let len = self.total_len() as i128;
        let cur = match self {
            Self::Local { file, pos, .. } => {
                let next = file.seek(style)?;
                *pos = next;
                return Ok(next);
            }
            Self::Remote { pos, .. } | Self::Memory { pos, .. } => *pos as i128,
        };
        let target = match style {
            SeekFrom::Start(n) => n as i128,
            SeekFrom::End(d) => len + d as i128,
            SeekFrom::Current(d) => cur + d as i128,
        };
        if target < 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "seek before start of stream",
            ));
        }
        let next = target as u64;
        match self {
            Self::Remote { pos, .. } | Self::Memory { pos, .. } => *pos = next,
            Self::Local { .. } => unreachable!(),
        }
        Ok(next)
    }
}

/// Fetch all bytes from a local file or remote URL.
pub async fn fetch_bytes(loc: &Location, auth_token: Option<&str>) -> Result<Vec<u8>> {
    match loc {
        Location::Local(p) => tokio::fs::read(p)
            .await
            .map_err(|e| IgvError::io(p, e)),
        Location::Remote(url) => {
            let client = reqwest::Client::builder()
                .user_agent("igv-rs/0.8")
                .build()
                .map_err(|e| IgvError::Other(format!("http client: {e}")))?;
            let mut req = client.get(url);
            if let Some(tok) = auth_token {
                req = req.bearer_auth(tok);
            }
            let resp = req
                .send()
                .await
                .map_err(|e| IgvError::Other(format!("http get {url}: {e}")))?;
            if !resp.status().is_success() {
                return Err(IgvError::Other(format!(
                    "http status {} fetching {url}",
                    resp.status()
                )));
            }
            resp.bytes()
                .await
                .map(|b| b.to_vec())
                .map_err(|e| IgvError::Other(format!("http read {url}: {e}")))
        }
    }
}

/// Check if a local file exists or remote resource responds with success/206.
pub async fn location_exists(loc: &Location, auth_token: Option<&str>) -> bool {
    match loc {
        Location::Local(p) => p.exists(),
        Location::Remote(url) => {
            let Ok(client) = reqwest::Client::builder().user_agent("igv-rs/0.8").build() else {
                return false;
            };
            let mut req = client.head(url);
            if let Some(tok) = auth_token {
                req = req.bearer_auth(tok);
            }
            if let Ok(resp) = req.send().await {
                if resp.status().is_success() {
                    return true;
                }
            }
            let mut req = client
                .get(url)
                .header(reqwest::header::RANGE, "bytes=0-0");
            if let Some(tok) = auth_token {
                req = req.bearer_auth(tok);
            }
            req.send()
                .await
                .is_ok_and(|r| r.status().is_success() || r.status() == reqwest::StatusCode::PARTIAL_CONTENT)
        }
    }
}

/// Decompress gzipped bytes if they start with the gzip magic header (`0x1f, 0x8b`),
/// returning raw bytes otherwise.
pub fn maybe_gunzip(bytes: Vec<u8>) -> Result<Vec<u8>> {
    if bytes.len() >= 2 && bytes[0] == 0x1f && bytes[1] == 0x8b {
        let mut dec = MultiGzDecoder::new(&bytes[..]);
        let mut out = Vec::new();
        dec.read_to_end(&mut out)
            .map_err(|e| IgvError::Other(format!("gzip decode: {e}")))?;
        Ok(out)
    } else {
        Ok(bytes)
    }
}

/// Fetch a local or remote text file, transparently gunzipping `.gz`/BGZF payloads.
pub async fn fetch_text_maybe_gz(loc: &Location, auth_token: Option<&str>) -> Result<String> {
    let raw = fetch_bytes(loc, auth_token).await?;
    let plain = maybe_gunzip(raw)?;
    Ok(String::from_utf8_lossy(&plain).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        extract::State,
        http::{header, HeaderMap, StatusCode},
        response::{IntoResponse, Response},
        routing::get,
        Router,
    };

    #[test]
    fn parse_cloud_and_local_locations() {
        assert_eq!(
            Location::parse("s3://my-bucket/dir/sample.bam"),
            Location::Remote("https://my-bucket.s3.amazonaws.com/dir/sample.bam".into())
        );
        assert_eq!(
            Location::parse("gs://gcp-bucket/a.bw"),
            Location::Remote("https://storage.googleapis.com/gcp-bucket/a.bw".into())
        );
        assert_eq!(
            Location::parse("file:///tmp/x.fa"),
            Location::Local(PathBuf::from("/tmp/x.fa"))
        );
        assert_eq!(
            Location::parse("https://example.org/a.bam?token=1")
                .sibling("bai"),
            Location::Remote("https://example.org/a.bam.bai?token=1".into())
        );
    }

    async fn range_handler(
        State(data): State<Arc<Vec<u8>>>,
        headers: HeaderMap,
    ) -> Response {
        if let Some(range) = headers.get(header::RANGE).and_then(|v| v.to_str().ok()) {
            if let Some(spec) = range.strip_prefix("bytes=") {
                let parts: Vec<&str> = spec.split('-').collect();
                if parts.len() == 2 {
                    let start: usize = parts[0].parse().unwrap_or(0);
                    let end: usize = parts[1].parse().unwrap_or(data.len().saturating_sub(1));
                    let end = end.min(data.len().saturating_sub(1));
                    if start <= end && start < data.len() {
                        let slice = data[start..=end].to_vec();
                        let cr = format!("bytes {start}-{end}/{}", data.len());
                        return (
                            StatusCode::PARTIAL_CONTENT,
                            [
                                (header::CONTENT_RANGE, cr),
                                (header::ACCEPT_RANGES, "bytes".to_string()),
                                (header::CONTENT_LENGTH, slice.len().to_string()),
                            ],
                            slice,
                        )
                            .into_response();
                    }
                }
            }
        }
        (
            StatusCode::OK,
            [(header::CONTENT_LENGTH, data.len().to_string())],
            data.as_ref().clone(),
        )
            .into_response()
    }

    #[tokio::test]
    async fn http_range_backend_reads_and_seeks_across_pages() {
        let payload: Vec<u8> = (0..150_000u32).map(|i| (i % 251) as u8).collect();
        let state = Arc::new(payload.clone());
        let app = Router::new()
            .route("/blob.bin", get(range_handler))
            .with_state(state);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });

        let loc = Location::Remote(format!("http://{addr}/blob.bin"));
        let mut stream = SeekableStream::open(&loc, None).await.unwrap();
        let got = tokio::task::spawn_blocking(move || -> io::Result<Vec<u8>> {
            stream.seek(SeekFrom::Start(65_500))?;
            let mut buf = vec![0u8; 100];
            stream.read_exact(&mut buf)?;
            Ok(buf)
        })
        .await
        .unwrap()
        .unwrap();

        assert_eq!(&got[..], &payload[65_500..65_600]);
    }
}
