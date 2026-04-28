use igv_core::Region;

#[derive(Debug, Clone)]
pub enum Action {
    /// Move forward/backward. `large=false` shifts by 1/10 of the window
    /// (fine step, `h`/`l`); `large=true` shifts by a full window
    /// (page step, `a`/`d` and arrow keys).
    Move { forward: bool, large: bool },
    /// Zoom in / out.
    Zoom { zoom_in: bool },
    /// Jump to an explicit region.
    Goto(Region),
    /// Toggle dark/light theme.
    ToggleTheme,
    /// Open the command palette.
    OpenCommand,
    /// Submit the command palette buffer.
    CommandSubmit(String),
    /// Cancel command palette.
    CommandCancel,
    /// Set a bookmark to the current region under key `c`.
    SetBookmark(char),
    /// Jump to the bookmark stored at key `c`.
    JumpBookmark(char),
    /// Scroll the alignment lanes vertically. Positive = down, negative = up.
    ScrollAlignments(i16),
    /// Resize alignment-track minimum height. Positive = grow.
    ResizeAlignments(i16),
    /// Resize coverage-track height. Positive = grow.
    ResizeCoverage(i16),
    /// Toggle per-track / shared auto-scale across all signal tracks.
    ToggleSignalSharedScale,
    /// Resize signal-track height. Positive = grow.
    ResizeSignal(i16),
    /// Toggle the keybinding help overlay.
    ToggleHelp,
    /// Close the keybinding help overlay (any-key dismiss).
    CloseHelp,
    /// Quit the application.
    Quit,
    /// No-op (used as a sentinel).
    None,
}
