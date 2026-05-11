//! Resolve a list of gene names into LabeledRegions, using loaded
//! AnnotationSource backends. Mirrors AppState::find_gene_region's
//! "union of matches on the same chromosome" rule.

use std::path::Path;
use std::sync::Arc;

use anyhow::{Context, Result};
use igv_core::region::Region;
use igv_core::source::AnnotationSource;
use tracing::warn;

use crate::snapshot::regions::LabeledRegion;

/// Read a one-name-per-line file. Lines starting with `#` and blank
/// lines are skipped. Returns the (case-preserved) names.
pub fn read_names(path: &Path) -> Result<Vec<String>> {
    let text = std::fs::read_to_string(path)
        .with_context(|| format!("read {}", path.display()))?;
    Ok(text
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(str::to_string)
        .collect())
}

/// Resolve each name into a `LabeledRegion` by querying every
/// annotation source. Names not found are dropped with a warning.
pub fn resolve(
    names: &[String],
    sources: &[Arc<dyn AnnotationSource>],
) -> Vec<LabeledRegion> {
    let mut out = Vec::with_capacity(names.len());
    for name in names {
        match resolve_one(name, sources) {
            Some(lr) => out.push(lr),
            None => {
                warn!("snapshot-genes: unknown gene '{}'", name);
                eprintln!("snapshot-genes: unknown gene '{}'", name);
            }
        }
    }
    out
}

fn resolve_one(query: &str, sources: &[Arc<dyn AnnotationSource>]) -> Option<LabeledRegion> {
    let mut chrom: Option<String> = None;
    let mut span: Option<(u64, u64)> = None;
    for src in sources {
        for (c, tx) in src.find_by_name(query) {
            let Some((s, e)) = tx.span() else { continue };
            match &chrom {
                None => {
                    chrom = Some(c);
                    span = Some((s, e));
                }
                Some(existing) if existing == &c => {
                    let (cs, ce) = span.unwrap();
                    span = Some((cs.min(s), ce.max(e)));
                }
                Some(_) => {}
            }
        }
    }
    let chrom = chrom?;
    let (s, e) = span?;
    Region::new(chrom, s, e)
        .ok()
        .map(|region| LabeledRegion {
            region,
            label: Some(query.to_string()),
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_names_strips_blank_and_comments() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("g.txt");
        std::fs::write(&p, "BRCA1\n# comment\n\nTP53\n").unwrap();
        let v = read_names(&p).unwrap();
        assert_eq!(v, vec!["BRCA1".to_string(), "TP53".to_string()]);
    }
}
