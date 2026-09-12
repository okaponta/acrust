//! ターミナル出力のごく薄いラッパ。
//!
//! 色は stdout/stderr が TTY のときだけ付ける。`NO_COLOR` が設定されていれば常に無色。
//!
//! 桁揃えは**文字数ではなく表示幅**で行う。ラベルを日本語にしたので、
//! `{:<14}` のような文字数ベースの詰めでは全角ぶんだけ右にずれる。

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

/// ラベル列の幅。`依存クレート`（12 桁）と `language-list`（13 桁）が収まる幅。
const LABEL_WIDTH: usize = 14;

/// 1ブロックに出す最大行数。
///
/// WA のとき出力が何万行あっても画面を流さないための上限。食い違う行は必ず窓に入れる。
const MAX_BLOCK_LINES: usize = 20;

/// 全角を 2 桁として数えた表示幅。
///
/// acrust が出すのは ASCII と日本語、それに `✓ ✗ → ─` だけなので、
/// 東アジアの全角レンジだけ見れば足りる（`unicode-width` を入れるほどではない）。
pub fn display_width(s: &str) -> usize {
    s.chars().map(char_width).sum()
}

fn char_width(c: char) -> usize {
    match c as u32 {
        0x1100..=0x115F           // ハングル字母
        | 0x2E80..=0x303E         // CJK 部首・約物（、。「」）
        | 0x3041..=0x33FF         // ひらがな・カタカナ・互換文字
        | 0x3400..=0x4DBF
        | 0x4E00..=0x9FFF         // 漢字
        | 0xA000..=0xA4CF
        | 0xAC00..=0xD7A3
        | 0xF900..=0xFAFF
        | 0xFE30..=0xFE6F
        | 0xFF00..=0xFF60         // 全角英数・全角括弧
        | 0xFFE0..=0xFFE6 => 2,
        _ => 1,
    }
}

/// 表示幅が `width` になるまで右に空白を足す。足りていればそのまま。
fn pad(s: &str, width: usize) -> String {
    let mut padded = s.to_owned();
    for _ in display_width(s)..width {
        padded.push(' ');
    }
    padded
}

/// `→ abc474 c (src/bin/c.rs)` のような、推定結果や進行状況の1行。
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

/// `warning:` の続きの行。`warning: ` のぶんだけ字下げして揃える。
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

/// `migrate` や `env update` などのラベル付き行。ラベル幅を揃える。
pub fn field(label: &str, value: &str) {
    let label = pad(&format!("{label}:"), LABEL_WIDTH);
    if color_enabled() {
        println!("  {} {value}", label.dimmed());
    } else {
        println!("  {label} {value}");
    }
}

/// `status` の各行に付ける判定。OK かどうかが一目で分かるようにするためのもの。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mark {
    Ok,
    /// 良いも悪いもなく、ただそうなっているだけ。
    Info,
    /// 動くが、やっておいた方がいいことがある。
    Todo,
    /// 直さないと使えない。
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

/// `  ✓ ラベル        値` の1行。ラベル幅は呼び出し側が揃える。
pub fn row(mark: Mark, label: &str, value: &str, label_width: usize) {
    println!("  {} {}  {value}", mark.paint(), pad(label, label_width));
}

/// 最後にまとめて出す一言。`status` が OK かどうかをここで言い切る。
pub fn summary(mark: Mark, msg: &str) {
    println!("{} {msg}", mark.paint());
}

/// `AC  sample1      12 ms` の1行。
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

/// 失敗したケースの見出し。
pub fn section(title: &str) {
    if color_enabled() {
        println!("{}", format!("── {title}").bold());
    } else {
        println!("── {title}");
    }
}

/// `block` と同じ見出しで、値が 1 行に収まるもの。桁は詰めない
/// （`input:` や `stderr:` の見出しと同じ高さに揃えたいので）。
pub fn inline(label: &str, value: &str) {
    println!("{label}: {value}");
}

/// 入力や標準エラーなど、そのまま見せたいテキスト。
pub fn block(label: &str, text: &str) {
    block_limited(label, text, MAX_BLOCK_LINES);
}

/// 長いテキストは頭だけ見せる。長い入力やパニックのメッセージで画面が流れるのを防ぐ。
pub fn block_limited(label: &str, text: &str, max_lines: usize) {
    println!("{label}:");
    let lines: Vec<&str> = text.lines().collect();
    for line in lines.iter().take(max_lines) {
        println!("  {line}");
    }
    if lines.len() > max_lines {
        println!("  …（あと {} 行）", lines.len() - max_lines);
    }
    if text.is_empty() {
        println!("  （空）");
    }
}

/// 期待した出力と実際の出力を、別々のブロックにして行番号付きで並べる。
///
/// 食い違う行には `✗` を付ける。横に並べる形（`期待 / 実際`）をやめたのは、
/// 1 行が長い問題だと折り返して読めなくなるため。
///
/// 字下げしないのは、そのままコピーして使えるようにするため（cargo-compete も
/// 失敗したケースの中身を左端から出す）。行番号と `✗` のぶんだけ右にずれる。
pub fn expected_and_output(expected: &str, actual: &str) {
    let want = crate::judge::lines(expected);
    let got = crate::judge::lines(actual);
    let first = crate::judge::first_difference(expected, actual);
    let number_width = digits(want.len().max(got.len()));

    numbered("expected", &want, &got, first, number_width, true);
    numbered("output", &got, &want, first, number_width, false);
}

/// 行番号と `✗` を付けてブロックを1つ出す。`other` は食い違いの判定に使う相手。
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
        println!("  （空）");
        return;
    }

    // 食い違う行が上限より後ろにあるときは、そこまで飛ばす。見えないと意味がない。
    let start = match first {
        Some(first) if first >= MAX_BLOCK_LINES => first.saturating_sub(2),
        _ => 0,
    };
    let end = (start + MAX_BLOCK_LINES).min(lines.len());
    if start > 0 {
        println!("…（前略 {start} 行）");
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
        println!("…（あと {} 行）", lines.len() - end);
    }
}

/// `n` を 10 進で書いたときの桁数。行番号の幅を揃えるのに使う。
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
        // 印と罫線は 1 桁として扱う（端末もそう描く）。
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
        // 足りていれば切らない。
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
