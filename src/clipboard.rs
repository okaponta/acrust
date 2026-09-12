//! クリップボードへのコピー。`acrust copy` が使う。
//!
//! `arboard` のようなクレートを入れず外部コマンドを叩くのは、Linux で X11 / Wayland の
//! 開発パッケージを要求されるため。AtCoder 用の CLI に持ち込むには重い。
//! ブラウザ起動（`browser`）と同じ割り切り。

use anyhow::{bail, Context as _, Result};
use std::io::Write as _;
use std::process::{Command, Stdio};

/// この順に試して、最初に動いたものを使う。
fn candidates() -> &'static [(&'static str, &'static [&'static str])] {
    if cfg!(target_os = "macos") {
        &[("pbcopy", &[])]
    } else if cfg!(target_os = "windows") {
        &[("clip", &[])]
    } else {
        // Wayland → X11 の順。環境によってどちらか片方しか入っていない。
        &[
            ("wl-copy", &[]),
            ("xclip", &["-selection", "clipboard"]),
            ("xsel", &["--clipboard", "--input"]),
        ]
    }
}

/// `text` をクリップボードに入れる。戻り値は実際に使ったコマンド名。
pub fn copy(text: &str) -> Result<&'static str> {
    for (command, args) in candidates() {
        if run(command, args, text)? {
            return Ok(command);
        }
    }
    let tried: Vec<&str> = candidates().iter().map(|(command, _)| *command).collect();
    bail!(
        "クリップボードにコピーできませんでした（{} のいずれも見つかりません）",
        tried.join(" / ")
    )
}

/// コマンドが見つからなければ `Ok(false)`。見つかって失敗したときはエラー。
fn run(command: &str, args: &[&str], text: &str) -> Result<bool> {
    let mut child = match Command::new(command)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(child) => child,
        // 入っていないだけなら次の候補へ。
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(e) => return Err(e).with_context(|| format!("{command} を起動できませんでした")),
    };

    {
        let mut stdin = child.stdin.take().context("標準入力を掴めませんでした")?;
        stdin
            .write_all(text.as_bytes())
            .with_context(|| format!("{command} に書き込めませんでした"))?;
        // ここで drop されて EOF が伝わる。閉じないと相手が読み終わらない。
    }

    let status = child
        .wait()
        .with_context(|| format!("{command} の終了を待てませんでした"))?;
    if !status.success() {
        bail!("{command} が {status} で終了しました");
    }
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_platform_has_at_least_one_way_to_copy() {
        assert!(!candidates().is_empty());
    }

    /// 入っていないコマンドはエラーではなく「次を試す」になること。
    #[test]
    fn a_missing_command_is_not_an_error() {
        assert!(!run("acrust-no-such-clipboard-command", &[], "x").unwrap());
    }

    /// 実際に往復させて中身が一致すること。
    ///
    /// **手元のクリップボードを書き換える**ので `live` を付けたときだけ走らせる
    /// （`cargo test` のたびにコピー中のものが消えるのは困る）。
    #[cfg(all(target_os = "macos", feature = "live"))]
    #[test]
    fn macos_copies_through_pbcopy() {
        let text = "fn main() { println!(\"テスト\"); }\n";
        assert_eq!(copy(text).unwrap(), "pbcopy");
        let pasted = Command::new("pbpaste").output().unwrap().stdout;
        assert_eq!(String::from_utf8(pasted).unwrap(), text);
    }
}
