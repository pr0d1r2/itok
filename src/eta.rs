//! `~to compaction`: how long the context has before it hits the red line
//! (V119). An extrapolation on V116's extrapolated threshold and V117's
//! rate, which is why most of this module is refusals.
//!
//! The clock is ACTIVE time (V111). The answer is in active seconds and
//! the json key says so, because read as wall clock it would sell a lunch
//! break as headroom.

/// Seconds per hour, which is the unit `growth_per_hour` arrives in.
const HOUR: u64 = 3600;

/// The floored unit ladder: largest unit that yields at least 1.
const LADDER: [(u64, char); 3] = [(86_400, 'd'), (HOUR, 'h'), (60, 'm')];

/// Active seconds before `used` reaches `limit` at the recent growth
/// rate, or `None` when the question cannot be answered (V92).
///
/// REFUSALS, in order: no red line means nothing to arrive at; a zero or
/// absent growth rate means a context that is not filling, and infinity
/// is not a number, so neither is dressed up as a very large one.
///
/// Reaching or passing the line reports 0 rather than `None`. The gauge
/// is pinned there and the reader is past the point -- that is a
/// measurement, not a missing one, and `saturating_sub` is what says so.
pub(crate) fn seconds(
    used: u64,
    limit: Option<u64>,
    growth_per_hour: Option<u64>,
) -> Option<u64> {
    let limit = limit?;
    let rate = growth_per_hour.filter(|r| *r > 0)?;
    let left = limit.saturating_sub(used);
    left.saturating_mul(HOUR).checked_div(rate)
}

/// A duration, FLOORED (V119).
///
/// The opposite direction from `shorten()`, which ceilings because it
/// publishes token counts (V108). Two functions rather than one with a
/// direction argument: a single rounder that goes both ways by parameter
/// is how the rule stops being visible at the call site.
pub(crate) fn duration(seconds: u64) -> String {
    for (size, unit) in LADDER {
        if seconds >= size {
            let n = seconds.checked_div(size).unwrap_or(0);
            return format!("{n}{unit}");
        }
    }
    format!("{seconds}s")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_estimate_divides_the_room_left_by_the_recent_rate() {
        assert_eq!(
            seconds(900_000, Some(1_000_000), Some(100_000)),
            Some(HOUR)
        );
    }

    /// V92: no red line, no arrival time. The gauge has nothing to
    /// annotate and neither has this.
    #[test]
    fn without_a_red_line_there_is_no_estimate() {
        assert_eq!(seconds(900_000, None, Some(100_000)), None);
    }

    /// A context that is not filling never arrives. Infinity is not a
    /// number, so it is not published as a very large one.
    #[test]
    fn a_context_that_is_not_growing_has_no_arrival_time() {
        assert_eq!(seconds(900_000, Some(1_000_000), Some(0)), None);
        assert_eq!(seconds(900_000, Some(1_000_000), None), None);
    }

    /// Past the line is a MEASUREMENT, not a missing one: the gauge is
    /// pinned and the reader is over the point.
    #[test]
    fn at_or_past_the_line_the_estimate_is_zero_rather_than_absent() {
        assert_eq!(seconds(1_000_000, Some(1_000_000), Some(100_000)), Some(0));
        assert_eq!(seconds(1_200_000, Some(1_000_000), Some(100_000)), Some(0));
    }

    /// V119, the whole point: 90 seconds is one minute, never two. A
    /// deadline rounded up is confidence the reader has not got.
    #[test]
    fn a_duration_floors_rather_than_ceilings() {
        assert_eq!(duration(90), "1m");
        assert_eq!(duration(119), "1m");
        assert_eq!(duration(7_199), "1h");
        assert_eq!(duration(172_799), "1d");
    }

    #[test]
    fn each_unit_takes_over_at_its_own_boundary() {
        assert_eq!(duration(59), "59s");
        assert_eq!(duration(60), "1m");
        assert_eq!(duration(3_600), "1h");
        assert_eq!(duration(86_400), "1d");
    }

    /// Zero is a real answer here -- it is what "past the line" renders
    /// as -- so it must print, not vanish.
    #[test]
    fn zero_seconds_still_prints() {
        assert_eq!(duration(0), "0s");
    }
}
