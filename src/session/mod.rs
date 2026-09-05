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
    /// Running total of transcript CONTENT bytes at this turn -- what the
    /// conversation had accumulated when this window was billed (V102).
    /// Stamped here because the fit pairs an exact window against the
    /// content that produced it, and a pair needs both on one record.
    pub cum_content_bytes: u64,
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
/// One compaction, as the HARNESS recorded it -- not as itok inferred
/// it. The transcript writes this at every boundary, so where
/// auto-compact fires is DATA rather than a fraction someone picked
/// (V116).
///
/// `pre_tokens` is the harness's own count of the context that tripped
/// it, which runs slightly ABOVE what [`window`] reports for the same
/// moment (measured: 516-3,441 tokens over six compactions) because it
/// includes the turn that crossed the line and the transcript has not
/// billed that turn yet. Kept as two numbers, never reconciled: they
/// are counts of different things (V2).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Compaction {
    /// `auto` when the harness fired it, `manual` when a human asked.
    /// Only `auto` says anything about where the threshold IS (V116).
    pub trigger: String,
    /// The context size that tripped it.
    pub pre_tokens: u64,
    /// What survived into the next turn.
    pub post_tokens: u64,
    /// When, in the transcript's own stamp.
    pub ts: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Session {
    pub events: Vec<LoadEvent>,
    pub turns: Vec<Turn>,
    /// Records that could not be parsed, counted rather than dropped.
    pub skipped: usize,
    /// Total transcript content bytes seen (V45: a count, never text).
    pub content_bytes: u64,
}

/// How much this session GREW: the sum of positive window deltas, with
/// the first window counted whole (V117).
///
/// The first turn's window IS growth -- system prompt, tool schemas and
/// the opening message all entered the context, and starting the sum at
/// the second turn would report a 30k entry cost as free.
///
/// A NEGATIVE delta is a compaction or an eviction. It is DROPPED rather
/// than subtracted, because this measures what ENTERED; what left is
/// V116's business, and netting the two would let a compaction cancel out
/// the growth that caused it. `saturating_sub` is what drops it.
///
/// A zero window is EXCLUDED entirely (V93): one real turn reported 0,
/// which a naive delta rendered as a -387,741 compaction that never
/// happened -- and here it would also invent the whole window back as
/// growth on the following turn.
///
/// MEASURED against the occupancy it explains: growth/peak is 1.00 at the
/// median over 49 transcripts, and only a session that compacted exceeds
/// it. Contrast `entered`, which runs 1.45-28.5x higher.
#[must_use]
pub fn growth(turns: &[Turn]) -> u64 {
    let mut previous: Option<u64> = None;
    let mut total = 0u64;
    let windows = turns.iter().filter_map(Turn::billed_input);
    for window in windows.filter(|w| *w > 0) {
        let step = previous.map_or(window, |p| window.saturating_sub(p));
        total = total.saturating_add(step);
        previous = Some(window);
    }
    total
}

/// The LOWEST auto-compaction point in a set of boundaries (V116).
///
/// Lowest, because it is the one that fired EARLIEST: a red line drawn at
/// the smallest observed trigger warns before every point that has
/// actually been seen, and warning early is the side of this a reader can
/// act on (V108's ceiling logic).
///
/// `manual` boundaries are EXCLUDED. A human compacting at 200k says what
/// that human wanted, not where the machine would have fired, and mixing
/// the two would drag the line down to whatever the most impatient
/// session did.
///
/// A FREE function over the records rather than a method on `Session`,
/// because `Session` is a struct-literal-constructible public type: a new
/// field on it is a breaking change for every downstream literal, and a
/// red line for one badge is not worth spending a version rung on (V39).
#[must_use]
pub fn compact_point(compactions: &[Compaction]) -> Option<u64> {
    auto_compactions(compactions).map(|c| c.pre_tokens).min()
}

/// The auto compactions, which is what [`compact_point`] is drawn from
/// and what a report has to state an `n` for (V116).
pub fn auto_compactions(
    compactions: &[Compaction],
) -> impl Iterator<Item = &Compaction> {
    compactions.iter().filter(|c| c.trigger == "auto")
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

    /// Content that ENTERED the window: fresh input plus what was
    /// written into cache.
    ///
    /// NOT growth, and the difference is measured: a cache entry that
    /// outlives its TTL is re-created and re-billed as fresh writing, so
    /// this runs 1.45-28.5x above the occupancy it appears to explain
    /// (median 3.60). It measures cache TRAFFIC (V117). Kept because
    /// traffic is what the premium half of the bill is charged on.
    #[must_use]
    pub fn entered(&self) -> Option<u64> {
        match (self.fresh_input(), self.cache_written()) {
            (None, None) => None,
            (fresh, written) => {
                Some(fresh.unwrap_or(0).saturating_add(written.unwrap_or(0)))
            }
        }
    }

    /// Output tokens across the session -- the axis itok has never
    /// counted (V118). Measured at 56,423-2,642,850 per session, billed
    /// at a multiple of input, so absent is not small.
    #[must_use]
    pub fn output_tokens(&self) -> Option<u64> {
        total_of(&self.turns, |t| t.output)
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

/// Totals for one `usage` field across every turn, or `None` when NO
/// turn carried it.
///
/// V47: absent must stay distinguishable from zero. A harness that
/// stopped reporting cache fields would otherwise show `0 cache read`,
/// which reads as "the cache was never used" -- a measurement, when the
/// truth is that nothing was measured. Summing `Option`s and returning
/// `None` for "never present" is what keeps those apart.
fn total_of(turns: &[Turn], pick: fn(&Turn) -> Option<u64>) -> Option<u64> {
    let mut seen = false;
    let mut sum = 0u64;
    for t in turns {
        if let Some(v) = pick(t) {
            seen = true;
            sum = sum.saturating_add(v);
        }
    }
    seen.then_some(sum)
}

impl Session {
    /// Fresh, uncached input across the session (V47: read, not inferred).
    #[must_use]
    pub fn fresh_input(&self) -> Option<u64> {
        total_of(&self.turns, |t| t.input)
    }

    /// Tokens written INTO the prompt cache.
    #[must_use]
    pub fn cache_written(&self) -> Option<u64> {
        total_of(&self.turns, |t| t.cache_creation)
    }

    /// Tokens served FROM the prompt cache -- the re-billing of a context
    /// already sent. Typically the bulk of a long session's input.
    #[must_use]
    pub fn cache_read(&self) -> Option<u64> {
        total_of(&self.turns, |t| t.cache_read)
    }
}

impl Session {
    /// Per-turn context occupancy, in order, with ZERO-window turns
    /// dropped (V93).
    ///
    /// The exclusion is the whole point. Measured on a real session, one
    /// turn reported a window of 0; differencing it against its
    /// neighbours rendered a -387,741 "compaction" that never happened.
    /// A zero is not an occupancy of nothing, it is a turn that reported
    /// nothing (V47), and keeping it would let a missing measurement
    /// masquerade as a measured collapse.
    ///
    /// Filtering BEFORE differencing is what makes the series honest:
    /// the deltas are then between two turns that both reported.
    #[must_use]
    pub fn occupancy(&self) -> Vec<u64> {
        self.turns
            .iter()
            .filter_map(Turn::billed_input)
            .filter(|w| *w > 0)
            .collect()
    }

    /// Mean context growth over the last `n` turns, in itok PER TURN.
    ///
    /// The clock is TURNS, not wall-seconds (V91): context grows per turn
    /// and nothing happens between them, so a per-second rate would
    /// report "load dropping" through an idle hour while the window sat
    /// exactly as full as it was.
    ///
    /// `None` when the series holds fewer than `n + 1` samples -- `n`
    /// deltas need `n + 1` points. Substituting a shorter window's value
    /// would read as a flat trend when the truth is "not enough turns
    /// yet", and absent must stay distinguishable from measured (V47).
    ///
    /// Growth only: a window that SHRANK yields 0 rather than a negative.
    /// The question this answers is how fast the context is filling, and
    /// a negative rate would make `turns left` extrapolate backwards.
    #[must_use]
    pub fn rate(&self, n: usize) -> Option<u64> {
        let occ = self.occupancy();
        let last = occ.last()?;
        let first = occ.get(occ.len().checked_sub(n.checked_add(1)?)?)?;
        u64::try_from(n)
            .ok()
            .filter(|d| *d > 0)
            .map(|d| last.saturating_sub(*first).checked_div(d).unwrap_or(0))
    }
}

/// Turns whose cache WRITE exceeded their cache READ: the prefix was
/// re-written rather than served.
///
/// The rule needs no threshold. On a warm cache the read IS the whole
/// prefix while the write is only the new turn's content, so read
/// dominates; cold, there is nothing to read. Measured on a real
/// session, the two populations sit 20x apart -- warm turns peak at a
/// write/read ratio of 0.589, the cold ones start at 12.15 -- so `write
/// > read` separates them without a tuned number.
///
/// Equal values classify as NEITHER. A turn reporting zero for both is
/// not a cold cache, it is a turn that reported nothing.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ColdCache {
    pub turns: usize,
    pub written: u64,
    pub read: u64,
}

impl Session {
    /// Cold-cache turns, or `None` when no turn reported BOTH cache
    /// fields -- "cannot tell" is not "none found" (V47).
    #[must_use]
    pub fn cold_cache(&self) -> Option<ColdCache> {
        let mut seen = false;
        let mut out = ColdCache::default();
        for t in &self.turns {
            let (Some(w), Some(r)) = (t.cache_creation, t.cache_read) else {
                continue;
            };
            seen = true;
            if w > r {
                out.turns = out.turns.saturating_add(1);
                out.written = out.written.saturating_add(w);
                out.read = out.read.saturating_add(r);
            }
        }
        seen.then_some(out)
    }
}

/// A session's fitted context model (V102).
///
/// Two parameters: a FIXED overhead the transcript cannot see -- system
/// prompt plus tool schemas -- and a SCALE from `bytes/4` of transcript
/// content to actual billed tokens.
///
/// The scale is NOT a tokenizer ratio and must not be sold as one: it also
/// absorbs per-message framing and the stripped `thinking` text. That is
/// why it is derived per session and per harness rather than shipped as a
/// constant (V102), and why V48's rule still binds -- the factor is
/// REPORTED, never folded silently into the estimator ladder (V4).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Fit {
    /// Tokens present before any transcript content: the intercept.
    pub overhead: u64,
    /// Actual tokens per `bytes/4` unit, in THOUSANDTHS -- 1571 = 1.571x.
    /// Integer so the reported number is stable to read and to test.
    pub scale_milli: u64,
    /// Turns the parameters were fitted on.
    pub fitted: usize,
    /// Turns held BACK and used only to measure error.
    pub validated: usize,
    /// Worst held-out error, in tenths of a percent: the BAND (V102).
    pub band_permille: u64,
}

/// Turns needed before a fit is reported at all.
///
/// Two parameters can be solved from three points, but a BAND needs points
/// the fit never saw, so half the sample is held back. Below this the band
/// would be noise, and a band that understates is worse than none -- it is
/// what the refuse-inside-the-band rule depends on (V102/V92).
pub const MIN_FIT_TURNS: usize = 20;

/// What a fit can honestly say about whether something fits a window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    Fits,
    Over,
    /// Inside the error band: the fit declines to say (V102).
    TooClose,
}

impl Fit {
    /// Predicted total context for a given content size.
    #[must_use]
    pub fn predict(&self, content_bytes: u64) -> u64 {
        let units = content_bytes.checked_div(4).unwrap_or(0);
        let scaled = units
            .checked_mul(self.scale_milli)
            .and_then(|v| v.checked_div(1000))
            .unwrap_or(u64::MAX);
        self.overhead.saturating_add(scaled)
    }

    /// Whether a predicted total fits a window -- and `TooClose` when the
    /// answer lies INSIDE the band.
    ///
    /// A 5% band on 340k is +-17k: decisive at 20% full and at 90% full,
    /// worthless at 97%. Returning Fits or Over there would be a verdict
    /// the measurement cannot support (V102), so the third answer exists.
    #[must_use]
    pub fn verdict(&self, predicted: u64, window: u64) -> Verdict {
        let margin = predicted
            .saturating_mul(self.band_permille)
            .checked_div(1000)
            .unwrap_or(0);
        if predicted.saturating_add(margin) <= window {
            Verdict::Fits
        } else if predicted.saturating_sub(margin) > window {
            Verdict::Over
        } else {
            Verdict::TooClose
        }
    }
}

impl Session {
    /// Fit the context model, or `None` when the sample cannot support one.
    ///
    /// Fits on the first half and measures error on the UNSEEN second half.
    /// In-sample residuals understate, and an understated band is exactly
    /// what `verdict` must not be handed (V102).
    #[must_use]
    pub fn fit(&self) -> Option<Fit> {
        let s: Vec<(f64, f64)> = self.samples();
        if s.len() < MIN_FIT_TURNS {
            return None;
        }
        let half = s.len().checked_div(2)?;
        let (train, test) = (s.get(..half)?, s.get(half..)?);
        let (scale, overhead) = least_squares(train)?;
        Some(Fit {
            overhead: overhead.max(0.0) as u64,
            scale_milli: (scale * 1000.0).max(0.0) as u64,
            fitted: train.len(),
            validated: test.len(),
            band_permille: worst_error(test, scale, overhead),
        })
    }

    /// (content units, actual window) per turn that reported usage.
    ///
    /// Zero-window turns are excluded for V93's reason: one real turn
    /// reported a window of 0, and feeding it to a fit would drag the
    /// intercept toward a measurement that never happened.
    fn samples(&self) -> Vec<(f64, f64)> {
        self.turns
            .iter()
            .filter_map(|t| Some((t.cum_content_bytes, t.billed_input()?)))
            .filter(|(_, w)| *w > 0)
            .map(|(c, w)| ((c / 4) as f64, w as f64))
            .collect()
    }
}

/// Ordinary least squares: returns `(slope, intercept)`.
///
/// `None` when the inputs cannot determine a line -- a degenerate sample
/// with no variation in content gives a zero denominator, and a session
/// long enough to reach here can still be degenerate. Returning None beats
/// emitting an infinity that would render as a number (V47's rule, on
/// arithmetic).
fn least_squares(pts: &[(f64, f64)]) -> Option<(f64, f64)> {
    let n = pts.len() as f64;
    let sx: f64 = pts.iter().map(|(x, _)| x).sum();
    let sy: f64 = pts.iter().map(|(_, y)| y).sum();
    let sxx: f64 = pts.iter().map(|(x, _)| x * x).sum();
    let sxy: f64 = pts.iter().map(|(x, y)| x * y).sum();
    let denom = n * sxx - sx * sx;
    if denom == 0.0 || !denom.is_finite() {
        return None;
    }
    let slope = (n * sxy - sx * sy) / denom;
    let intercept = (sy - slope * sx) / n;
    (slope.is_finite() && intercept.is_finite()).then_some((slope, intercept))
}

/// Worst relative error over the HELD-OUT points, in tenths of a percent.
fn worst_error(pts: &[(f64, f64)], slope: f64, intercept: f64) -> u64 {
    pts.iter()
        .filter(|(_, actual)| *actual > 0.0)
        .map(|(x, actual)| {
            let err = (intercept + slope * x - actual).abs() / actual;
            (err * 1000.0) as u64
        })
        .max()
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn windows(ws: &[Option<u64>]) -> Vec<Turn> {
        ws.iter()
            .map(|w| Turn {
                input: *w,
                ..Turn::default()
            })
            .collect()
    }

    /// The first window is growth in full -- system prompt, tool schemas
    /// and the opening message all entered -- and each later turn
    /// contributes only what it added.
    #[test]
    fn growth_counts_the_first_window_whole_and_then_the_deltas() {
        let turns = windows(&[Some(30_000), Some(35_000), Some(50_000)]);
        assert_eq!(growth(&turns), 50_000);
    }

    /// V117: a compaction is not negative growth. Dropping the delta
    /// rather than subtracting it keeps the drop from cancelling the
    /// growth that caused it.
    #[test]
    fn a_compaction_drops_out_of_growth_rather_than_subtracting() {
        let turns = windows(&[Some(900_000), Some(12_000), Some(20_000)]);
        assert_eq!(growth(&turns), 908_000);
    }

    /// V93: the turn that reported window 0 must not enter a delta at
    /// all. Counted, it would invent the whole window back as growth on
    /// the turn after it.
    #[test]
    fn a_zero_window_turn_is_excluded_from_every_delta() {
        let turns = windows(&[Some(40_000), Some(0), Some(41_000)]);
        assert_eq!(growth(&turns), 41_000);
    }

    /// A turn with no usage at all is skipped the same way, and a
    /// session of nothing but those grows by nothing.
    #[test]
    fn turns_without_usage_contribute_no_growth() {
        assert_eq!(growth(&windows(&[None, None])), 0);
        assert_eq!(growth(&[]), 0);
    }

    /// Growth reconstructs the occupancy it explains: on a monotone
    /// session it equals the final window, which is what the measured
    /// growth/peak of 1.00 says on real transcripts (V117).
    #[test]
    fn growth_of_a_monotone_session_is_its_last_window() {
        let turns = windows(&[Some(10), Some(20), Some(30), Some(31)]);
        assert_eq!(growth(&turns), 31);
    }

    #[test]
    fn entered_is_fresh_input_plus_what_was_written_to_cache() {
        let session = Session {
            turns: vec![Turn {
                input: Some(100),
                cache_creation: Some(2_000),
                cache_read: Some(500_000),
                ..Turn::default()
            }],
            ..Session::default()
        };
        assert_eq!(session.entered(), Some(2_100));
        assert_eq!(session.cache_read(), Some(500_000));
    }

    /// V47 on both halves: a harness reporting neither field leaves
    /// `None`, which is not the same claim as "nothing entered".
    #[test]
    fn entered_is_absent_when_neither_field_was_reported() {
        let session = Session {
            turns: vec![Turn {
                cache_read: Some(500_000),
                ..Turn::default()
            }],
            ..Session::default()
        };
        assert_eq!(session.entered(), None);
    }

    #[test]
    fn output_is_summed_across_turns_and_absent_when_never_reported() {
        let with = Session {
            turns: vec![
                Turn {
                    output: Some(1_000),
                    ..Turn::default()
                },
                Turn {
                    output: Some(2_500),
                    ..Turn::default()
                },
            ],
            ..Session::default()
        };
        assert_eq!(with.output_tokens(), Some(3_500));
        assert_eq!(Session::default().output_tokens(), None);
    }
}
