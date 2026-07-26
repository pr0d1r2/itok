//! The runtime axis's reader (V41): what a session actually LOADED, from
//! the harness's own on-disk transcript.
//!
//! Read-only, always (V43). itok never writes, moves or mutates a
//! transcript -- it is the user's data and the ground truth at once.
//!
//! The schema is FOREIGN and unversioned, so every field is optional as
//! far as this module is concerned: unknown fields are ignored, a record
//! that will not parse is SKIPPED AND COUNTED, and the last line may be
//! half-written because the file appends while a session runs (V43).
//! Nothing here returns an error for malformed input -- a torn tail is
//! normal, not exceptional.
//!
//! Content NEVER enters these types (V45). A `LoadEvent` carries a path,
//! a size and a tool name; it cannot carry a file body or message text,
//! because the struct has nowhere to put one. That is deliberate: the
//! type system is the enforcement, not a review convention.
//!
//! Harness-pluggable: this module owns the vocabulary, and one submodule
//! per harness owns the shape. A new harness is a new reader, not a new
//! tool (V43).

pub mod claude_code;

/// What a load event came from. Attachments are a load class in their own
/// right (V78): hook output and injected context cost input tokens just
/// as a tool result does, and counting only tool results would inflate
/// `unaccounted` while the data sat in the transcript.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Source {
    /// A tool returned something into the context.
    Tool(String),
    /// Hook output, a reminder, injected context.
    Attachment(String),
}

impl Source {
    /// The name to show in a report.
    #[must_use]
    pub fn label(&self) -> &str {
        match self {
            Self::Tool(name) | Self::Attachment(name) => name,
        }
    }
}

/// One thing that entered the context.
///
/// `bytes` is what was BILLED -- the retained, possibly truncated content
/// (V76). A harness may spill an oversized result to a side file and keep
/// only a prefix; the spilled size is recorded separately and is NEVER
/// the load size. Measured: 5,749,032 bytes on disk against 30,000
/// retained, the same event 190x apart.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadEvent {
    pub session: String,
    pub ts: String,
    pub source: Source,
    /// The file this load concerned, when the tool named one.
    pub path: Option<String>,
    /// Billed size, in bytes of retained content (V76).
    pub bytes: usize,
    /// Bytes spilled to a side file and NOT billed. Reported so the gap
    /// is visible, never added to `bytes`.
    pub spilled: Option<usize>,
}

/// One model turn's token accounting, straight from the harness's record
/// of the API response.
///
/// These are ACTUAL counts, not estimates (V43) -- which is why the axis
/// can close the estimate-vs-truth loop for free. Every field is
/// `Option`: absent must stay distinguishable from zero, because a zero
/// reads as a measurement (V47).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Turn {
    pub session: String,
    pub ts: String,
    pub input: Option<u64>,
    pub cache_creation: Option<u64>,
    pub cache_read: Option<u64>,
    pub output: Option<u64>,
}

impl Turn {
    /// Everything billed as input for this turn: fresh + cache creation +
    /// cache read. `None` when the record carried no usage at all.
    #[must_use]
    pub fn billed_input(&self) -> Option<u64> {
        let parts = [self.input, self.cache_creation, self.cache_read];
        if parts.iter().all(Option::is_none) {
            return None;
        }
        Some(
            parts
                .iter()
                .flatten()
                .fold(0u64, |a, b| a.saturating_add(*b)),
        )
    }
}

/// A parsed session: what was loaded, what it cost, and what could not be
/// read.
///
/// `skipped` is part of the result, not a log line (V43/V44). A reader
/// that silently discarded malformed records would report a total that
/// looks complete, which is the honesty failure this axis exists to
/// avoid.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Session {
    pub events: Vec<LoadEvent>,
    pub turns: Vec<Turn>,
    /// Records that could not be parsed, counted rather than dropped.
    pub skipped: usize,
}

impl Session {
    /// Total input tokens billed across every turn. EXACT, not an
    /// estimate: `usage` is present on every assistant record (V44), so
    /// only the ATTRIBUTION of that total to individual loads is partial.
    #[must_use]
    pub fn billed_input(&self) -> u64 {
        self.turns
            .iter()
            .filter_map(Turn::billed_input)
            .fold(0u64, u64::saturating_add)
    }

    /// Retained bytes across every load event -- the ACCOUNTED share.
    #[must_use]
    pub fn accounted_bytes(&self) -> usize {
        self.events
            .iter()
            .map(|e| e.bytes)
            .fold(0usize, usize::saturating_add)
    }
}
