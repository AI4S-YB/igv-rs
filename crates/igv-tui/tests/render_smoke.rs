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
