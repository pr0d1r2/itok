//! The fill gauge: an occupancy LEVEL rendered as colour (V114).
//!
//! `rate`'s first badge value is the last turn's billed input, which by
//! V98 is the whole window on a warm turn. That is a level, and a level
//! is read against a CAPACITY -- how full -- not against a fixed token
//! count, which answers "how full" only for the one window size someone
//! had in mind when they wrote the number down.
//!
//! Everything here is integer arithmetic. A statusline is rendered every
//! turn and its output is compared in tests, so a float's last bit is a
//! flake waiting to happen; hue at S=100/L=50 is piecewise LINEAR anyway,
//! and only two of its six sectors lie between green and red.

/// Full scale. Fills are per-mille so the `sqrt` gamma has resolution to
/// work with while staying in integers.
const FULL: u64 = 1000;

/// Where the level earns a second channel (V115): hue alone cannot carry
/// the signal past a red-green colour blindness, and the last tenth is
/// the part a reader must not miss.
const ALARM: u64 = 900;

/// Green, in degrees. The ramp runs from here to 0 (red) and never
/// enters the blue half of the wheel.
const GREEN_HUE: u64 = 120;

/// Sector width in the HSL wheel.
const SECTOR: u64 = 60;

const MAX_CHANNEL: u64 = 255;

const GREEN: &str = "\x1b[32m";
const AMBER: &str = "\x1b[33m";
const RED: &str = "\x1b[31m";

/// How full, in per-mille of the limit, or `None` when the question
/// cannot be asked.
///
/// V115: an ABSENT or ZERO level is not an empty tank. A turn reporting
/// window 0 is a measurement that went missing (V93 records a real one),
/// and rendering it as the calmest green on the ramp would publish
/// reassurance manufactured from nothing. `None` here becomes no colour
/// at all, which is what V109 already does with a metric that has no
/// threshold.
pub(crate) fn fill(used: u64, limit: u64) -> Option<u64> {
    if used == 0 || limit == 0 {
        return None;
    }
    let scaled = used.saturating_mul(FULL).checked_div(limit)?;
    Some(scaled.min(FULL))
}

/// The mark that survives a monochrome terminal and a colour-blind
/// reader (V115). `~` already means "projected" on this badge, so the
/// alarm takes `!` and the two never collide.
pub(crate) fn alarm(fill: u64) -> &'static str {
    if fill >= ALARM { "!" } else { "" }
}

/// The escape for a fill: a 24-bit ramp where the terminal says it can
/// show one, else the three bands cut from the SAME fraction.
///
/// The bands are the degraded rendering of one gauge, not a second
/// opinion: reading them from `[rate].turn` instead would put a
/// truecolor terminal and a 256-colour one on different instruments.
pub(crate) fn ansi(fill: u64, truecolor: bool) -> String {
    if !truecolor {
        return band(fill).to_owned();
    }
    let (r, g, b) = rgb(hue(fill));
    format!("\x1b[38;2;{r};{g};{b}m")
}

/// The three bands, cut from the ramp rather than from a second set of
/// numbers. Red is `ALARM` -- the same event the mark reports -- and
/// green is the green sector of the wheel, so a 16-colour terminal and a
/// truecolor one change colour at the same fills instead of disagreeing
/// across the whole top fifth of the gauge.
fn band(fill: u64) -> &'static str {
    if fill >= ALARM {
        RED
    } else if hue(fill) > SECTOR {
        GREEN
    } else {
        AMBER
    }
}

/// Hue for a fill, with a `sqrt` gamma: green through the flat half,
/// diving over the last fifth, and PINNED at red from `ALARM` on.
///
/// A LINEAR ramp spends its resolution where nothing is at stake -- the
/// walk from 10% to 50% full is the part of a session nobody needs
/// warning about, and it would eat a third of the wheel. `sqrt` moves
/// the colour slowly while there is room and quickly once there is not,
/// which is the shape of the reader's actual concern.
///
/// The ramp runs out at `ALARM` rather than at `FULL` because V115 put
/// the mark and the colour on the same event, and a ramp that reached
/// red only at `FULL` broke that: at the fill where `!` first appears it
/// still rendered 255,157,0 -- orange, the colour of caution, under a
/// mark that means "do not miss this". Two channels were added to say
/// one thing twice; they have to run out together to do that.
fn hue(fill: u64) -> u64 {
    let room = ALARM.saturating_sub(fill);
    let scaled = room.saturating_mul(FULL).checked_div(ALARM).unwrap_or(0);
    let root = scaled.saturating_mul(FULL).isqrt().min(FULL);
    GREEN_HUE
        .saturating_mul(root)
        .checked_div(FULL)
        .unwrap_or(0)
}

/// Hue to RGB at S=100%, L=50%, for the two sectors that matter. Red is
/// pinned through the first sector and green through the second, which
/// is the whole of the standard conversion once blue is known to be 0.
fn rgb(hue: u64) -> (u64, u64, u64) {
    if hue <= SECTOR {
        (MAX_CHANNEL, sector(hue), 0)
    } else {
        (sector(GREEN_HUE.saturating_sub(hue)), MAX_CHANNEL, 0)
    }
}

fn sector(degrees: u64) -> u64 {
    degrees
        .saturating_mul(MAX_CHANNEL)
        .checked_div(SECTOR)
        .unwrap_or(MAX_CHANNEL)
        .min(MAX_CHANNEL)
}

/// Whether the terminal has said it can render 24-bit colour.
///
/// `COLORTERM` is the only portable signal, and its absence is not a
/// claim that the terminal cannot -- it is the absence of a claim that it
/// can. Degrading on silence is the safe direction: three bands render
/// everywhere, while a 24-bit escape a terminal cannot read prints as
/// literal garbage in the middle of a statusline.
pub(crate) fn truecolor() -> bool {
    std::env::var("COLORTERM")
        .is_ok_and(|v| v.contains("truecolor") || v.contains("24bit"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fill_is_the_level_over_the_limit_in_per_mille() {
        assert_eq!(fill(500, 1000), Some(500));
        assert_eq!(fill(963_000, 1_000_000), Some(963));
    }

    /// V115, the failure this invariant exists for: a zero level is a
    /// measurement that went missing, never an empty tank.
    #[test]
    fn a_zero_level_has_no_fill_at_all() {
        assert_eq!(fill(0, 1_000_000), None);
    }

    #[test]
    fn a_zero_limit_has_no_fill_either() {
        assert_eq!(fill(963_000, 0), None);
    }

    /// Past the limit the gauge pins rather than wrapping: a hue over
    /// 120 degrees would walk back into green from the far side.
    #[test]
    fn a_level_over_the_limit_pins_at_full() {
        assert_eq!(fill(2_000_000, 1_000_000), Some(FULL));
    }

    #[test]
    fn an_empty_gauge_is_green_and_a_full_one_is_red() {
        assert_eq!(rgb(hue(0)), (0, MAX_CHANNEL, 0));
        assert_eq!(rgb(hue(FULL)), (MAX_CHANNEL, 0, 0));
    }

    /// The gamma, stated as a test rather than left in a comment: at
    /// half full the hue is still in the green half of the ramp.
    #[test]
    fn the_ramp_holds_green_through_the_flat_half() {
        assert!(hue(500) > SECTOR, "hue at 50% was {}", hue(500));
        assert!(hue(ALARM) < SECTOR, "hue at the alarm was {}", hue(ALARM));
    }

    #[test]
    fn the_hue_never_leaves_the_green_to_red_arc() {
        for f in 0..=FULL {
            assert!(hue(f) <= GREEN_HUE, "hue({f}) = {}", hue(f));
        }
    }

    /// Monotonic: more full is never greener. A ramp that wobbled would
    /// be worse than no colour, because the reader would learn to
    /// distrust the direction rather than the value.
    #[test]
    fn the_ramp_never_turns_back_toward_green() {
        let mut last = GREEN_HUE;
        for f in 0..=FULL {
            let h = hue(f);
            assert!(h <= last, "hue rose at {f}: {last} then {h}");
            last = h;
        }
    }

    #[test]
    fn the_alarm_mark_appears_only_in_the_last_tenth() {
        assert_eq!(alarm(899), "");
        assert_eq!(alarm(ALARM), "!");
        assert_eq!(alarm(FULL), "!");
    }

    #[test]
    fn without_truecolor_the_same_fill_falls_to_three_bands() {
        assert_eq!(ansi(100, false), GREEN);
        assert_eq!(ansi(800, false), AMBER);
        assert_eq!(ansi(950, false), RED);
    }

    /// The failure this fix exists for: at the fill where `!` first
    /// appears the ramp still rendered orange, so the mark and the
    /// colour contradicted each other for the whole top tenth.
    #[test]
    fn the_alarm_zone_is_red_in_both_renderings() {
        for f in ALARM..=FULL {
            assert_eq!(rgb(hue(f)), (MAX_CHANNEL, 0, 0), "ramp at {f}");
            assert_eq!(band(f), RED, "band at {f}");
            assert_eq!(alarm(f), "!", "mark at {f}");
        }
    }

    /// Below the alarm the ramp must NOT be red yet, or the gauge would
    /// spend its last fifth saying the same thing and the mark would
    /// stop meaning anything.
    #[test]
    fn the_ramp_reaches_red_no_earlier_than_the_alarm() {
        assert!(hue(ALARM - 1) > 0, "hue just under alarm was 0");
    }

    #[test]
    fn with_truecolor_the_fill_becomes_a_24_bit_escape() {
        assert_eq!(ansi(0, true), "\x1b[38;2;0;255;0m");
        assert_eq!(ansi(FULL, true), "\x1b[38;2;255;0;0m");
    }
}
