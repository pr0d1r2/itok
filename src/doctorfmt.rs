//! doctor's report rendering (V17): the `Health` aggregate and the two
//! views over it -- the human signal lines and the stable json object --
//! plus the signal MATH they share (pct, spread, verdict). Split from
//! `doctor` so the command module (dispatch + gathering) and its
//! presentation each stay under the module-length limit (V284).

use crate::render::Method;

/// Aggregate health of a fileset. Percentages are whole numbers. Built by
/// `doctor::health`, rendered here.
pub(crate) struct Health {
    pub(crate) dummy_total: u64,
    pub(crate) real_total: u64,
    pub(crate) has_bpe: bool,
    pub(crate) method: &'static Method,
    pub(crate) window: Option<u64>,
    pub(crate) biggest: u64,
    pub(crate) noise: u64,
}

/// Percentage `part` is of `whole`, saturating and division-safe.
fn pct(part: u64, whole: u64) -> u64 {
    part.saturating_mul(100).checked_div(whole).unwrap_or(0)
}

/// The dummy-vs-real spread: percent, and the direction bytes/4 erred.
fn spread(dummy_total: u64, real_total: u64) -> (u64, char) {
    if real_total >= dummy_total {
        (pct(real_total.saturating_sub(dummy_total), real_total), '+')
    } else {
        (
            pct(dummy_total.saturating_sub(real_total), dummy_total),
            '-',
        )
    }
}

/// Where occupancy stops being comfortable, as a percentage of the
/// window. ONE number (V64): `fit` sorts a fileset by it, and
/// `--session` fires V99's advice on it, so a session and a bundle agree
/// about what "nearly full" means.
pub(crate) const FIT_WARN: u64 = 80;

pub(crate) fn verdict(pct: u64, warn_at: u64) -> &'static str {
    if pct > 100 {
        "OVER"
    } else if pct >= warn_at {
        "warn"
    } else {
        "ok"
    }
}

pub(crate) fn human(h: &Health) -> String {
    let method = h.method.label();
    let mut s = format!("context: {} itok ({method})\n", h.real_total);
    s.push_str(&fit_line(h));
    s.push_str(&balance_line(h));
    s.push_str(&noise_line(h));
    s.push_str(&confidence_line(h));
    s
}

fn fit_line(h: &Health) -> String {
    match h.window {
        None => "  fit         (pass --window to check)\n".to_owned(),
        Some(w) => {
            let p = pct(h.real_total, w);
            format!(
                "  fit         {} / {w}  {p}%  {}\n",
                h.real_total,
                verdict(p, FIT_WARN)
            )
        }
    }
}

fn balance_line(h: &Health) -> String {
    let p = pct(h.biggest, h.real_total);
    format!("  balance     biggest file {p}%  {}\n", verdict(p, 50))
}

fn noise_line(h: &Health) -> String {
    let p = pct(h.noise, h.real_total);
    format!("  noise       lockfiles {p}%  {}\n", verdict(p, 20))
}

fn confidence_line(h: &Health) -> String {
    if !h.has_bpe {
        return "  confidence  n/a (build with the bpe feature)\n".to_owned();
    }
    let (s, dir) = spread(h.dummy_total, h.real_total);
    format!("  confidence  bytes/4 is {dir}{s}% vs o200k\n")
}

pub(crate) fn json(h: &Health) -> String {
    let method = h.method.label();
    let window = h
        .window
        .map_or_else(|| "null".to_owned(), |w| w.to_string());
    let fit = h.window.map_or(0, |w| pct(h.real_total, w));
    format!(
        "{{\"total_tokens\":{},\"method\":\"{method}\",\"window\":{window},\
         \"fit_pct\":{fit},\"biggest_pct\":{},\"noise_pct\":{},\
         \"confidence_pct\":{}}}\n",
        h.real_total,
        pct(h.biggest, h.real_total),
        pct(h.noise, h.real_total),
        spread(h.dummy_total, h.real_total).0,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    // has_bpe:false and window:None are unreachable through a default
    // (bpe-on, real-repo) run, so drive them with a built Health.
    fn h(real: u64, dummy: u64, window: Option<u64>, bpe: bool) -> Health {
        let method = if bpe {
            &crate::render::O200K
        } else {
            &crate::render::DUMMY
        };
        Health {
            dummy_total: dummy,
            real_total: real,
            has_bpe: bpe,
            method,
            window,
            biggest: real,
            noise: 0,
        }
    }

    #[test]
    fn pct_is_division_safe() {
        assert_eq!(pct(50, 200), 25);
        assert_eq!(pct(5, 0), 0);
    }

    #[test]
    fn spread_reports_direction() {
        assert_eq!(spread(80, 100), (20, '+'));
        assert_eq!(spread(100, 80), (20, '-'));
    }

    #[test]
    fn verdict_thresholds() {
        assert_eq!(verdict(10, 80), "ok");
        assert_eq!(verdict(90, 80), "warn");
        assert_eq!(verdict(120, 80), "OVER");
    }

    #[test]
    fn confidence_is_na_without_bpe() {
        assert!(confidence_line(&h(100, 80, None, false)).contains("n/a"));
    }

    #[test]
    fn human_names_the_dummy_method_without_bpe() {
        assert!(human(&h(100, 80, None, false)).contains("bytes/4"));
    }

    #[test]
    fn json_window_is_null_when_absent() {
        let j = json(&h(100, 80, None, true));
        assert!(j.contains("\"window\":null"));
        assert!(j.contains("\"fit_pct\":0"));
    }
}
