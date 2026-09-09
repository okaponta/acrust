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
