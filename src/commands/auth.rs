//! `acrust login` / `logout` / `status`。

use crate::atcoder::{auth, AtCoderClient};
use crate::browser;
use crate::config::{AtcoderConfig, LoadedConfig};
use crate::session::{self, Session};
use crate::ui::{self, Mark};
use crate::workspace::{resolve_problem, Origin, Package};
use anyhow::{Context as _, Result};
use std::io::IsTerminal as _;
use std::sync::OnceLock;

/// ブラウザで取得したセッションクッキーを取り込んで保存する。
///
/// AtCoder の `/login` は Cloudflare Turnstile で守られており、ID / パスワードの
/// POST はプログラムからは通らない（`atcoder::auth` のモジュールコメント参照）。
pub fn login(cookie: Option<String>, no_open: bool) -> Result<()> {
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
            // 貼る元のページを開いておく。DevTools を出すところは手でやってもらう。
            if !no_open && std::io::stdin().is_terminal() {
                ui::arrow(&format!("{} をブラウザで開きます", auth::LOGIN_URL));
                if let Err(e) = browser::open(auth::LOGIN_URL) {
                    ui::warn(&format!("ブラウザを開けませんでした: {e:#}"));
                }
            }
            print_instructions();
            read_secret("REVEL_SESSION: ")?
        }
    };
    let value = auth::extract_session_value(&pasted).context("セッションクッキーが空です")?;

    let user = auth::verify_session_cookie(&client, &value)?;
    let session = Session::new(value, user.clone());
    let path = session::save(&session)?;

    ui::ok(&format!("{user} としてログインしました"));
    ui::field("session", &format!("{}（600）", path.display()));
    Ok(())
}

/// 端末なら伏せ字で、パイプ越しなら普通に 1 行読む。
fn read_secret(label: &str) -> Result<String> {
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
    ui::info("ブラウザの Cookie から REVEL_SESSION の値を貼り付けてください。");
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

/// `status` の1行。ラベル幅を揃えるため、全部そろえてから描く。
struct Line {
    mark: Mark,
    label: &'static str,
    value: String,
}

/// 直すために打つコマンドと、その理由。
struct Todo {
    command: &'static str,
    reason: &'static str,
}

/// ログイン状態・設定の場所・ジャッジ環境のバージョンを表示する。
///
/// 並んだ値を読んで判断させないため、最後に「問題ないか」を1行で言い切る。
pub fn status(offline: bool) -> Result<()> {
    let mut lines: Vec<Line> = Vec::new();
    let mut todos: Vec<Todo> = Vec::new();

    let loaded = LoadedConfig::find();
    match &loaded {
        Ok(loaded) => {
            // 設定ファイルを探す手間を無くすため、絶対パスを必ず出す（決定 D2）。
            lines.push(Line {
                mark: Mark::Ok,
                label: "設定",
                value: loaded.path.display().to_string(),
            });
            lines.push(judge_env_line(loaded, &mut todos));
            lines.push(rustc_line(loaded));
        }
        Err(_) => {
            lines.push(Line {
                mark: Mark::Bad,
                label: "設定",
                value: ".acrust/config.toml が見つかりません".to_owned(),
            });
            todos.push(Todo {
                command: "acrust init",
                reason: "このディレクトリを acrust の管理下にする",
            });
        }
    }

    let session_path = session::session_path()?;
    let saved = session::load_from(&session_path)?;
    match &saved {
        Some(_) => {
            let mode = session::mode_of(&session_path)
                .map(|mode| format!("（{mode:o}）"))
                .unwrap_or_default();
            lines.push(Line {
                mark: Mark::Ok,
                label: "セッション",
                value: format!("{}{mode}", session_path.display()),
            });
        }
        None => lines.push(Line {
            mark: Mark::Bad,
            label: "セッション",
            value: format!("{}（未保存）", session_path.display()),
        }),
    }

    lines.push(login_line(saved.as_ref(), offline, &mut todos)?);
    if let Ok(loaded) = &loaded {
        lines.extend(package_lines(loaded));
    }

    ui::info(&format!("acrust {}", env!("CARGO_PKG_VERSION")));
    ui::info("");
    let width = lines
        .iter()
        .map(|line| ui::display_width(line.label))
        .max()
        .unwrap_or(0);
    for line in &lines {
        ui::row(line.mark, line.label, &line.value, width);
    }

    ui::info("");
    if todos.is_empty() {
        ui::summary(Mark::Ok, "異常なし");
        return Ok(());
    }
    ui::summary(Mark::Todo, &format!("次にやること（{} 件）", todos.len()));
    let width = todos
        .iter()
        .map(|todo| todo.command.len())
        .max()
        .unwrap_or(0);
    for todo in &todos {
        ui::info(&format!("    {:<width$}   {}", todo.command, todo.reason));
    }
    Ok(())
}

fn login_line(
    saved: Option<&session::Session>,
    offline: bool,
    todos: &mut Vec<Todo>,
) -> Result<Line> {
    const RELOGIN: Todo = Todo {
        command: "acrust login",
        reason: "AtCoder にログインする",
    };

    let Some(saved) = saved else {
        todos.push(RELOGIN);
        return Ok(Line {
            mark: Mark::Bad,
            label: "ログイン",
            value: "未ログイン".to_owned(),
        });
    };
    if offline {
        let name = if saved.user_screen_name.is_empty() {
            "(不明)"
        } else {
            &saved.user_screen_name
        };
        // --offline は本人が望んだ状態なので、対応の要る「!」ではなく事実の「·」。
        return Ok(Line {
            mark: Mark::Info,
            label: "ログイン",
            value: format!("{name}（AtCoder には確認していません）"),
        });
    }

    let client = AtCoderClient::new(&atcoder_config())?;
    client.load_session()?;
    match auth::current_user(&client)? {
        Some(user) => Ok(Line {
            mark: Mark::Ok,
            label: "ログイン",
            value: user,
        }),
        None => {
            todos.push(RELOGIN);
            Ok(Line {
                mark: Mark::Bad,
                label: "ログイン",
                value: "セッションが無効です".to_owned(),
            })
        }
    }
}

/// パッケージの中で実行されたときは、いま何が対象になるかも見せる。
fn package_lines(loaded: &LoadedConfig) -> Vec<Line> {
    let Ok(package) = Package::find() else {
        return Vec::new();
    };
    let mut lines = vec![Line {
        mark: Mark::Ok,
        label: "パッケージ",
        value: package.name.clone(),
    }];

    let template = std::fs::read_to_string(loaded.template_src()).ok();
    lines.push(
        match resolve_problem(
            &package,
            None,
            loaded.config.test.resolve,
            template.as_deref(),
        ) {
            Ok(resolved) => {
                let src = resolved
                    .bin
                    .src_path
                    .strip_prefix(&package.dir)
                    .unwrap_or(&resolved.bin.src_path);
                let suffix = if resolved.origin == Origin::Inferred {
                    "・推定"
                } else {
                    ""
                };
                Line {
                    mark: Mark::Ok,
                    label: "問題",
                    value: format!("{}（{}{suffix}）", resolved.bin.alias, src.display()),
                }
            }
            // まだ解答を書いていないだけなので、これも事実の報告にとどめる。
            Err(e) => Line {
                mark: Mark::Info,
                label: "問題",
                value: first_sentence(&format!("{e}")),
            },
        },
    );
    lines
}

/// 最初の句点までを返す。
///
/// 問題が決まらない理由の説明には「指定してください。候補: …」が続くが、
/// それは `acrust test` が言うことで、1 行に収めたい `status` では長いだけ。
fn first_sentence(message: &str) -> String {
    match message.split_once('。') {
        Some((head, _)) => head.to_owned(),
        None => message.to_owned(),
    }
}

/// 言語アップデートの版（例 `2025-10`）と、テンプレートの状態。
fn judge_env_line(loaded: &LoadedConfig, todos: &mut Vec<Todo>) -> Line {
    let version = language_update_version(&loaded.config.atcoder.language_list)
        .unwrap_or_else(|| "(不明)".to_owned());
    let crates = std::fs::read_to_string(loaded.template_dependencies())
        .ok()
        .and_then(|text| toml::from_str::<toml::Table>(&text).ok())
        .map(|table| table.len());
    let has_lock = loaded.template_cargo_lock().is_file();

    let (mark, reason) = match (crates, has_lock) {
        (Some(_), true) => (Mark::Ok, None),
        // Cargo.lock が無くてもビルドはできるが、ジャッジと同じ版で固まらない。
        (Some(_), false) => (Mark::Todo, Some("ジャッジと同じ Cargo.lock を取得する")),
        (None, _) => (
            Mark::Bad,
            Some("依存クレートのテンプレートを作り直す（いま無い状態です）"),
        ),
    };
    if let Some(reason) = reason {
        todos.push(Todo {
            command: "acrust env update",
            reason,
        });
    }

    let crates = match crates {
        Some(count) => format!("{count} クレート"),
        None => "dependencies.toml なし".to_owned(),
    };
    let lock = if has_lock {
        "Cargo.lock あり"
    } else {
        "Cargo.lock なし"
    };
    Line {
        mark,
        label: "ジャッジ環境",
        value: format!(
            "{version} / edition {} / {crates} / {lock}",
            loaded.config.package.edition
        ),
    }
}

fn rustc_line(loaded: &LoadedConfig) -> Line {
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
    // 固定した版と実際に動く版が食い違うと、手元では通って提出で初めて CE になる。
    let (mark, value) = match pinned {
        Some(pinned) if pinned == local => {
            (Mark::Ok, format!("{pinned}（rust-toolchain.toml と一致）"))
        }
        Some(pinned) => (
            Mark::Todo,
            format!("rust-toolchain.toml は {pinned} / 実行中は {local}"),
        ),
        None => (Mark::Todo, format!("固定なし / 実行中は {local}")),
    };
    Line {
        mark,
        label: "rustc",
        value,
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

    /// 「次にやること（N 件）」に並ぶ対応は、原因のある行だけから積まれる。
    #[test]
    fn a_missing_session_is_the_only_todo_when_everything_else_is_fine() {
        let mut todos = Vec::new();
        let line = login_line(None, false, &mut todos).unwrap();
        assert_eq!(line.mark, Mark::Bad);
        assert_eq!(line.value, "未ログイン");
        assert_eq!(todos.len(), 1);
        assert_eq!(todos[0].command, "acrust login");
    }

    #[test]
    fn only_the_first_sentence_of_a_long_reason_reaches_the_status_row() {
        assert_eq!(
            first_sentence(
                "どの問題を指すか決められません（生成直後で全てテンプレートのままです）。\
                 問題を指定してください。候補: a, b, c"
            ),
            "どの問題を指すか決められません（生成直後で全てテンプレートのままです）"
        );
        // 句点が無ければそのまま。
        assert_eq!(first_sentence("bin がありません"), "bin がありません");
    }

    /// `--offline` では AtCoder を叩かないので、確認できていないことを明示する。
    #[test]
    fn offline_reports_the_saved_name_without_asking_atcoder() {
        let saved = Session::new("cookie".to_owned(), "okaponta".to_owned());
        let mut todos = Vec::new();
        let line = login_line(Some(&saved), true, &mut todos).unwrap();
        assert_eq!(line.mark, Mark::Info);
        assert!(line.value.starts_with("okaponta"), "{}", line.value);
        assert!(todos.is_empty(), "確認していないだけで、壊れてはいない");
    }
}
