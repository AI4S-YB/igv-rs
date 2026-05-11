//! Filename builders shared by interactive auto-naming and batch.

use std::path::{Path, PathBuf};

use igv_core::region::Region;

use crate::app::action::SnapshotFormat;

pub fn ext(format: SnapshotFormat) -> &'static str {
    match format {
        SnapshotFormat::Svg => "svg",
        SnapshotFormat::Png => "png",
    }
}

/// Default name for the `S`-key snapshot in cwd:
/// `snapshot_<chrom>_<start>_<end>.<ext>`.
pub fn auto_name(region: &Region, format: SnapshotFormat) -> PathBuf {
    PathBuf::from(format!(
        "snapshot_{}_{}_{}.{}",
        sanitize(&region.chrom),
        region.start,
        region.end,
        ext(format),
    ))
}

/// Name for batch outputs: `<label>_<chrom>_<start>_<end>.<ext>`.
/// `label = None` → `<chrom>_<start>_<end>.<ext>`.
pub fn batch_name(
    out_dir: &Path,
    label: Option<&str>,
    region: &Region,
    format: SnapshotFormat,
) -> PathBuf {
    let stem = match label {
        Some(l) if !l.trim().is_empty() => format!(
            "{}_{}_{}_{}",
            sanitize(l),
            sanitize(&region.chrom),
            region.start,
            region.end
        ),
        _ => format!(
            "{}_{}_{}",
            sanitize(&region.chrom),
            region.start,
            region.end
        ),
    };
    out_dir.join(format!("{}.{}", stem, ext(format)))
}

fn sanitize(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '.' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auto_name_default() {
        let r = Region::new("chr1", 1000, 2000).unwrap();
        let p = auto_name(&r, SnapshotFormat::Svg);
        assert_eq!(p.to_str().unwrap(), "snapshot_chr1_1000_2000.svg");
    }

    #[test]
    fn batch_name_with_label() {
        let r = Region::new("chr2", 5, 10).unwrap();
        let p = batch_name(Path::new("out"), Some("BRCA1"), &r, SnapshotFormat::Png);
        assert_eq!(p.to_str().unwrap(), "out/BRCA1_chr2_5_10.png");
    }

    #[test]
    fn batch_name_without_label() {
        let r = Region::new("chr2", 5, 10).unwrap();
        let p = batch_name(Path::new("out"), None, &r, SnapshotFormat::Svg);
        assert_eq!(p.to_str().unwrap(), "out/chr2_5_10.svg");
    }

    #[test]
    fn sanitize_strips_path_separators() {
        let r = Region::new("chr1", 1, 2).unwrap();
        let p = batch_name(Path::new("out"), Some("a/b\\c"), &r, SnapshotFormat::Svg);
        assert_eq!(p.to_str().unwrap(), "out/a_b_c_chr1_1_2.svg");
    }
}
