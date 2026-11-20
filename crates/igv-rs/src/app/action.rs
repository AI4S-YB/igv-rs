use igv_core::Region;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SnapshotFormat {
    Svg,
    Png,
}

impl SnapshotFormat {
    pub fn from_path(p: &std::path::Path) -> Self {
        match p
            .extension()
            .and_then(|e| e.to_str())
            .map(|s| s.to_ascii_lowercase())
        {
            Some(ref s) if s == "png" => Self::Png,
            _ => Self::Svg,
        }
    }
}

#[derive(Debug, Clone)]
pub enum Action {
    /// Save the current view to disk. `path = None` → auto-named in cwd.
    SaveSnapshot {
        path: Option<std::path::PathBuf>,
        format: SnapshotFormat,
    },
    /// Move forward/backward. `large=false` shifts by 1/10 of the window
    /// (fine step, `h`/`l`); `large=true` shifts by a full window
    /// (page step, `a`/`d` and arrow keys).
    Move { forward: bool, large: bool },
    /// Zoom in / out.
    Zoom { zoom_in: bool },
    /// Jump to an explicit region.
    Goto(Region),
    /// Cycle active multi-locus split pane (`Tab`).
    NextPane,
    /// Toggle dark/light theme.
    ToggleTheme,
    /// Toggle Sashimi splice-junction track visibility (`J`).
    ToggleSashimi,
    /// Toggle View-as-Pairs mode (`P`).
    TogglePairs,
    /// Cycle alignment coloring mode (`C`).
    CycleColorMode,
    /// Toggle 3-frame amino-acid translation (`T`).
    ToggleTranslation,
    /// Toggle inline terminal pixel graphics preview (`G`).
    ToggleGraphics,
    /// Open the command palette.
    OpenCommand,
    /// Submit the command palette buffer.
    CommandSubmit(String),
    /// Cancel command palette.
    CommandCancel,
    /// Dismiss modal feature inspector (`Esc`/`Enter`/`q`).
    CloseInspector,
    /// Handle left mouse click at terminal cell `(x, y)`.
    MouseClick { x: u16, y: u16 },
    /// Handle horizontal mouse drag on ruler (`start_x..=end_x` at row `y`).
    MouseDrag { start_x: u16, end_x: u16, y: u16 },
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
    /// Resize link-track height. Positive = grow.
    ResizeLink(i16),
    /// Toggle the keybinding help overlay.
    ToggleHelp,
    /// Close the keybinding help overlay (any-key dismiss).
    CloseHelp,
    /// Quit the application.
    Quit,
    /// Open a browser window showing the current view via igv-serve.
    OpenBrowser,
    /// No-op (used as a sentinel).
    None,
}
