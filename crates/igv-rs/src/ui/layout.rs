//! Top-level layout: header, multi-pane body (overview/ruler/sequence/annotations/
//! links/variants/coverage/sashimi/signals/segs/gwas/alignments), footer.

use ratatui::layout::{Constraint, Direction, Layout, Rect};

#[derive(Debug, Clone)]
pub struct LayoutAreas {
    pub header: Rect,
    pub overview: Rect,
    pub ruler: Rect,
    pub sequence: Rect,
    pub annotations: Vec<Rect>,
    pub links: Vec<Rect>,
    pub variants: Option<Rect>,
    pub vcf_tracks: Vec<Rect>,
    pub coverage: Option<Rect>,
    pub sashimi: Vec<Rect>,
    pub signals: Vec<Rect>,
    pub segs: Vec<Rect>,
    pub gwas: Vec<Rect>,
    pub alignments: Vec<Rect>,
    pub footer: Rect,
}

#[derive(Debug, Clone)]
pub struct LayoutSpec {
    pub has_vcf: bool,
    pub vcf_heights: Vec<u16>,
    pub bam_count: usize,
    pub coverage_height: u16,
    pub show_sashimi: bool,
    pub sashimi_count: usize,
    pub sashimi_height: u16,
    pub alignments_min_per_track: u16,
    pub annotation_tracks: usize,
    pub annotation_height_per_track: u16,
    pub link_count: usize,
    pub link_height_per_track: u16,
    pub signal_count: usize,
    pub signal_height_per_track: u16,
    pub seg_count: usize,
    pub seg_height_per_track: u16,
    pub gwas_count: usize,
    pub gwas_height_per_track: u16,
    pub sequence_height: u16,
}

impl Default for LayoutSpec {
    fn default() -> Self {
        Self {
            has_vcf: false,
            vcf_heights: Vec::new(),
            bam_count: 0,
            coverage_height: 5,
            show_sashimi: false,
            sashimi_count: 0,
            sashimi_height: 5,
            alignments_min_per_track: 6,
            annotation_tracks: 0,
            annotation_height_per_track: 4,
            link_count: 0,
            link_height_per_track: 6,
            signal_count: 0,
            signal_height_per_track: 4,
            seg_count: 0,
            seg_height_per_track: 6,
            gwas_count: 0,
            gwas_height_per_track: 6,
            sequence_height: 2,
        }
    }
}

/// Split an interior body rect horizontally across `n_panes` (`1..=4`).
pub fn split_panes(body: Rect, n_panes: usize) -> Vec<Rect> {
    let n = n_panes.clamp(1, 4);
    if n == 1 {
        return vec![body];
    }
    let constraints: Vec<Constraint> = (0..n).map(|_| Constraint::Ratio(1, n as u32)).collect();
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints(constraints)
        .split(body)
        .to_vec()
}

pub fn compute(area: Rect, spec: &LayoutSpec) -> LayoutAreas {
    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // header
            Constraint::Min(1),    // body
            Constraint::Length(2), // footer
        ])
        .split(area);

    let header = outer[0];
    let body = outer[1];
    let footer = outer[2];

    let inner_areas = compute_pane_body(body, spec);
    LayoutAreas {
        header,
        footer,
        ..inner_areas
    }
}

/// Compute per-track vertical layout inside an arbitrary pane column rect.
pub fn compute_pane_body(body: Rect, spec: &LayoutSpec) -> LayoutAreas {
    let mut constraints: Vec<Constraint> = vec![
        Constraint::Length(3),                          // overview
        Constraint::Length(1),                          // ruler
        Constraint::Length(spec.sequence_height.max(1)), // sequence
    ];

    for _ in 0..spec.annotation_tracks {
        constraints.push(Constraint::Min(spec.annotation_height_per_track));
    }
    for _ in 0..spec.link_count {
        constraints.push(Constraint::Length(spec.link_height_per_track));
    }
    for &vh in &spec.vcf_heights {
        constraints.push(Constraint::Length(vh.max(3)));
    }
    if spec.vcf_heights.is_empty() && spec.has_vcf {
        constraints.push(Constraint::Length(3));
    }
    if spec.bam_count > 0 {
        constraints.push(Constraint::Length(spec.coverage_height));
    }
    if spec.show_sashimi {
        for _ in 0..spec.sashimi_count {
            constraints.push(Constraint::Length(spec.sashimi_height.max(4)));
        }
    }
    for _ in 0..spec.signal_count {
        constraints.push(Constraint::Length(spec.signal_height_per_track));
    }
    for _ in 0..spec.seg_count {
        constraints.push(Constraint::Length(spec.seg_height_per_track.max(4)));
    }
    for _ in 0..spec.gwas_count {
        constraints.push(Constraint::Length(spec.gwas_height_per_track.max(4)));
    }
    for _ in 0..spec.bam_count {
        constraints.push(Constraint::Min(spec.alignments_min_per_track));
    }

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints.as_slice())
        .split(body);

    let mut idx = 0;
    let overview = chunks[idx];
    idx += 1;
    let ruler = chunks[idx];
    idx += 1;
    let sequence = chunks[idx];
    idx += 1;

    let mut annotations = Vec::new();
    for _ in 0..spec.annotation_tracks {
        annotations.push(chunks[idx]);
        idx += 1;
    }
    let mut links = Vec::new();
    for _ in 0..spec.link_count {
        links.push(chunks[idx]);
        idx += 1;
    }
    let mut vcf_tracks = Vec::new();
    for _ in &spec.vcf_heights {
        vcf_tracks.push(chunks[idx]);
        idx += 1;
    }
    let variants = if spec.vcf_heights.is_empty() && spec.has_vcf {
        let v = chunks[idx];
        idx += 1;
        Some(v)
    } else {
        None
    };
    let coverage = if spec.bam_count > 0 {
        let c = chunks[idx];
        idx += 1;
        Some(c)
    } else {
        None
    };
    let mut sashimi = Vec::new();
    if spec.show_sashimi {
        for _ in 0..spec.sashimi_count {
            sashimi.push(chunks[idx]);
            idx += 1;
        }
    }
    let mut signals = Vec::new();
    for _ in 0..spec.signal_count {
        signals.push(chunks[idx]);
        idx += 1;
    }
    let mut segs = Vec::new();
    for _ in 0..spec.seg_count {
        segs.push(chunks[idx]);
        idx += 1;
    }
    let mut gwas = Vec::new();
    for _ in 0..spec.gwas_count {
        gwas.push(chunks[idx]);
        idx += 1;
    }
    let mut alignments = Vec::new();
    for _ in 0..spec.bam_count {
        alignments.push(chunks[idx]);
        idx += 1;
    }

    LayoutAreas {
        header: Rect::default(),
        overview,
        ruler,
        sequence,
        annotations,
        links,
        variants,
        vcf_tracks,
        coverage,
        sashimi,
        signals,
        segs,
        gwas,
        alignments,
        footer: Rect::default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::layout::Rect;

    #[test]
    fn link_tracks_get_dedicated_areas_after_annotations() {
        let area = Rect::new(0, 0, 80, 60);
        let spec = LayoutSpec {
            has_vcf: false,
            bam_count: 0,
            annotation_tracks: 1,
            link_count: 2,
            link_height_per_track: 6,
            ..Default::default()
        };
        let areas = compute(area, &spec);
        assert_eq!(areas.links.len(), 2);
        assert!(areas.links[0].y > areas.annotations[0].y);
        assert!(areas.links[0].y >= areas.annotations[0].y + areas.annotations[0].height);
        assert_eq!(areas.links[0].height, 6);
        assert_eq!(areas.links[1].height, 6);
    }
}
