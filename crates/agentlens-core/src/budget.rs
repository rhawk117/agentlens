use serde::Serialize;

pub const DEFAULT_BUDGET: usize = 4000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Detail {
    Full,
    Summary,
    Counts,
}

impl Detail {
    pub fn next(self) -> Option<Self> {
        match self {
            Self::Full => Some(Self::Summary),
            Self::Summary => Some(Self::Counts),
            Self::Counts => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Full => "full",
            Self::Summary => "summary",
            Self::Counts => "counts",
        }
    }

    pub fn ladder() -> [Self; 3] {
        [Self::Full, Self::Summary, Self::Counts]
    }
}

/// Characters per token, times ten, calibrated so [`estimate_tokens`] is
/// unbiased rather than conservative.
///
/// Measured against `tiktoken` `o200k_base` on 492 samples of real rendered
/// output: 345 exit-0 records from the frozen 108-run Django benchmark, plus
/// 147 invocations over this repo's own committed fixtures. At this value the
/// ratio of true tokens to estimate has a pooled median of 1.001 (Django
/// 0.983, local 1.091), p05 0.856, p95 1.317.
///
/// Safety is deliberately not this constant's job. Folding the ladder's
/// conservatism in here, by dividing by 4 or less, buys a safer ladder only
/// by making the `tokens` field in the JSON envelope read high on a typical
/// call: at 4.0 the median estimate overshoots by 24%. The two jobs are now
/// separate, so this number is the honest figure callers see and
/// [`LADDER_MARGIN_TENTHS`] carries the margin.
///
/// The per-corpus medians still differ by 11%, and no single divisor removes
/// that. Estimating from non-whitespace characters was tested as a cause and
/// rejected: it moved the two corpora's 5th percentiles from 3.81/2.95 to
/// 2.80/2.11, shifting both down rather than converging them. Identifier
/// shape, not padding width, drives the gap.
const CHARS_PER_TOKEN_TENTHS: usize = 49;

/// How much headroom [`fit`] leaves against `--budget`, in tenths.
///
/// [`estimate_tokens`] is unbiased, so it reads low about half the time and
/// the ladder needs a margin of its own for `--budget` to mean anything. This
/// makes `--budget` an approximate guard rather than a hard cap. A hard cap
/// is not reachable with any fixed constant, because the character-to-token
/// ratio varies by corpus; it would need the rendered text run through a real
/// tokenizer after the fact.
///
/// Both directions cost tokens, so the value is the knee of that trade over
/// the same 492 samples rather than a coverage target:
///
/// | margin | budget respected | median headroom wasted | worst overrun |
/// |--------|------------------|------------------------|---------------|
/// | 1.2    | 82.3%            | 16.6%                  | 1.39x         |
/// | 1.3    | 93.3%            | 23.0%                  | 1.28x         |
/// | 1.5    | 99.2%            | 33.3%                  | 1.11x         |
/// | 1.7    | 100.0%           | 41.1%                  | 1.00x         |
///
/// Moving 1.2 to 1.3 buys 11 points of coverage for 6 of waste; 1.3 to 1.5
/// buys 6 for 10. Past the knee the tool spends more on systematically
/// under-filling every call, and on the re-calls that provokes, than it saves
/// on the tail. At 1.5, `--budget 4000` behaves like 2670 on a typical call.
///
/// Chasing 1.7 also reintroduces a ratchet: that value is fitted to the
/// observed maximum, so the first corpus that exceeds it forces another
/// raise. A stated confidence level does not have that property.
const LADDER_MARGIN_TENTHS: usize = 13;

/// Estimates the `o200k_base` token count of rendered output.
///
/// The word-and-punctuation unit count binds on about a quarter of real
/// output, making it a co-equal term rather than a rare floor. It dominates
/// on punctuation-dense renderings such as a settings outline, which is
/// exactly where the character ratio is furthest off. Both terms are inside
/// the calibration on [`CHARS_PER_TOKEN_TENTHS`].
pub fn estimate_tokens(text: &str) -> usize {
    let mut units = 0usize;
    let mut in_word = false;
    for ch in text.chars() {
        if ch.is_alphanumeric() || ch == '_' {
            if !in_word {
                units += 1;
                in_word = true;
            }
        } else {
            in_word = false;
            if !ch.is_whitespace() {
                units += 1;
            }
        }
    }
    let by_chars = (text.chars().count() * 10).div_ceil(CHARS_PER_TOKEN_TENTHS);
    units.max(by_chars)
}

fn ladder_cost(text: &str) -> usize {
    (estimate_tokens(text) * LADDER_MARGIN_TENTHS).div_ceil(10)
}

/// Renders at the most detailed rung that fits the budget.
///
/// The rung is chosen against [`ladder_cost`], which is the estimate plus a
/// margin, while the `tokens` field each op reports is the raw
/// [`estimate_tokens`] figure. Those are two different numbers on purpose:
/// degrading wants a conservative bias and reporting wants accuracy, and one
/// constant serving both means tightening either the ladder or the report
/// necessarily loosens the other.
///
/// Returns the rendered text, the rung it came from, and whether that rung
/// was below `Full`. When nothing fits, `Counts` is returned over budget
/// rather than empty output.
pub fn fit<T, F>(budget: usize, mut render: F) -> (String, Detail, bool)
where
    F: FnMut(Detail) -> T,
    T: AsRef<str>,
{
    let mut last = String::new();
    let mut last_detail = Detail::Full;
    for detail in Detail::ladder() {
        let text = render(detail).as_ref().to_string();
        let cost = ladder_cost(&text);
        if cost <= budget {
            return (text, detail, detail != Detail::Full);
        }
        last = text;
        last_detail = detail;
    }
    (last, last_detail, true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_text_is_free() {
        assert_eq!(estimate_tokens(""), 0);
    }

    #[test]
    fn estimate_grows_with_text() {
        let small = estimate_tokens("def f(): pass");
        let large = estimate_tokens(&"def f(): pass\n".repeat(50));
        assert!(large > small);
    }

    #[test]
    fn ladder_degrades_when_over_budget() {
        let (text, detail, degraded) = fit(3, |detail| match detail {
            Detail::Full => "a very long body that will not fit at all".to_string(),
            Detail::Summary => "still too long for three tokens".to_string(),
            Detail::Counts => "1 item".to_string(),
        });
        assert_eq!(detail, Detail::Counts);
        assert!(degraded);
        assert_eq!(text, "1 item");
    }

    #[test]
    fn ladder_keeps_full_when_it_fits() {
        let (_, detail, degraded) = fit(4000, |_| "short".to_string());
        assert_eq!(detail, Detail::Full);
        assert!(!degraded);
    }

    const SAMPLES: &str = include_str!("../tests/fixtures/token-samples.tsv");

    fn samples() -> Vec<(usize, String)> {
        SAMPLES
            .lines()
            .filter(|line| !line.starts_with('#') && !line.is_empty())
            .map(|line| {
                let mut columns = line.splitn(3, '\t');
                columns.next().expect("command column");
                let tokens = columns.next().expect("token column");
                let text = columns.next().expect("text column");
                (tokens.parse().expect("token count"), unescape(text))
            })
            .collect()
    }

    fn unescape(text: &str) -> String {
        let mut out = String::with_capacity(text.len());
        let mut chars = text.chars();
        while let Some(ch) = chars.next() {
            if ch != '\\' {
                out.push(ch);
                continue;
            }
            match chars.next() {
                Some('n') => out.push('\n'),
                Some('t') => out.push('\t'),
                Some(other) => out.push(other),
                None => out.push('\\'),
            }
        }
        out
    }

    fn ratios() -> Vec<f64> {
        let mut all: Vec<f64> = samples()
            .iter()
            .map(|(tokens, text)| {
                let estimate = estimate_tokens(text);
                assert!(estimate > 0, "sample estimated at zero tokens");
                #[allow(clippy::cast_precision_loss)]
                let ratio = *tokens as f64 / estimate as f64;
                ratio
            })
            .collect();
        all.sort_by(f64::total_cmp);
        all
    }

    /// Pins `estimate_tokens` to the calibration derived from 492 measured
    /// samples. The bound is on the median of true-to-estimate ratios, which
    /// moves in proportion to [`CHARS_PER_TOKEN_TENTHS`]: dropping the
    /// divisor to 4.0 lands the median near 0.94 and fails this test. These
    /// 17 fixture samples are all from this repo, whose median ratio runs
    /// above the pooled 1.001, so the window is centred on 1.15 rather than
    /// on 1.0.
    #[test]
    fn estimator_calibration() {
        let all = ratios();
        let median = all[all.len() / 2];
        assert!(
            (1.05..=1.30).contains(&median),
            "median true/estimate ratio {median:.3} left the calibrated window"
        );
    }

    /// `--budget` is an approximate guard, not a hard cap, so this pins the
    /// size of the residual rather than asserting there is none. One of the
    /// 17 samples exceeds its budget, by 27%. A test asserting zero overrun
    /// would be asserting a promise the tool does not make.
    #[test]
    fn the_ladder_margin_bounds_but_does_not_eliminate_overrun() {
        let worst = samples()
            .iter()
            .map(|(tokens, text)| {
                let allowed = ladder_cost(text);
                #[allow(clippy::cast_precision_loss)]
                let overrun = *tokens as f64 / allowed as f64;
                overrun
            })
            .fold(0.0_f64, f64::max);
        assert!(
            worst <= 1.30,
            "worst overrun {worst:.3} exceeds the documented residual"
        );
    }

    #[test]
    fn the_ladder_degrades_earlier_than_the_reported_estimate_would() {
        let text = "word ".repeat(100);
        let reported = estimate_tokens(&text);
        let (_, detail, degraded) = fit(reported, |detail| match detail {
            Detail::Full => text.clone(),
            _ => "1 item".to_string(),
        });
        assert_ne!(detail, Detail::Full);
        assert!(degraded);
    }
}
