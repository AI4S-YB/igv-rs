//! Snapshot-style smoke test: render a known-state frame to TestBackend and
//! assert on a few characters of the buffer. Insta is deliberately not used
//! here so the test stays self-contained.

use ratatui::backend::TestBackend;
use ratatui::Terminal;

#[test]
fn empty_layout_does_not_panic() {
    let backend = TestBackend::new(80, 24);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal.draw(|_f| {}).unwrap();
}

// ---------------------------------------------------------------------------
// Signal-track smoke test
//
// Uses the real production SignalWidget from igv-tui's library surface so
// that changes or breakage in src/ui/widgets/signal.rs are caught here.
// ---------------------------------------------------------------------------

use igv_core::region::Region;
use igv_core::source::signal::SignalBin;
use igv_tui::ui::theme::Theme;
use igv_tui::ui::widgets::signal::SignalWidget;

/// Helper: collect all cell symbols from a TestBackend buffer into a single
/// String so assertions can use `contains`.
fn buffer_to_string(buf: &ratatui::buffer::Buffer) -> String {
    let area = buf.area();
    let mut out = String::with_capacity((area.width * area.height) as usize);
    for y in area.top()..area.bottom() {
        for x in area.left()..area.right() {
            out.push_str(buf[(x, y)].symbol());
        }
    }
    out
}

#[test]
fn signal_track_renders_bars_and_title() {
    // Build 50 bins with values 0..50 (genomic positions 1..=50).
    let bins: Vec<SignalBin> = (0u64..50)
        .map(|i| SignalBin {
            start: i + 1,
            end:   i + 1,
            value: i as f32,
        })
        .collect();

    let region = Region::new("chr1", 1, 100).unwrap();
    let theme = Theme::dark();

    // Render the widget into a 80×24 TestBackend.
    let backend = TestBackend::new(80, 24);
    let mut terminal = Terminal::new(backend).unwrap();

    terminal
        .draw(|f| {
            let area = f.area();
            f.render_widget(
                SignalWidget {
                    display_name: "mock",
                    bins: &bins,
                    region: &region,
                    theme: &theme,
                    shared_max: None,
                },
                area,
            );
        })
        .unwrap();

    let buf = terminal.backend().buffer().clone();
    let rendered = buffer_to_string(&buf);

    // Assert 1: title contains "signal[mock]"
    assert!(
        rendered.contains("signal[mock]"),
        "expected 'signal[mock]' in rendered output, got:\n{}",
        rendered
    );

    // Assert 2: title contains "[0-49" (scale max is 49.0; formatted as
    // "[0-49.0]" with the {:.1} format used by the widget)
    assert!(
        rendered.contains("[0-49"),
        "expected '[0-49' in rendered output, got:\n{}",
        rendered
    );

    // Assert 3: at least one cell has the block character '█'
    let has_block = buf
        .area()
        .rows()
        .flat_map(|row| {
            (row.left()..row.right()).map(move |x| (x, row.y))
        })
        .any(|(x, y)| buf[(x, y)].symbol() == "█");
    assert!(has_block, "expected at least one '█' bar character in the rendered frame");
}
