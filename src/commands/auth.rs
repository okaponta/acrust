//! `acrust login` / `logout` / `status`。

use crate::atcoder::{auth, AtCoderClient};
use crate::config::{AtcoderConfig, LoadedConfig};
use crate::session::{self, Session};
use crate::ui;
use crate::workspace::{resolve_problem, Origin, Package};
use anyhow::{Context as _, Result};
use std::sync::OnceLock;

/// ブラウザで取得したセッションクッキーを取り込んで保存する。
///
/// AtCoder の `/login` は Cloudflare Turnstile で守られており、ID / パスワードの
/// POST はプログラムからは通らない（`atcoder::auth` のモジュールコメント参照）。
pub fn login(cookie: Option<String>) -> Result<()> {
    let atcoder = atcoder_config();
    let client = AtCoderClient::new(&atcoder)?;

    if cookie.is_none() && client.load_session()? {
        if let Some(user) = auth::current_user(&client)? {
            ui::ok(&format!("すでに {user} としてログインしています"));
            ui::info("別のユーザーで入り直すには `acrust logout` を実行してください");
            return Ok(());
        }
        // 期限切れのセッションが残っていた。捨ててから入り直す。
        client.clear_cookies();
    }

    let pasted = match cookie {
        Some(cookie) => cookie,
        None => {
            print_instructions();
            read_secret("REVEL_SESSION: ")?
        }
    };
    let value = auth::extract_session_value(&pasted).context("セッションクッキーが空です")?;

    let user = auth::verify_session_cookie(&client, &value)?;
    let session = Session::new(value, user.clone());
    let path = session::save(&session)?;

    ui::ok(&format!("{user} としてログインしました"));
    ui::field("session", &format!("{} (0600)", path.display()));
    Ok(())
}

/// 端末なら伏せ字で、パイプ越しなら普通に 1 行読む。
fn read_secret(label: &str) -> Result<String> {
    use std::io::IsTerminal as _;

    if std::io::stdin().is_terminal() {
        return rpassword::prompt_password(label).context("入力を読めませんでした");
    }
    let mut line = String::new();
    std::io::stdin()
        .read_line(&mut line)
        .context("標準入力を読めませんでした")?;
    Ok(line)
}

fn print_instructions() {
    ui::info(
        "ブラウザの https://atcoder.jp の Cookie から REVEL_SESSION の値を貼り付けてください。",
    );
    ui::info("（DevTools → Application → Cookies → https://atcoder.jp）");
    ui::info("");
}

/// 保存済みのセッションを破棄する。
pub fn logout() -> Result<()> {
    match session::discard()? {
        Some(path) => {
            ui::ok(&format!("セッションを破棄しました（{}）", path.display()));
        }
        None => ui::info("保存されたセッションはありません"),
    }
    Ok(())
}

/// ログイン状態・設定の場所・ジャッジ環境のバージョンを表示する。
pub fn status(offline: bool) -> Result<()> {
    ui::info(&format!("acrust {}", env!("CARGO_PKG_VERSION")));
    ui::info("");

    let loaded = LoadedConfig::find();
    match &loaded {
        Ok(loaded) => {
            // 設定ファイルを探す手間を無くすため、絶対パスを必ず出す（決定 D2）。
            ui::field("config", &loaded.path.display().to_string());
            ui::field("root", &loaded.root.display().to_string());
            ui::field("judge env", &judge_env_summary(loaded));
            ui::field("toolchain", &toolchain_summary(loaded));
        }
        Err(e) => {
            ui::field("config", "（見つかりません）");
            ui::field("", &format!("{e}"));
        }
    }

    let session_path = session::session_path()?;
    let saved = session::load_from(&session_path)?;
    match &saved {
        Some(_) => {
            let mode = session::mode_of(&session_path)
                .map(|m| format!(" ({m:04o})"))
                .unwrap_or_default();
            ui::field("session", &format!("{}{mode}", session_path.display()));
        }
        None => ui::field("session", &format!("{} （未保存）", session_path.display())),
    }

    ui::field("login", &login_summary(saved.as_ref(), offline)?);

    if let Ok(loaded) = &loaded {
        show_current_problem(loaded);
    }
    Ok(())
}

fn login_summary(saved: Option<&session::Session>, offline: bool) -> Result<String> {
    let Some(saved) = saved else {
        return Ok("未ログイン（`acrust login`）".to_owned());
    };
    if offline {
        let name = if saved.user_screen_name.is_empty() {
            "(不明)"
        } else {
            &saved.user_screen_name
        };
        return Ok(format!("{name} （セッション保存済み・未確認）"));
    }
    let atcoder = atcoder_config();
    let client = AtCoderClient::new(&atcoder)?;
    client.load_session()?;
    match auth::current_user(&client)? {
        Some(user) => Ok(user),
        None => Ok("セッションが無効です（`acrust login` をやり直してください）".to_owned()),
    }
}

/// パッケージの中で実行されたときは、いま何が対象になるかも見せる。
fn show_current_problem(loaded: &LoadedConfig) {
    let Ok(package) = Package::find() else { return };
    ui::info("");
    ui::field(
        "package",
        &format!("{} ({})", package.name, package.dir.display()),
    );

    let template = std::fs::read_to_string(loaded.template_src()).ok();
    match resolve_problem(
        &package,
        None,
        loaded.config.test.resolve,
        template.as_deref(),
    ) {
        Ok(resolved) => {
            let suffix = if resolved.origin == Origin::Inferred {
                " [推定]"
            } else {
                ""
            };
            ui::field(
                "problem",
                &format!("{}{suffix}", resolved.describe(&package)),
            );
        }
        Err(e) => ui::field("problem", &format!("{e}")),
    }
}

/// 言語アップデートの版（例 `2025-10`）と、テンプレートの状態。
fn judge_env_summary(loaded: &LoadedConfig) -> String {
    let version = language_update_version(&loaded.config.atcoder.language_list)
        .unwrap_or_else(|| "(不明)".to_owned());
    let deps = loaded.template_dependencies();
    let crates = std::fs::read_to_string(&deps)
        .ok()
        .and_then(|text| toml::from_str::<toml::Table>(&text).ok())
        .map(|table| format!("{} crates", table.len()))
        .unwrap_or_else(|| "dependencies.toml なし".to_owned());
    let lock = if loaded.template_cargo_lock().is_file() {
        "Cargo.lock あり"
    } else {
        "Cargo.lock なし（`acrust env update`）"
    };
    format!(
        "{version} / edition {} / {crates} / {lock}",
        loaded.config.package.edition
    )
}

fn toolchain_summary(loaded: &LoadedConfig) -> String {
    let pinned = std::fs::read_to_string(loaded.rust_toolchain_path())
        .ok()
        .and_then(|text| toml::from_str::<toml::Table>(&text).ok())
        .and_then(|table| {
            table
                .get("toolchain")?
                .get("channel")?
                .as_str()
                .map(str::to_owned)
        });
    let local = local_rustc_version().unwrap_or_else(|| "(不明)".to_owned());
    match pinned {
        Some(pinned) if pinned == local => format!("{pinned}（rust-toolchain.toml・一致）"),
        Some(pinned) => format!("{pinned}（rust-toolchain.toml） / 実行中の rustc は {local}"),
        None => format!(
            "固定なし / 実行中の rustc は {local}（`acrust init` で rust-toolchain.toml を作れます）"
        ),
    }
}

/// `https://.../language-update/2025-10/...` から `2025-10` を取り出す。
fn language_update_version(url: &str) -> Option<String> {
    static RE: OnceLock<regex::Regex> = OnceLock::new();
    let re = RE.get_or_init(|| {
        regex::Regex::new(r"/language-update/(\d{4}-\d{2})/").expect("valid regex")
    });
    Some(re.captures(url)?[1].to_owned())
}

fn local_rustc_version() -> Option<String> {
    let output = std::process::Command::new("rustc")
        .arg("-V")
        .output()
        .ok()?;
    let text = String::from_utf8(output.stdout).ok()?;
    text.split_whitespace().nth(1).map(str::to_owned)
}

/// `status` や `login` は設定が無くても動かしたいので、無ければ既定値を使う。
fn atcoder_config() -> AtcoderConfig {
    LoadedConfig::find()
        .map(|loaded| loaded.config.atcoder)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_language_update_version_from_the_url() {
        assert_eq!(
            language_update_version(
                "https://img.atcoder.jp/file/language-update/2025-10/language-list.html"
            )
            .as_deref(),
            Some("2025-10")
        );
        assert_eq!(language_update_version("https://example.com/"), None);
    }
}
