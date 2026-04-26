use igv_core::Region;

#[derive(Debug, Clone)]
pub enum Action {
    /// Move forward by `nav_overlap` of view width.
    MoveForward,
    /// Move backward.
    MoveBackward,
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
    /// Quit the application.
    Quit,
    /// No-op (used as a sentinel).
    None,
}
