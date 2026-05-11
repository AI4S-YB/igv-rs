//! View event types pushed from TUI to browser, and the initial config
//! snapshot used to build `/api/config`.

use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ViewEvent {
    pub chrom: String,
    pub start: u64,
    pub end: u64,
}

#[derive(Debug, Clone)]
pub struct ViewSnapshot {
    pub initial: ViewEvent,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn view_event_serializes_to_expected_keys() {
        let ev = ViewEvent {
            chrom: "chr1".into(),
            start: 1000,
            end: 2000,
        };
        let s = serde_json::to_string(&ev).unwrap();
        assert_eq!(s, r#"{"chrom":"chr1","start":1000,"end":2000}"#);
    }
}
