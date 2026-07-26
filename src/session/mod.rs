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

/// The part of a transcript that is safe to read: everything up to and
/// including the last complete line.
///
/// The file appends while a session runs, so the tail may be a
/// half-written record (V43). Truncating here is what makes two reads a
/// second apart AGREE -- a report-only verb that contradicts itself is
/// broken (V5). The determinism comes from this truncation, not from
/// storing a snapshot anywhere: an on-disk cache was measured at 8ms of
/// benefit and rejected (V77/B8).
#[must_use]
pub fn complete_prefix(text: &str) -> &str {
    match text.rfind('\n') {
        // Include the newline: the prefix ends at a record boundary.
        Some(i) => text.get(..i.saturating_add(1)).unwrap_or(""),
        // No newline at all: nothing is complete yet.
        None => "",
    }
}

/// A content key over the complete-line prefix.
///
/// Keys the CONTENT, not the file: appending a partial record leaves the
/// key unchanged, and a whole new record changes it (V77). Two reads that
/// produce the same key describe the same session state.
///
/// FNV-1a over the prefix, paired with its length. Hand-rolled on
/// purpose: `std`'s `DefaultHasher` is explicitly not stable across Rust
/// releases, so a toolchain upgrade would silently change every key --
/// and a hashing crate is a dependency this does not need (V13). The
/// length pairing makes a collision unreachable at this scale; this is a
/// cache key, never a security primitive.
#[must_use]
pub fn content_key(text: &str) -> String {
    let prefix = complete_prefix(text);
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for b in prefix.as_bytes() {
        hash ^= u64::from(*b);
        hash = hash.wrapping_mul(0x100_0000_01b3);
    }
    format!("{}-{hash:016x}", prefix.len())
}

/// What the model actually received on the LAST turn: the size of one
/// context window, exact, from the harness's own usage record.
///
/// This -- not the cumulative bill -- is what `accounted` must be
/// compared against. The cumulative counts the same content once per
/// turn, so measuring attribution against it would report ~99% missing
/// when the truth is that the same accounted bytes were re-sent (V44).
/// `None` when no turn carried usage: absent must not read as zero (V47).
#[must_use]
pub fn window(turns: &[Turn]) -> Option<u64> {
    let last = turns.last()?;
    let parts = [last.cache_creation, last.cache_read, last.input];
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

impl Session {
    /// One window's size (V44). See [`window`].
    #[must_use]
    pub fn window(&self) -> Option<u64> {
        window(&self.turns)
    }

    /// Estimated tokens across every load event -- the ACCOUNTED share.
    /// `bytes/4`, because no content is retained to tokenize (V45).
    #[must_use]
    pub fn accounted_tokens(&self) -> u64 {
        self.events
            .iter()
            .map(|e| u64::try_from(e.bytes / 4).unwrap_or(u64::MAX))
            .fold(0u64, u64::saturating_add)
    }

    /// Window minus accounted: system prompt, tool schemas, project
    /// instructions, and the conversation itself (V44).
    ///
    /// Clamped at zero. `bytes/4` can over-estimate, and a negative gap
    /// would say something false about the data rather than about the
    /// estimator.
    #[must_use]
    pub fn unaccounted_tokens(&self) -> Option<u64> {
        Some(self.window()?.saturating_sub(self.accounted_tokens()))
    }
}
