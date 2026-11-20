//! Reference genome management: `.2bit` random-access reader, remote FAI
//! reader, built-in hosted genome registry, `.json`/`.genome` descriptor
//! loader, cytobands, and bidirectional chromosome alias resolution.

use std::collections::{HashMap, HashSet};
use std::io::{Read, Seek, SeekFrom};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use byteorder::{BigEndian, LittleEndian, ReadBytesExt};
use serde::Deserialize;

use crate::error::{IgvError, Result};
use crate::io::{fetch_bytes, fetch_text_maybe_gz, location_exists, Location, SeekableStream};
use crate::region::Region;
use crate::source::{FastaSource, NoodlesFastaSource, RefMeta};

/// Bidirectional chromosome name alias table (`chr1` <-> `1`, `chrM` <-> `MT`, etc.).
#[derive(Debug, Clone, Default)]
pub struct ChromAliases {
    canonical_to_aliases: HashMap<String, Vec<String>>,
    alias_to_canonical: HashMap<String, String>,
}

impl ChromAliases {
    pub fn new() -> Self {
        Self::default()
    }

    /// Populate aliases from a tab-delimited alias table (lines: `canonical\talias1\talias2...`).
    pub fn parse_tsv(&mut self, text: &str) {
        for line in text.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }
            let cols: Vec<&str> = trimmed.split('\t').map(str::trim).filter(|s| !s.is_empty()).collect();
            if cols.len() < 2 {
                continue;
            }
            let canonical = cols[0].to_string();
            for alias in &cols[1..] {
                self.add_pair(&canonical, alias);
            }
        }
    }

    pub fn add_pair(&mut self, canonical: &str, alias: &str) {
        self.alias_to_canonical
            .insert(alias.to_string(), canonical.to_string());
        self.canonical_to_aliases
            .entry(canonical.to_string())
            .or_default()
            .push(alias.to_string());
    }

    /// Generate ordered candidate chromosome names for `name`, including
    /// explicit aliases and standard UCSC/Ensembl/NCBI rules.
    pub fn candidates(&self, name: &str) -> Vec<String> {
        let mut out = Vec::new();
        let mut seen = HashSet::new();
        let mut push = |s: String| {
            if seen.insert(s.clone()) {
                out.push(s);
            }
        };

        push(name.to_string());
        if let Some(canon) = self.alias_to_canonical.get(name) {
            push(canon.clone());
            if let Some(list) = self.canonical_to_aliases.get(canon) {
                for a in list {
                    push(a.clone());
                }
            }
        }
        if let Some(list) = self.canonical_to_aliases.get(name) {
            for a in list {
                push(a.clone());
            }
        }

        // Standard UCSC <-> Ensembl / GRCh rules
        match name {
            "chrM" => {
                push("MT".into());
                push("M".into());
            }
            "MT" => {
                push("chrM".into());
                push("M".into());
            }
            "M" => {
                push("chrM".into());
                push("MT".into());
            }
            _ => {
                if let Some(stripped) = name.strip_prefix("chr") {
                    push(stripped.to_string());
                } else {
                    push(format!("chr{name}"));
                }
            }
        }
        out
    }

    /// Resolve `requested` against a list of chromosome names present in a file.
    pub fn resolve<'a>(&self, requested: &str, available: &'a [String]) -> Option<&'a str> {
        for cand in self.candidates(requested) {
            if let Some(found) = available.iter().find(|a| a.as_str() == cand) {
                return Some(found.as_str());
            }
        }
        None
    }
}

/// Global resolution helper (uses standard UCSC/Ensembl rules when no custom alias table exists).
pub fn resolve_chrom_name<'a>(requested: &str, available: &'a [String]) -> Option<&'a str> {
    ChromAliases::default().resolve(requested, available)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CytoStain {
    Gneg,
    Gpos25,
    Gpos50,
    Gpos75,
    Gpos100,
    Acen,
    Gvar,
    Stalk,
}

impl CytoStain {
    pub fn parse(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "gneg" => Self::Gneg,
            "gpos25" => Self::Gpos25,
            "gpos50" => Self::Gpos50,
            "gpos75" => Self::Gpos75,
            "gpos100" | "gpos" => Self::Gpos100,
            "acen" => Self::Acen,
            "gvar" => Self::Gvar,
            "stalk" => Self::Stalk,
            _ => Self::Gneg,
        }
    }
}

/// 1-based inclusive cytoband interval.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cytoband {
    pub chrom: String,
    pub start: u64,
    pub end: u64,
    pub name: String,
    pub stain: CytoStain,
}

/// Parse UCSC `cytoBand.txt` / `cytoBandIdeo.txt` (0-based half-open on disk).
pub fn parse_cytobands(text: &str) -> Vec<Cytoband> {
    let mut out = Vec::new();
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with("track ") {
            continue;
        }
        let cols: Vec<&str> = trimmed.split('\t').collect();
        if cols.len() < 4 {
            continue;
        }
        let Ok(s0) = cols[1].parse::<u64>() else {
            continue;
        };
        let Ok(e0) = cols[2].parse::<u64>() else {
            continue;
        };
        if e0 <= s0 {
            continue;
        }
        let stain = cols.get(4).map(|s| CytoStain::parse(s)).unwrap_or(CytoStain::Gneg);
        out.push(Cytoband {
            chrom: cols[0].to_string(),
            start: s0 + 1,
            end: e0,
            name: cols[3].to_string(),
            stain,
        });
    }
    out
}

/// Parse `.chrom.sizes` into `RefMeta` entries.
pub fn parse_chrom_sizes(text: &str) -> Vec<RefMeta> {
    let mut out = Vec::new();
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let mut parts = trimmed.split_whitespace();
        if let (Some(name), Some(len_s)) = (parts.next(), parts.next()) {
            if let Ok(length) = len_s.parse::<u64>() {
                out.push(RefMeta {
                    name: name.to_string(),
                    length,
                });
            }
        }
    }
    out
}

#[derive(Debug, Clone, Copy)]
enum TwoBitEndian {
    Little,
    Big,
}

impl TwoBitEndian {
    fn read_u32(self, r: &mut dyn Read) -> std::io::Result<u32> {
        match self {
            Self::Little => r.read_u32::<LittleEndian>(),
            Self::Big => r.read_u32::<BigEndian>(),
        }
    }

    fn read_u64(self, r: &mut dyn Read) -> std::io::Result<u64> {
        match self {
            Self::Little => r.read_u64::<LittleEndian>(),
            Self::Big => r.read_u64::<BigEndian>(),
        }
    }
}

#[derive(Debug, Clone)]
struct TwoBitSeqHeader {
    dna_size: u64,
    n_blocks: Vec<(u64, u64)>,
    mask_blocks: Vec<(u64, u64)>,
    packed_dna_offset: u64,
}

#[derive(Debug, Clone)]
struct TwoBitEntry {
    offset: u64,
    header: Option<TwoBitSeqHeader>,
}

/// UCSC `.2bit` random-access reference sequence source (local or remote HTTP/S3/GS).
pub struct TwoBitFastaSource {
    stream: Arc<Mutex<SeekableStream>>,
    endian: TwoBitEndian,
    entries: Arc<Mutex<HashMap<String, TwoBitEntry>>>,
    refs: Vec<RefMeta>,
    aliases: ChromAliases,
}

impl std::fmt::Debug for TwoBitFastaSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TwoBitFastaSource")
            .field("refs", &self.refs.len())
            .finish_non_exhaustive()
    }
}

impl TwoBitFastaSource {
    pub async fn open(
        loc: &Location,
        auth_token: Option<&str>,
        known_lengths: Option<&HashMap<String, u64>>,
    ) -> Result<Self> {
        let mut stream = SeekableStream::open(loc, auth_token).await?;
        let is_remote = loc.is_remote();
        let known_lengths_owned = known_lengths.cloned().unwrap_or_default();

        type TwoBitLoadResult = (
            SeekableStream,
            TwoBitEndian,
            HashMap<String, TwoBitEntry>,
            Vec<RefMeta>,
        );
        let (stream, endian, entries_map, refs) = tokio::task::spawn_blocking(
            move || -> Result<TwoBitLoadResult> {
                stream
                    .seek(SeekFrom::Start(0))
                    .map_err(|e| IgvError::Other(format!("2bit seek: {e}")))?;
                let magic_le = stream
                    .read_u32::<LittleEndian>()
                    .map_err(|e| IgvError::Other(format!("2bit magic: {e}")))?;
                let endian = match magic_le {
                    0x1A412743 => TwoBitEndian::Little,
                    0x4327411A => TwoBitEndian::Big,
                    other => {
                        return Err(IgvError::Other(format!(
                            "invalid .2bit signature: 0x{other:08x}"
                        )));
                    }
                };
                let version = endian
                    .read_u32(&mut stream)
                    .map_err(|e| IgvError::Other(format!("2bit version: {e}")))?;
                if version != 0 && version != 1 {
                    return Err(IgvError::Other(format!("unsupported .2bit version {version}")));
                }
                let seq_count = endian
                    .read_u32(&mut stream)
                    .map_err(|e| IgvError::Other(format!("2bit count: {e}")))?;
                let _reserved = endian
                    .read_u32(&mut stream)
                    .map_err(|e| IgvError::Other(format!("2bit reserved: {e}")))?;

                let mut raw_entries = Vec::with_capacity(seq_count as usize);
                for _ in 0..seq_count {
                    let name_len = stream
                        .read_u8()
                        .map_err(|e| IgvError::Other(format!("2bit name len: {e}")))?
                        as usize;
                    let mut name_buf = vec![0u8; name_len];
                    stream
                        .read_exact(&mut name_buf)
                        .map_err(|e| IgvError::Other(format!("2bit name: {e}")))?;
                    let name = String::from_utf8_lossy(&name_buf).into_owned();
                    let offset = if version == 1 {
                        endian
                            .read_u64(&mut stream)
                            .map_err(|e| IgvError::Other(format!("2bit offset64: {e}")))?
                    } else {
                        u64::from(
                            endian
                                .read_u32(&mut stream)
                                .map_err(|e| IgvError::Other(format!("2bit offset32: {e}")))?,
                        )
                    };
                    raw_entries.push((name, offset));
                }

                let mut entries_map = HashMap::with_capacity(raw_entries.len());
                let mut refs = Vec::with_capacity(raw_entries.len());
                // For local files (or small .2bit files), eagerly read dnaSize headers so
                // chromosome lengths are known up-front. For large remote .2bit files,
                // keep headers lazy and seed lengths from cytobands/chrom.sizes if given.
                let eager_headers = !is_remote || raw_entries.len() <= 32;
                for (name, offset) in raw_entries {
                    let (header, len) = if eager_headers {
                        let h = read_2bit_seq_header(&mut stream, endian, offset)?;
                        let l = h.dna_size;
                        (Some(h), l)
                    } else {
                        let l = known_lengths_owned.get(&name).copied().unwrap_or(0);
                        (None, l)
                    };
                    refs.push(RefMeta {
                        name: name.clone(),
                        length: len,
                    });
                    entries_map.insert(
                        name,
                        TwoBitEntry {
                            offset,
                            header,
                        },
                    );
                }

                Ok((stream, endian, entries_map, refs))
            },
        )
        .await
        .map_err(|e| IgvError::Other(e.to_string()))??;

        Ok(Self {
            stream: Arc::new(Mutex::new(stream)),
            endian,
            entries: Arc::new(Mutex::new(entries_map)),
            refs,
            aliases: ChromAliases::default(),
        })
    }

    pub fn with_aliases(mut self, aliases: ChromAliases) -> Self {
        self.aliases = aliases;
        self
    }
}

fn read_2bit_seq_header(
    stream: &mut SeekableStream,
    endian: TwoBitEndian,
    offset: u64,
) -> Result<TwoBitSeqHeader> {
    stream
        .seek(SeekFrom::Start(offset))
        .map_err(|e| IgvError::Other(format!("2bit seq seek: {e}")))?;
    let dna_size = u64::from(
        endian
            .read_u32(stream)
            .map_err(|e| IgvError::Other(format!("2bit dna_size: {e}")))?,
    );
    let n_count = endian
        .read_u32(stream)
        .map_err(|e| IgvError::Other(format!("2bit n_count: {e}")))? as usize;
    let mut n_starts = Vec::with_capacity(n_count);
    for _ in 0..n_count {
        n_starts.push(u64::from(
            endian
                .read_u32(stream)
                .map_err(|e| IgvError::Other(format!("2bit n_start: {e}")))?,
        ));
    }
    let mut n_blocks = Vec::with_capacity(n_count);
    for s in n_starts {
        let sz = u64::from(
            endian
                .read_u32(stream)
                .map_err(|e| IgvError::Other(format!("2bit n_size: {e}")))?,
        );
        n_blocks.push((s, s + sz));
    }

    let mask_count = endian
        .read_u32(stream)
        .map_err(|e| IgvError::Other(format!("2bit mask_count: {e}")))? as usize;
    let mut mask_starts = Vec::with_capacity(mask_count);
    for _ in 0..mask_count {
        mask_starts.push(u64::from(
            endian
                .read_u32(stream)
                .map_err(|e| IgvError::Other(format!("2bit mask_start: {e}")))?,
        ));
    }
    let mut mask_blocks = Vec::with_capacity(mask_count);
    for s in mask_starts {
        let sz = u64::from(
            endian
                .read_u32(stream)
                .map_err(|e| IgvError::Other(format!("2bit mask_size: {e}")))?,
        );
        mask_blocks.push((s, s + sz));
    }
    let _reserved = endian
        .read_u32(stream)
        .map_err(|e| IgvError::Other(format!("2bit seq reserved: {e}")))?;
    let packed_dna_offset = stream
        .stream_position()
        .map_err(|e| IgvError::Other(format!("2bit pos: {e}")))?;

    Ok(TwoBitSeqHeader {
        dna_size,
        n_blocks,
        mask_blocks,
        packed_dna_offset,
    })
}

#[async_trait]
impl FastaSource for TwoBitFastaSource {
    async fn references(&self) -> Result<Vec<RefMeta>> {
        Ok(self.refs.clone())
    }

    async fn fetch(&self, region: &Region) -> Result<Vec<u8>> {
        let available: Vec<String> = self.refs.iter().map(|r| r.name.clone()).collect();
        let resolved = self
            .aliases
            .resolve(&region.chrom, &available)
            .ok_or_else(|| IgvError::UnknownChromosome(region.chrom.clone()))?
            .to_string();

        let start_0 = region.start.saturating_sub(1);
        let end_0_excl = region.end;
        let stream = Arc::clone(&self.stream);
        let entries = Arc::clone(&self.entries);
        let endian = self.endian;

        tokio::task::spawn_blocking(move || -> Result<Vec<u8>> {
            let mut stream_guard = stream
                .lock()
                .map_err(|_| IgvError::Other("2bit stream poisoned".into()))?;
            let seq_header = {
                let mut map_guard = entries
                    .lock()
                    .map_err(|_| IgvError::Other("2bit entries poisoned".into()))?;
                let entry = map_guard
                    .get_mut(&resolved)
                    .ok_or_else(|| IgvError::UnknownChromosome(resolved.clone()))?;
                if entry.header.is_none() {
                    entry.header = Some(read_2bit_seq_header(
                        &mut stream_guard,
                        endian,
                        entry.offset,
                    )?);
                }
                entry.header.clone().unwrap()
            };

            let s0 = start_0.min(seq_header.dna_size);
            let e0 = end_0_excl.min(seq_header.dna_size);
            if e0 <= s0 {
                return Ok(Vec::new());
            }

            let first_byte = s0 / 4;
            let last_byte = (e0 - 1) / 4;
            let byte_len = (last_byte - first_byte + 1) as usize;
            let mut packed = vec![0u8; byte_len];

            stream_guard
                .seek(SeekFrom::Start(seq_header.packed_dna_offset + first_byte))
                .map_err(|e| IgvError::Other(format!("2bit dna seek: {e}")))?;
            stream_guard
                .read_exact(&mut packed)
                .map_err(|e| IgvError::Other(format!("2bit dna read: {e}")))?;

            let mut out = Vec::with_capacity((e0 - s0) as usize);
            for pos in s0..e0 {
                let b_idx = (pos / 4 - first_byte) as usize;
                let shift = 6 - 2 * (pos % 4) as u8;
                let code = (packed[b_idx] >> shift) & 0x03;
                // 2bit specification: 00 = T, 01 = C, 10 = A, 11 = G
                let mut base = match code {
                    0 => b'T',
                    1 => b'C',
                    2 => b'A',
                    3 => b'G',
                    _ => unreachable!(),
                };
                if in_blocks(&seq_header.n_blocks, pos) {
                    base = b'N';
                } else if in_blocks(&seq_header.mask_blocks, pos) {
                    base = base.to_ascii_lowercase();
                }
                out.push(base);
            }
            Ok(out)
        })
        .await
        .map_err(|e| IgvError::Other(e.to_string()))?
    }
}

fn in_blocks(blocks: &[(u64, u64)], pos: u64) -> bool {
    // Blocks are sorted by start; binary search for the last block with start <= pos.
    let idx = blocks.partition_point(|&(s, _)| s <= pos);
    if idx == 0 {
        return false;
    }
    let (s, e) = blocks[idx - 1];
    pos >= s && pos < e
}

#[derive(Debug, Clone)]
struct RemoteFaiEntry {
    length: u64,
    offset: u64,
    line_bases: u64,
    line_bytes: u64,
}

/// Remote HTTP/S3/GS FASTA + `.fai` reader backed by `SeekableStream`.
pub struct RemoteFastaSource {
    stream: Arc<Mutex<SeekableStream>>,
    entries: HashMap<String, RemoteFaiEntry>,
    refs: Vec<RefMeta>,
    aliases: ChromAliases,
}

impl std::fmt::Debug for RemoteFastaSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RemoteFastaSource")
            .field("refs", &self.refs.len())
            .finish_non_exhaustive()
    }
}

impl RemoteFastaSource {
    pub async fn open(
        fasta_loc: &Location,
        fai_loc: Option<&Location>,
        auth_token: Option<&str>,
    ) -> Result<Self> {
        let default_fai = fasta_loc.sibling("fai");
        let fai_loc = fai_loc.unwrap_or(&default_fai);
        let fai_text = fetch_text_maybe_gz(fai_loc, auth_token).await?;
        let mut entries = HashMap::new();
        let mut refs = Vec::new();
        for line in fai_text.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            let cols: Vec<&str> = trimmed.split('\t').collect();
            if cols.len() < 5 {
                continue;
            }
            let name = cols[0].to_string();
            let length: u64 = cols[1]
                .parse()
                .map_err(|_| IgvError::Other(format!("invalid FAI length in {trimmed}")))?;
            let offset: u64 = cols[2]
                .parse()
                .map_err(|_| IgvError::Other(format!("invalid FAI offset in {trimmed}")))?;
            let line_bases: u64 = cols[3]
                .parse()
                .map_err(|_| IgvError::Other(format!("invalid FAI line_bases in {trimmed}")))?;
            let line_bytes: u64 = cols[4]
                .parse()
                .map_err(|_| IgvError::Other(format!("invalid FAI line_bytes in {trimmed}")))?;
            refs.push(RefMeta {
                name: name.clone(),
                length,
            });
            entries.insert(
                name,
                RemoteFaiEntry {
                    length,
                    offset,
                    line_bases: line_bases.max(1),
                    line_bytes: line_bytes.max(1),
                },
            );
        }
        let stream = SeekableStream::open(fasta_loc, auth_token).await?;
        Ok(Self {
            stream: Arc::new(Mutex::new(stream)),
            entries,
            refs,
            aliases: ChromAliases::default(),
        })
    }

    pub fn with_aliases(mut self, aliases: ChromAliases) -> Self {
        self.aliases = aliases;
        self
    }
}

#[async_trait]
impl FastaSource for RemoteFastaSource {
    async fn references(&self) -> Result<Vec<RefMeta>> {
        Ok(self.refs.clone())
    }

    async fn fetch(&self, region: &Region) -> Result<Vec<u8>> {
        let available: Vec<String> = self.refs.iter().map(|r| r.name.clone()).collect();
        let resolved = self
            .aliases
            .resolve(&region.chrom, &available)
            .ok_or_else(|| IgvError::UnknownChromosome(region.chrom.clone()))?;
        let entry = self
            .entries
            .get(resolved)
            .ok_or_else(|| IgvError::UnknownChromosome(region.chrom.clone()))?
            .clone();

        let s0 = region.start.saturating_sub(1).min(entry.length);
        let e0 = region.end.min(entry.length);
        if e0 <= s0 {
            return Ok(Vec::new());
        }
        let start_byte = entry.offset
            + (s0 / entry.line_bases) * entry.line_bytes
            + (s0 % entry.line_bases);
        let end_idx = e0 - 1;
        let end_byte = entry.offset
            + (end_idx / entry.line_bases) * entry.line_bytes
            + (end_idx % entry.line_bases);
        let to_read = (end_byte - start_byte + 1) as usize;
        let stream = Arc::clone(&self.stream);

        tokio::task::spawn_blocking(move || -> Result<Vec<u8>> {
            let mut guard = stream
                .lock()
                .map_err(|_| IgvError::Other("remote fasta poisoned".into()))?;
            guard
                .seek(SeekFrom::Start(start_byte))
                .map_err(|e| IgvError::Other(format!("remote fasta seek: {e}")))?;
            let mut raw = vec![0u8; to_read];
            guard
                .read_exact(&mut raw)
                .map_err(|e| IgvError::Other(format!("remote fasta read: {e}")))?;
            let mut seq = Vec::with_capacity((e0 - s0) as usize);
            for b in raw {
                if !b.is_ascii_whitespace() {
                    seq.push(b);
                }
            }
            Ok(seq)
        })
        .await
        .map_err(|e| IgvError::Other(e.to_string()))?
    }
}

/// JSON genome descriptor (compatible with IGV genome JSON format).
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GenomeDescriptor {
    pub id: Option<String>,
    pub name: Option<String>,
    #[serde(alias = "fastaURL")]
    pub fasta_url: Option<String>,
    #[serde(alias = "twoBitURL")]
    pub two_bit_url: Option<String>,
    #[serde(alias = "indexURL")]
    pub index_url: Option<String>,
    #[serde(alias = "cytobandURL")]
    pub cytoband_url: Option<String>,
    #[serde(alias = "aliasURL")]
    pub alias_url: Option<String>,
    #[serde(alias = "chromSizesURL")]
    pub chrom_sizes_url: Option<String>,
}

impl GenomeDescriptor {
    /// Look up a built-in hosted reference genome preset by IGV/UCSC ID.
    pub fn builtin(id: &str) -> Option<Self> {
        let norm = match id.trim().to_ascii_lowercase().as_str() {
            "hg38" | "grch38" => "hg38",
            "hg19" | "grch37" | "b37" => "hg19",
            "mm39" | "grcm39" => "mm39",
            "mm10" | "grcm38" => "mm10",
            "danrer11" | "grcz11" => "danRer11",
            "dm6" => "dm6",
            "ce11" => "ce11",
            "saccer3" => "sacCer3",
            _ => return None,
        };
        Some(Self {
            id: Some(norm.to_string()),
            name: Some(norm.to_string()),
            fasta_url: None,
            two_bit_url: Some(format!(
                "https://hgdownload.soe.ucsc.edu/goldenPath/{norm}/bigZips/{norm}.2bit"
            )),
            index_url: None,
            cytoband_url: Some(format!(
                "https://hgdownload.soe.ucsc.edu/goldenPath/{norm}/database/cytoBandIdeo.txt.gz"
            )),
            alias_url: Some(format!(
                "https://hgdownload.soe.ucsc.edu/goldenPath/{norm}/bigZips/{norm}.chromAlias.txt"
            )),
            chrom_sizes_url: Some(format!(
                "https://hgdownload.soe.ucsc.edu/goldenPath/{norm}/bigZips/{norm}.chrom.sizes"
            )),
        })
    }

    pub fn builtin_ids() -> &'static [&'static str] {
        &["hg38", "hg19", "mm39", "mm10", "danRer11", "dm6", "ce11", "sacCer3"]
    }
}

/// Result of loading a genome via `open_genome`.
pub struct LoadedGenome {
    pub fasta: Arc<dyn FastaSource>,
    pub references: Vec<RefMeta>,
    pub cytobands: Vec<Cytoband>,
    pub aliases: ChromAliases,
    pub source_spec: String,
    pub genome_id: String,
}

impl std::fmt::Debug for LoadedGenome {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LoadedGenome")
            .field("genome_id", &self.genome_id)
            .field("refs", &self.refs_len())
            .field("cytobands", &self.cytobands.len())
            .finish_non_exhaustive()
    }
}

impl LoadedGenome {
    fn refs_len(&self) -> usize {
        self.references.len()
    }
}

/// Open a genome from a built-in ID (`hg38`, `mm39`, ...), a `.json` genome descriptor,
/// a `.2bit` file/URL, or a `.fa`/`.fasta` file/URL.
pub async fn open_genome(spec: &str, auth_token: Option<&str>) -> Result<LoadedGenome> {
    if let Some(desc) = GenomeDescriptor::builtin(spec) {
        return load_from_descriptor(&desc, None, spec, auth_token).await;
    }

    let loc = Location::parse(spec);
    let lower = loc.lowercase_name();
    if lower.ends_with(".json") {
        let text = fetch_text_maybe_gz(&loc, auth_token).await?;
        let desc: GenomeDescriptor = serde_json::from_str(&text)
            .map_err(|e| IgvError::Other(format!("genome json {spec}: {e}")))?;
        return load_from_descriptor(&desc, Some(&loc), spec, auth_token).await;
    }

    if lower.ends_with(".2bit") {
        let src = TwoBitFastaSource::open(&loc, auth_token, None).await?;
        let references = src.references().await?;
        return Ok(LoadedGenome {
            fasta: Arc::new(src),
            references,
            cytobands: Vec::new(),
            aliases: ChromAliases::default(),
            source_spec: spec.to_string(),
            genome_id: loc.display_name(),
        });
    }

    // FASTA local or remote
    match &loc {
        Location::Local(p) => {
            let src = NoodlesFastaSource::open(p).await?;
            let references = src.references().await?;
            Ok(LoadedGenome {
                fasta: Arc::new(src),
                references,
                cytobands: Vec::new(),
                aliases: ChromAliases::default(),
                source_spec: spec.to_string(),
                genome_id: loc.display_name(),
            })
        }
        Location::Remote(_) => {
            let src = RemoteFastaSource::open(&loc, None, auth_token).await?;
            let references = src.references().await?;
            Ok(LoadedGenome {
                fasta: Arc::new(src),
                references,
                cytobands: Vec::new(),
                aliases: ChromAliases::default(),
                source_spec: spec.to_string(),
                genome_id: loc.display_name(),
            })
        }
    }
}

async fn load_from_descriptor(
    desc: &GenomeDescriptor,
    base_loc: Option<&Location>,
    raw_spec: &str,
    auth_token: Option<&str>,
) -> Result<LoadedGenome> {
    let resolve = |s: &str| -> Location {
        match base_loc {
            Some(b) => b.resolve_relative(s),
            None => Location::parse(s),
        }
    };

    let mut aliases = ChromAliases::default();
    if let Some(alias_url) = &desc.alias_url {
        let al = resolve(alias_url);
        if location_exists(&al, auth_token).await {
            if let Ok(text) = fetch_text_maybe_gz(&al, auth_token).await {
                aliases.parse_tsv(&text);
            }
        }
    }

    let mut cytobands = Vec::new();
    let mut known_lengths: HashMap<String, u64> = HashMap::new();
    if let Some(cyto_url) = &desc.cytoband_url {
        let cl = resolve(cyto_url);
        if location_exists(&cl, auth_token).await {
            if let Ok(text) = fetch_text_maybe_gz(&cl, auth_token).await {
                cytobands = parse_cytobands(&text);
                for cb in &cytobands {
                    let entry = known_lengths.entry(cb.chrom.clone()).or_insert(0);
                    *entry = (*entry).max(cb.end);
                }
            }
        }
    }

    if let Some(sizes_url) = &desc.chrom_sizes_url {
        let sl = resolve(sizes_url);
        if location_exists(&sl, auth_token).await {
            if let Ok(text) = fetch_text_maybe_gz(&sl, auth_token).await {
                for r in parse_chrom_sizes(&text) {
                    known_lengths.insert(r.name, r.length);
                }
            }
        }
    }

    let (fasta, mut references): (Arc<dyn FastaSource>, Vec<RefMeta>) =
        if let Some(tb) = &desc.two_bit_url {
            let loc = resolve(tb);
            let src = TwoBitFastaSource::open(&loc, auth_token, Some(&known_lengths))
                .await?
                .with_aliases(aliases.clone());
            let refs = src.references().await?;
            (Arc::new(src), refs)
        } else if let Some(fa) = &desc.fasta_url {
            let loc = resolve(fa);
            let idx = desc.index_url.as_deref().map(resolve);
            match &loc {
                Location::Local(p) => {
                    let src = NoodlesFastaSource::open(p).await?;
                    let refs = src.references().await?;
                    (Arc::new(src), refs)
                }
                Location::Remote(_) => {
                    let src = RemoteFastaSource::open(&loc, idx.as_ref(), auth_token)
                        .await?
                        .with_aliases(aliases.clone());
                    let refs = src.references().await?;
                    (Arc::new(src), refs)
                }
            }
        } else {
            return Err(IgvError::Other(format!(
                "genome descriptor '{}' specifies neither fastaURL nor twoBitURL",
                raw_spec
            )));
        };

    // Fill any zero-length refs from known_lengths
    for r in &mut references {
        if r.length == 0 {
            if let Some(len) = known_lengths.get(&r.name) {
                r.length = *len;
            }
        }
    }

    Ok(LoadedGenome {
        fasta,
        references,
        cytobands,
        aliases,
        source_spec: raw_spec.to_string(),
        genome_id: desc
            .id
            .clone()
            .or_else(|| desc.name.clone())
            .unwrap_or_else(|| raw_spec.to_string()),
    })
}

/// Optional auto-discovery of cytobands alongside a local FASTA file (`<name>.cytoBand.txt`).
pub async fn load_bytes_or_none(loc: &Location, auth_token: Option<&str>) -> Option<Vec<u8>> {
    fetch_bytes(loc, auth_token).await.ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use byteorder::WriteBytesExt;

    fn craft_synthetic_2bit(seq: &[u8], n_blocks: &[(u32, u32)], mask_blocks: &[(u32, u32)]) -> Vec<u8> {
        let mut buf = Vec::new();
        buf.write_u32::<LittleEndian>(0x1A412743).unwrap();
        buf.write_u32::<LittleEndian>(0).unwrap(); // version
        buf.write_u32::<LittleEndian>(1).unwrap(); // 1 seq
        buf.write_u32::<LittleEndian>(0).unwrap(); // reserved

        let name = b"chr1";
        buf.write_u8(name.len() as u8).unwrap();
        buf.extend_from_slice(name);
        let header_size = 16 + 1 + name.len() + 4;
        buf.write_u32::<LittleEndian>(header_size as u32).unwrap();

        buf.write_u32::<LittleEndian>(seq.len() as u32).unwrap();
        buf.write_u32::<LittleEndian>(n_blocks.len() as u32).unwrap();
        for &(s, _) in n_blocks {
            buf.write_u32::<LittleEndian>(s).unwrap();
        }
        for &(_, sz) in n_blocks {
            buf.write_u32::<LittleEndian>(sz).unwrap();
        }
        buf.write_u32::<LittleEndian>(mask_blocks.len() as u32).unwrap();
        for &(s, _) in mask_blocks {
            buf.write_u32::<LittleEndian>(s).unwrap();
        }
        for &(_, sz) in mask_blocks {
            buf.write_u32::<LittleEndian>(sz).unwrap();
        }
        buf.write_u32::<LittleEndian>(0).unwrap();

        for chunk in seq.chunks(4) {
            let mut byte = 0u8;
            for (i, &b) in chunk.iter().enumerate() {
                let code = match b.to_ascii_uppercase() {
                    b'T' => 0,
                    b'C' => 1,
                    b'A' => 2,
                    b'G' => 3,
                    _ => 0,
                };
                byte |= code << (6 - 2 * i);
            }
            buf.push(byte);
        }
        buf
    }

    #[tokio::test]
    async fn twobit_decodes_t_c_a_g_n_and_softmask() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        let bytes = craft_synthetic_2bit(b"TCAGNNacgt", &[(4, 2)], &[(6, 4)]);
        std::fs::write(tmp.path(), &bytes).unwrap();

        let src = TwoBitFastaSource::open(&Location::Local(tmp.path().to_path_buf()), None, None)
            .await
            .unwrap();
        let refs = src.references().await.unwrap();
        assert_eq!(refs, vec![RefMeta { name: "chr1".into(), length: 10 }]);

        let seq = src.fetch(&Region::new("chr1", 1, 10).unwrap()).await.unwrap();
        assert_eq!(std::str::from_utf8(&seq).unwrap(), "TCAGNNacgt");
        // Alias lookup: "1" resolves to "chr1"
        let alias_seq = src.fetch(&Region::new("1", 1, 4).unwrap()).await.unwrap();
        assert_eq!(std::str::from_utf8(&alias_seq).unwrap(), "TCAG");
    }

    #[tokio::test]
    async fn twobit_matches_igv_fixture_if_present() {
        let fixture_2bit = std::path::Path::new("/Users/zhanxw/Downloads/igv-main/test/data/twobit/foo.2bit");
        let fixture_fa = std::path::Path::new("/Users/zhanxw/Downloads/igv-main/test/data/twobit/foo.2bit.fa");
        if fixture_2bit.exists() && fixture_fa.exists() {
            let src = TwoBitFastaSource::open(&Location::Local(fixture_2bit.to_path_buf()), None, None)
                .await
                .unwrap();
            let refs = src.references().await.unwrap();
            let len = refs[0].length;
            let got = src.fetch(&Region::new("chr1", 1, len).unwrap()).await.unwrap();
            let fa_text = std::fs::read_to_string(fixture_fa).unwrap();
            let expected: String = fa_text.lines().skip(1).collect();
            assert_eq!(std::str::from_utf8(&got).unwrap(), expected);
        }
    }

    #[test]
    fn chrom_aliases_resolve_ucsc_and_ensembl() {
        let mut a = ChromAliases::new();
        a.add_pair("chr1", "NC_000001.11");
        let avail = vec!["1".to_string(), "MT".to_string()];
        assert_eq!(a.resolve("chr1", &avail), Some("1"));
        assert_eq!(a.resolve("chrM", &avail), Some("MT"));
        assert_eq!(a.resolve("NC_000001.11", &["chr1".into()]), Some("chr1"));
    }

    #[test]
    fn parse_cytobands_converts_to_one_based() {
        let txt = "chr1\t0\t2300000\tp36.33\tgneg\nchr1\t2300000\t5300000\tp36.32\tgpos25\n";
        let bands = parse_cytobands(txt);
        assert_eq!(bands.len(), 2);
        assert_eq!(bands[0].start, 1);
        assert_eq!(bands[0].end, 2300000);
        assert_eq!(bands[0].stain, CytoStain::Gneg);
        assert_eq!(bands[1].stain, CytoStain::Gpos25);
    }
}
