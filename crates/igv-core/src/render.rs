//! Render mode selection by zoom level. Thresholds are configurable but
//! ship with sensible defaults from the design spec.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Thresholds {
    /// At or below this width, show per-base sequence and full CIGAR.
    pub per_base: u64,
    /// At or below this width, still show per-base sequence.
    pub detailed: u64,
    /// At or below this width, hide alignments but keep coverage.
    pub coverage_only: u64,
    /// At or below this width, use coverage-as-heatbar; above it, only overview.
    pub heat: u64,
}

impl Default for Thresholds {
    fn default() -> Self {
        Self {
            per_base: 200,
            detailed: 1_000,
            coverage_only: 10_000,
            heat: 100_000,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderMode {
    PerBase,        // ≤ per_base
    DetailedReads,  // ≤ detailed
    CoverageDense,  // ≤ coverage_only
    HeatBar,        // ≤ heat
    OverviewOnly,   // > heat
}

impl Thresholds {
    pub fn classify(self, view_width: u64) -> RenderMode {
        match view_width {
            w if w <= self.per_base => RenderMode::PerBase,
            w if w <= self.detailed => RenderMode::DetailedReads,
            w if w <= self.coverage_only => RenderMode::CoverageDense,
            w if w <= self.heat => RenderMode::HeatBar,
            _ => RenderMode::OverviewOnly,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_default_thresholds() {
        let t = Thresholds::default();
        assert_eq!(t.classify(50), RenderMode::PerBase);
        assert_eq!(t.classify(200), RenderMode::PerBase);
        assert_eq!(t.classify(201), RenderMode::DetailedReads);
        assert_eq!(t.classify(1_000), RenderMode::DetailedReads);
        assert_eq!(t.classify(1_001), RenderMode::CoverageDense);
        assert_eq!(t.classify(10_000), RenderMode::CoverageDense);
        assert_eq!(t.classify(10_001), RenderMode::HeatBar);
        assert_eq!(t.classify(100_000), RenderMode::HeatBar);
        assert_eq!(t.classify(100_001), RenderMode::OverviewOnly);
    }
}
