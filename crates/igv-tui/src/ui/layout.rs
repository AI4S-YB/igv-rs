//! Top-level layout: header, body (overview/ruler/sequence/variants/coverage/
//! alignments), footer.

use ratatui::layout::{Constraint, Direction, Layout, Rect};

#[derive(Debug)]
pub struct LayoutAreas {
    pub header: Rect,
    pub overview: Rect,
    pub ruler: Rect,
    pub sequence: Rect,
    pub variants: Option<Rect>,
    pub coverage: Option<Rect>,
    pub alignments: Vec<Rect>,
    pub footer: Rect,
}

pub struct LayoutSpec {
    pub has_vcf: bool,
    pub bam_count: usize,
    pub coverage_height: u16,
    pub alignments_min_per_track: u16,
}

impl Default for LayoutSpec {
    fn default() -> Self {
        Self {
            has_vcf: false,
            bam_count: 0,
            coverage_height: 5,
            alignments_min_per_track: 6,
        }
    }
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

    let mut constraints: Vec<Constraint> = vec![
        Constraint::Length(3), // overview
        Constraint::Length(1), // ruler
        Constraint::Length(2), // sequence
    ];

    if spec.has_vcf {
        constraints.push(Constraint::Length(3));
    }
    if spec.bam_count > 0 {
        constraints.push(Constraint::Length(spec.coverage_height));
        for _ in 0..spec.bam_count {
            constraints.push(Constraint::Min(spec.alignments_min_per_track));
        }
    }

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints.as_slice())
        .split(body);

    let mut idx = 0;
    let overview = chunks[idx]; idx += 1;
    let ruler = chunks[idx]; idx += 1;
    let sequence = chunks[idx]; idx += 1;
    let variants = if spec.has_vcf {
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
    let mut alignments = Vec::new();
    for _ in 0..spec.bam_count {
        alignments.push(chunks[idx]);
        idx += 1;
    }

    LayoutAreas {
        header,
        overview,
        ruler,
        sequence,
        variants,
        coverage,
        alignments,
        footer,
    }
}
