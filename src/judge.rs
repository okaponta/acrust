//! Deciding whether an answer is right.
//!
//! Four matching modes, no more: 2,716 real test suites written by cargo-compete
//! needed nothing else. A problem that accepts several answers cannot be judged
//! automatically at all, so those get their `match` set by hand.

use crate::testcases::{FloatTolerance, Matching};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    Accepted,
    WrongAnswer,
    /// Died rather than finished: a panic or a signal.
    RuntimeError,
    TimeLimitExceeded,
}

impl Verdict {
    pub fn label(self) -> &'static str {
        match self {
            Verdict::Accepted => "AC",
            Verdict::WrongAnswer => "WA",
            Verdict::RuntimeError => "RE",
            Verdict::TimeLimitExceeded => "TLE",
        }
    }

    pub fn is_accepted(self) -> bool {
        matches!(self, Verdict::Accepted)
    }
}

pub fn matches(
    expected: &str,
    actual: &str,
    matching: Matching,
    float: Option<FloatTolerance>,
) -> bool {
    match matching {
        Matching::Exact => expected == actual,
        Matching::Lines => lines(expected) == lines(actual),
        Matching::Words => words(expected) == words(actual),
        Matching::Float => float_match(expected, actual, float.unwrap_or_default()),
    }
}

/// Each line with its trailing whitespace gone, and no blank lines at the end.
pub fn lines(text: &str) -> Vec<&str> {
    let mut lines: Vec<&str> = text.lines().map(|line| line.trim_end()).collect();
    while lines.last().is_some_and(|line| line.is_empty()) {
        lines.pop();
    }
    lines
}

fn words(text: &str) -> Vec<&str> {
    text.split_whitespace().collect()
}

/// Compares token by token as numbers, falling back to string equality for the
/// tokens that are not numbers.
fn float_match(expected: &str, actual: &str, tolerance: FloatTolerance) -> bool {
    let expected = words(expected);
    let actual = words(actual);
    if expected.len() != actual.len() {
        return false;
    }
    expected.iter().zip(&actual).all(|(want, got)| {
        match (want.parse::<f64>(), got.parse::<f64>()) {
            (Ok(want), Ok(got)) => within(want, got, tolerance),
            _ => want == got,
        }
    })
}

fn within(expected: f64, actual: f64, tolerance: FloatTolerance) -> bool {
    if expected == actual {
        return true;
    }
    if !expected.is_finite() || !actual.is_finite() {
        return false;
    }
    let difference = (expected - actual).abs();
    let absolute_ok = tolerance
        .absolute_error
        .is_some_and(|limit| difference <= limit);
    let relative_ok = tolerance
        .relative_error
        .is_some_and(|limit| difference <= limit * expected.abs());
    // With neither bound given, only exact equality passes — handled above.
    absolute_ok || relative_ok
}

/// The first line where the two differ, zero-based, for the failure display.
pub fn first_difference(expected: &str, actual: &str) -> Option<usize> {
    let expected = lines(expected);
    let actual = lines(actual);
    let count = expected.len().max(actual.len());
    (0..count).find(|&i| expected.get(i) != actual.get(i))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn float(relative: Option<f64>, absolute: Option<f64>) -> Option<FloatTolerance> {
        Some(FloatTolerance {
            relative_error: relative,
            absolute_error: absolute,
        })
    }

    #[test]
    fn lines_ignores_trailing_whitespace_and_trailing_blank_lines() {
        assert!(matches("Yes\n", "Yes\n", Matching::Lines, None));
        assert!(matches("Yes\n", "Yes", Matching::Lines, None));
        assert!(matches("Yes\n", "Yes  \n\n\n", Matching::Lines, None));
        assert!(matches("1\n2\n", "1\n2\n", Matching::Lines, None));
        // Neither a different line nor a missing one slips through.
        assert!(!matches("1\n2\n", "1\n3\n", Matching::Lines, None));
        assert!(!matches("1\n2\n", "1\n", Matching::Lines, None));
        assert!(
            !matches("Yes\n", " Yes\n", Matching::Lines, None),
            "leading whitespace is significant"
        );
    }

    #[test]
    fn exact_is_byte_equality() {
        assert!(matches("Yes\n", "Yes\n", Matching::Exact, None));
        assert!(!matches("Yes\n", "Yes", Matching::Exact, None));
    }

    #[test]
    fn words_ignores_how_the_tokens_are_laid_out() {
        assert!(matches("1 2 3\n", "1\n2\n3\n", Matching::Words, None));
        assert!(!matches("1 2 3\n", "1 2\n", Matching::Words, None));
    }

    #[test]
    fn float_uses_the_tolerance_from_the_statement() {
        let tolerance = float(None, Some(1e-9));
        assert!(matches(
            "0.5\n",
            "0.5000000001\n",
            Matching::Float,
            tolerance
        ));
        assert!(!matches("0.5\n", "0.500001\n", Matching::Float, tolerance));

        // With only a relative bound, larger values get more room.
        let relative = float(Some(1e-6), None);
        assert!(matches(
            "1000000\n",
            "1000000.5\n",
            Matching::Float,
            relative
        ));
        assert!(!matches("1\n", "1.5\n", Matching::Float, relative));
    }

    #[test]
    fn float_falls_back_to_string_comparison_for_non_numbers() {
        let tolerance = float(Some(1e-6), Some(1e-6));
        assert!(matches(
            "Yes 1.0\n",
            "Yes 1.0000001\n",
            Matching::Float,
            tolerance
        ));
        assert!(!matches(
            "Yes 1.0\n",
            "No 1.0\n",
            Matching::Float,
            tolerance
        ));
        // A different number of tokens is a mismatch.
        assert!(!matches("1.0\n", "1.0 1.0\n", Matching::Float, tolerance));
    }

    #[test]
    fn float_without_a_tolerance_requires_equality() {
        assert!(matches("0.5\n", "0.5\n", Matching::Float, None));
        assert!(!matches("0.5\n", "0.5000001\n", Matching::Float, None));
    }

    #[test]
    fn non_finite_values_never_slip_through() {
        let tolerance = float(Some(1.0), Some(1.0));
        assert!(!matches("1.0\n", "inf\n", Matching::Float, tolerance));
        assert!(!matches("1.0\n", "NaN\n", Matching::Float, tolerance));
        // Identical text still passes.
        assert!(matches("inf\n", "inf\n", Matching::Float, tolerance));
    }

    #[test]
    fn the_first_differing_line_is_reported() {
        assert_eq!(first_difference("1\n2\n3\n", "1\n2\n3\n"), None);
        assert_eq!(first_difference("1\n2\n3\n", "1\nX\n3\n"), Some(1));
        assert_eq!(first_difference("1\n2\n", "1\n2\n3\n"), Some(2));
        assert_eq!(first_difference("1\n2\n3\n", "1\n2\n"), Some(2));
    }
}
