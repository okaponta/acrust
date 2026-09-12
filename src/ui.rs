//! A very thin wrapper around terminal output.
//!
//! Colour only when both stdout and stderr are a TTY, and never when `NO_COLOR`
//! is set.
//!
//! Columns are aligned by display width, not character count. Problem titles come
//! from AtCoder and are usually Japanese, and `{:<14}` would push every row after
//! one of those out of line.

use owo_colors::OwoColorize as _;
use std::io::IsTerminal as _;
use std::sync::OnceLock;

fn color_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| {
        if std::env::var_os("NO_COLOR").is_some() {
            return false;
        }
        std::io::stdout().is_terminal() && std::io::stderr().is_terminal()
    })
}

/// Wide enough for the longest label acrust prints (`language-list`, 13).
const LABEL_WIDTH: usize = 14;

/// How many lines one block may print.
///
/// A WA whose output runs to tens of thousands of lines must not scroll the
/// terminal away. The window always includes the line that differs.
const MAX_BLOCK_LINES: usize = 20;

/// Display width, counting full-width characters as two columns.
///
/// What acrust prints is ASCII, Japanese, and `✓ ✗ → ─`, so the East Asian
/// wide ranges are the whole problem — not enough to justify `unicode-width`.
pub fn display_width(s: &str) -> usize {
    s.chars().map(char_width).sum()
}

fn char_width(c: char) -> usize {
    match c as u32 {
        0x1100..=0x115F           // Hangul jamo
        | 0x2E80..=0x303E         // CJK radicals and punctuation
        | 0x3041..=0x33FF         // kana and compatibility forms
        | 0x3400..=0x4DBF
        | 0x4E00..=0x9FFF         // CJK ideographs
        | 0xA000..=0xA4CF
        | 0xAC00..=0xD7A3
        | 0xF900..=0xFAFF
        | 0xFE30..=0xFE6F
        | 0xFF00..=0xFF60         // full-width forms
        | 0xFFE0..=0xFFE6 => 2,
        _ => 1,
    }
}

/// Pads on the right to `width` columns, leaving anything wider alone.
fn pad(s: &str, width: usize) -> String {
    let mut padded = s.to_owned();
    for _ in display_width(s)..width {
        padded.push(' ');
    }
    padded
}

/// A line of progress, or what was inferred: `→ abc474 c (src/bin/c.rs)`.
pub fn arrow(msg: &str) {
    if color_enabled() {
        println!("  {} {}", "→".cyan(), msg);
    } else {
        println!("  → {msg}");
    }
}

pub fn info(msg: &str) {
    println!("{msg}");
}

pub fn ok(msg: &str) {
    if color_enabled() {
        println!("{} {}", "✓".green(), msg);
    } else {
        println!("✓ {msg}");
    }
}

pub fn warn(msg: &str) {
    if color_enabled() {
        eprintln!("{} {}", "warning:".yellow().bold(), msg);
    } else {
        eprintln!("warning: {msg}");
    }
}

/// A continuation line under `warning:`, indented to line up with it.
pub fn warn_detail(msg: &str) {
    eprintln!("         {msg}");
}

pub fn error(msg: &str) {
    if color_enabled() {
        eprintln!("{} {}", "error:".red().bold(), msg);
    } else {
        eprintln!("error: {msg}");
    }
}

/// A labelled row, as `migrate` and `env update` print.
pub fn field(label: &str, value: &str) {
    let label = pad(&format!("{label}:"), LABEL_WIDTH);
    if color_enabled() {
        println!("  {} {value}", label.dimmed());
    } else {
        println!("  {label} {value}");
    }
}

/// The mark on a `status` row, so that "is anything wrong" is answered at a glance.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mark {
    Ok,
    /// Neither good nor bad; this is just how things are.
    Info,
    /// Works, but something is worth doing.
    Todo,
    /// Has to be fixed before anything works.
    Bad,
}

impl Mark {
    fn symbol(self) -> &'static str {
        match self {
            Mark::Ok => "✓",
            Mark::Info => "·",
            Mark::Todo => "!",
            Mark::Bad => "✗",
        }
    }

    fn paint(self) -> String {
        if !color_enabled() {
            return self.symbol().to_owned();
        }
        match self {
            Mark::Ok => self.symbol().green().to_string(),
            Mark::Info => self.symbol().dimmed().to_string(),
            Mark::Todo => self.symbol().yellow().bold().to_string(),
            Mark::Bad => self.symbol().red().bold().to_string(),
        }
    }
}

/// One `  ✓ label        value` row. The caller decides the label width.
pub fn row(mark: Mark, label: &str, value: &str, label_width: usize) {
    println!("  {} {}  {value}", mark.paint(), pad(label, label_width));
}

/// The closing line. This is where `status` commits to an answer.
pub fn summary(mark: Mark, msg: &str) {
    println!("{} {msg}", mark.paint());
}

/// One `AC  sample1      12 ms` row.
pub fn verdict(label: &str, accepted: bool, name: &str, detail: &str) {
    let label = pad(label, 3);
    let name = pad(name, 12);
    if color_enabled() {
        let label = if accepted {
            label.green().bold().to_string()
        } else {
            label.red().bold().to_string()
        };
        println!("  {label}  {name} {}", detail.dimmed());
    } else {
        println!("  {label}  {name} {detail}");
    }
}

/// The heading above a failed case.
pub fn section(title: &str) {
    if color_enabled() {
        println!("{}", format!("── {title}").bold());
    } else {
        println!("── {title}");
    }
}

/// A one-line value under the same kind of heading `block` uses, so that it sits
/// level with `input:` and `stderr:`.
pub fn inline(label: &str, value: &str) {
    println!("{label}: {value}");
}

/// Text shown as it is: input, stderr, and the like.
pub fn block(label: &str, text: &str) {
    block_limited(label, text, MAX_BLOCK_LINES);
}

/// Shows the head of a long text, so a big input cannot scroll the screen away.
pub fn block_limited(label: &str, text: &str, max_lines: usize) {
    println!("{label}:");
    let lines: Vec<&str> = text.lines().collect();
    for line in lines.iter().take(max_lines) {
        println!("  {line}");
    }
    if lines.len() > max_lines {
        println!("  … ({} more lines)", lines.len() - max_lines);
    }
    if text.is_empty() {
        println!("  (empty)");
    }
}

/// Expected and actual, one under the other, numbered, with `✗` on the lines that
/// differ.
///
/// Side by side was tried and dropped: a problem with long lines wraps, and a
/// wrapped comparison cannot be read at all.
///
/// The text itself is not indented, so it can be copied straight back out; only
/// the line number and the `✗` sit to the left of it.
pub fn expected_and_output(expected: &str, actual: &str) {
    let want = crate::judge::lines(expected);
    let got = crate::judge::lines(actual);
    let first = crate::judge::first_difference(expected, actual);
    let number_width = digits(want.len().max(got.len()));

    numbered("expected", &want, &got, first, number_width, true);
    numbered("output", &got, &want, first, number_width, false);
}

/// One numbered block. `other` is what each line is compared against.
fn numbered(
    label: &str,
    lines: &[&str],
    other: &[&str],
    first: Option<usize>,
    number_width: usize,
    is_expected: bool,
) {
    println!("{label}:");
    if lines.is_empty() {
        println!("  (empty)");
        return;
    }

    // Skip ahead when the differing line falls past the window: a comparison
    // that does not show the difference is worth nothing.
    let start = match first {
        Some(first) if first >= MAX_BLOCK_LINES => first.saturating_sub(2),
        _ => 0,
    };
    let end = (start + MAX_BLOCK_LINES).min(lines.len());
    if start > 0 {
        println!("… ({start} lines above)");
    }
    for i in start..end {
        let line = lines[i];
        let differs = lines.get(i) != other.get(i);
        let marker = if differs { "✗" } else { " " };
        let number = pad(&(i + 1).to_string(), number_width);
        if differs && color_enabled() {
            let line = if is_expected {
                line.green().to_string()
            } else {
                line.red().to_string()
            };
            println!("{} {number}  {line}", marker.red());
        } else {
            println!("{marker} {number}  {line}");
        }
    }
    if end < lines.len() {
        println!("… ({} more lines)", lines.len() - end);
    }
}

/// Decimal digits in `n`, which is how wide the line-number column has to be.
fn digits(n: usize) -> usize {
    n.to_string().len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn japanese_labels_count_as_two_columns_each() {
        assert_eq!(display_width("rustc"), 5);
        assert_eq!(display_width("依存クレート"), 12);
        assert_eq!(display_width("ジャッジ環境"), 12);
        // Marks and rules count as one column, which is how terminals draw them.
        assert_eq!(display_width("✓"), 1);
        assert_eq!(display_width("✗"), 1);
        assert_eq!(display_width("→"), 1);
    }

    #[test]
    fn padding_lines_up_mixed_scripts() {
        assert_eq!(pad("依存クレート", 12), "依存クレート");
        assert_eq!(pad("rustc", 12), "rustc       ");
        assert_eq!(display_width(&pad("rustc", 12)), 12);
        assert_eq!(display_width(&pad("ジャッジ環境", 12)), 12);
        // Already wider than asked for: left alone, never truncated.
        assert_eq!(pad("language-list", 12), "language-list");
    }

    #[test]
    fn the_line_number_column_grows_with_the_line_count() {
        assert_eq!(digits(0), 1);
        assert_eq!(digits(9), 1);
        assert_eq!(digits(10), 2);
        assert_eq!(digits(1000), 4);
    }
}
