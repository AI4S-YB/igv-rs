use std::sync::Arc;

use igv_core::region::Region;
use igv_core::source::annotation::Strand;
use igv_core::source::link::{LinkRecord, LinkScope, VisibleLink};
use igv_tui::ui::theme::Theme;
use igv_tui::ui::widgets::link::LinkWidget;
use ratatui::backend::TestBackend;
use ratatui::Terminal;

fn render(visible: &[VisibleLink], width: u16, height: u16) -> Vec<String> {
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).unwrap();
    let theme = Theme::dark();
    let region = Region::new("chr1", 1_000_000, 1_010_000).unwrap();
    terminal
        .draw(|f| {
            f.render_widget(
                LinkWidget {
                    display_name: "loops.bedpe",
                    region: &region,
                    theme: &theme,
                    visible,
                    total_record_count: visible.len(),
                    height_rows: height,
                },
                f.area(),
            );
        })
        .unwrap();
    let buf = terminal.backend().buffer().clone();
    (0..height)
        .map(|y| {
            (0..width)
                .map(|x| buf[(x, y)].symbol().to_string())
                .collect::<String>()
        })
        .collect()
}

fn cis_record(s_a: u64, e_a: u64, s_b: u64, e_b: u64, score: Option<f64>) -> VisibleLink {
    VisibleLink {
        record: LinkRecord {
            chrom_a: Arc::from("chr1"),
            start_a: s_a,
            end_a: e_a,
            chrom_b: Arc::from("chr1"),
            start_b: s_b,
            end_b: e_b,
            name: None,
            score,
            strand_a: Strand::Forward,
            strand_b: Strand::Reverse,
        },
        scope: LinkScope::BothIn,
    }
}

#[test]
fn arc_sparse_renders_anchor_strip_and_arcs() {
    let v = vec![
        cis_record(1_001_000, 1_002_000, 1_008_000, 1_009_000, Some(5.0)),
        cis_record(1_003_000, 1_004_000, 1_006_000, 1_007_000, Some(2.0)),
    ];
    let rows = render(&v, 80, 6);
    let title = &rows[rows.len() - 1];
    assert!(title.contains("loops.bedpe"), "title: {title:?}");
    assert!(title.contains("2 loops"), "title should report count: {title:?}");
    let anchor_row = &rows[rows.len() - 2];
    assert!(
        anchor_row.contains('\u{2588}'),
        "anchor row should contain █: {anchor_row:?}"
    );
    let has_arc_char = rows[..rows.len() - 2].iter().any(|row| {
        row.chars()
            .any(|c| matches!(c, '\u{256d}' | '\u{256e}' | '\u{2500}'))
    });
    assert!(has_arc_char, "expected at least one arc char above anchor strip");
}

#[test]
fn empty_visible_renders_zero_loops_title() {
    let rows = render(&[], 80, 6);
    let title = &rows[rows.len() - 1];
    assert!(title.contains("0 loops"), "title: {title:?}");
}
