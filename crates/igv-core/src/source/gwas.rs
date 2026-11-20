//! GWAS Manhattan association track source (`.gwas` / `.assoc` / `.tsv` / `.gz`).

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Cursor};

use async_trait::async_trait;

use crate::error::{IgvError, Result};
use crate::genome::resolve_chrom_name;
use crate::io::{fetch_bytes, maybe_gunzip, Location};
use crate::region::Region;

#[derive(Debug, Clone, PartialEq)]
pub struct GwasPoint {
    pub pos: u64, // 1-based
    pub neg_log10_p: f32,
    pub p_raw: f64,
    pub snp_id: String,
}

#[async_trait]
pub trait GwasSource: Send + Sync {
    async fn fetch(&self, region: &Region) -> Result<Vec<GwasPoint>>;
    fn display_name(&self) -> &str;
}

#[derive(Debug)]
pub struct FileGwasSource {
    display: String,
    by_chrom: HashMap<String, Vec<GwasPoint>>,
    chroms: Vec<String>,
}

impl FileGwasSource {
    pub async fn open_location(location: &Location, auth_token: Option<&str>) -> Result<Self> {
        let display = location.display_name();
        let raw = fetch_bytes(location, auth_token).await?;
        let by_chrom = tokio::task::spawn_blocking(move || -> Result<_> {
            let plain = maybe_gunzip(raw)?;
            Ok(parse_gwas(BufReader::new(Cursor::new(plain))))
        })
        .await
        .map_err(|e| IgvError::Other(e.to_string()))??;
        let chroms = by_chrom.keys().cloned().collect();
        Ok(Self {
            display,
            by_chrom,
            chroms,
        })
    }
}

fn parse_gwas<R: BufRead>(reader: R) -> HashMap<String, Vec<GwasPoint>> {
    let mut by_chrom: HashMap<String, Vec<GwasPoint>> = HashMap::new();
    let mut chr_col = 1usize;
    let mut pos_col = 2usize;
    let mut p_col = 3usize;
    let mut snp_col = Some(0usize);
    let mut header_seen = false;

    for line_res in reader.lines() {
        let Ok(line) = line_res else { continue };
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with("##") {
            continue;
        }
        let cols: Vec<&str> = trimmed.split_whitespace().collect();
        if cols.len() < 3 {
            continue;
        }
        if !header_seen {
            header_seen = true;
            // Inspect column headers if first line is non-numeric on pos/p
            let upper: Vec<String> = cols.iter().map(|c| c.trim_start_matches('#').to_ascii_uppercase()).collect();
            if let Some(c_idx) = upper.iter().position(|c| matches!(c.as_str(), "CHR" | "CHROM" | "CHROMOSOME")) {
                chr_col = c_idx;
                pos_col = upper
                    .iter()
                    .position(|c| matches!(c.as_str(), "BP" | "POS" | "POSITION"))
                    .unwrap_or(pos_col);
                p_col = upper
                    .iter()
                    .position(|c| matches!(c.as_str(), "P" | "PVAL" | "PVALUE" | "P_VALUE"))
                    .unwrap_or(p_col);
                snp_col = upper
                    .iter()
                    .position(|c| matches!(c.as_str(), "SNP" | "ID" | "RSID" | "MARKER"));
                continue;
            }
        }

        let Some(chrom) = cols.get(chr_col).map(|s| s.to_string()) else {
            continue;
        };
        let Some(pos) = cols.get(pos_col).and_then(|s| s.parse::<u64>().ok()) else {
            continue;
        };
        let Some(p_raw) = cols.get(p_col).and_then(|s| s.parse::<f64>().ok()) else {
            continue;
        };
        if !p_raw.is_finite() || p_raw <= 0.0 || p_raw > 1.0 {
            continue;
        }
        let neg_log10_p = (-p_raw.log10()) as f32;
        let snp_id = snp_col
            .and_then(|i| cols.get(i))
            .map(|s| (*s).to_string())
            .unwrap_or_else(|| format!("{chrom}:{pos}"));
        by_chrom.entry(chrom).or_default().push(GwasPoint {
            pos,
            neg_log10_p,
            p_raw,
            snp_id,
        });
    }

    for v in by_chrom.values_mut() {
        v.sort_by_key(|p| p.pos);
    }
    by_chrom
}

#[async_trait]
impl GwasSource for FileGwasSource {
    async fn fetch(&self, region: &Region) -> Result<Vec<GwasPoint>> {
        let Some(chrom) = resolve_chrom_name(&region.chrom, &self.chroms) else {
            return Ok(Vec::new());
        };
        let out = self
            .by_chrom
            .get(chrom)
            .map(|v| {
                v.iter()
                    .filter(|p| p.pos >= region.start && p.pos <= region.end)
                    .cloned()
                    .collect()
            })
            .unwrap_or_default();
        Ok(out)
    }

    fn display_name(&self) -> &str {
        &self.display
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_gwas_and_ignores_bad_p_values() {
        let data = "\
SNP CHR BP P
rs1 6 100 1e-8
rs2 6 200 0
rs3 6 300 -0.5
rs4 6 400 NA
";
        let map = parse_gwas(BufReader::new(Cursor::new(data)));
        assert_eq!(map["6"].len(), 1);
        assert_eq!(map["6"][0].snp_id, "rs1");
        assert!((map["6"][0].neg_log10_p - 8.0).abs() < 1e-4);
    }
}
