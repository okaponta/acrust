//! ターミナル出力のごく薄いラッパ。
//!
//! 色は stdout/stderr が TTY のときだけ付ける。`NO_COLOR` が設定されていれば常に無色。

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

/// `→ abc474 c (src/bin/c.rs)` のような、推定結果や進行状況の1行。
// test / run / submit（M3 以降）が使う。
#[allow(dead_code)]
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

pub fn error(msg: &str) {
    if color_enabled() {
        eprintln!("{} {}", "error:".red().bold(), msg);
    } else {
        eprintln!("error: {msg}");
    }
}

/// `status` などのラベル付き行。ラベル幅を揃える。
pub fn field(label: &str, value: &str) {
    if color_enabled() {
        println!("  {:<14} {}", format!("{label}:").dimmed(), value);
    } else {
        println!("  {:<14} {}", format!("{label}:"), value);
    }
}

/// `sample1 AC  12 ms` の1行。
pub fn verdict(label: &str, accepted: bool, name: &str, detail: &str) {
    if color_enabled() {
        let label = if accepted {
            format!("{:<3}", label.green().bold().to_string())
        } else {
            format!("{:<3}", label.red().bold().to_string())
        };
        println!("  {label}  {:<12} {}", name, detail.dimmed());
    } else {
        println!("  {label:<3}  {name:<12} {detail}");
    }
}

/// 失敗したケースの見出し。
pub fn section(title: &str) {
    if color_enabled() {
        println!("{}", format!("── {title} ").bold());
    } else {
        println!("── {title} ");
    }
}

/// 入力や標準エラーなど、そのまま見せたいテキスト。
pub fn block(label: &str, text: &str) {
    block_limited(label, text, usize::MAX);
}

/// 長いテキストは頭だけ見せる。バックトレースで画面が流れてしまうのを防ぐ。
pub fn block_limited(label: &str, text: &str, max_lines: usize) {
    println!("  {label}:");
    let lines: Vec<&str> = text.lines().collect();
    for line in lines.iter().take(max_lines) {
        println!("    {line}");
    }
    if lines.len() > max_lines {
        let hidden = lines.len() - max_lines;
        println!("    …（あと {hidden} 行）");
    }
    if text.is_empty() {
        println!("    （空）");
    }
}

/// 期待と実際を並べ、食い違う行に印を付ける。
/// 行が足りないことを示す印。全角を混ぜると桁が揃わないので ASCII にする。
const MISSING: &str = "~";

pub fn diff(expected_label: &str, expected: &str, actual_label: &str, actual: &str) {
    let first = crate::judge::first_difference(expected, actual);
    let expected_lines = crate::judge::lines(expected);
    let actual_lines = crate::judge::lines(actual);
    let count = expected_lines.len().max(actual_lines.len());

    println!("  {expected_label} / {actual_label}:");
    for i in 0..count {
        let want = expected_lines.get(i).copied().unwrap_or(MISSING);
        let got = actual_lines.get(i).copied().unwrap_or(MISSING);
        let differs = expected_lines.get(i) != actual_lines.get(i);
        let marker = if differs { "✗" } else { " " };
        if differs && color_enabled() {
            println!(
                "    {} {:<3} {:<24} | {}",
                marker.red(),
                i + 1,
                want.green(),
                got.red()
            );
        } else {
            println!("    {marker} {:<3} {want:<24} | {got}", i + 1);
        }
    }
    if count == 0 {
        println!("    （どちらも空）");
    }
    if let Some(first) = first {
        println!("    最初に食い違う行: {}", first + 1);
    }
}
