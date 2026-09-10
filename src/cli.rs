//! コマンド体系（設計 §4.1）。エントリポイントは `acrust` の1本のみ（決定 D1）。

use crate::commands;
use anyhow::{bail, Result};
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use std::process::ExitCode;

#[derive(Debug, Parser)]
#[command(
    name = "acrust",
    version,
    about = "AtCoder × Rust 専用の競技プログラミング支援ツール",
    long_about = "AtCoder × Rust 専用の競技プログラミング支援ツール。\n\
                  ジャッジ環境の依存クレートと rustc に追従し、\n\
                  サンプルの取得・テスト・提出までを1本のバイナリで行う。"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// AtCoder にログインする（ブラウザのセッションクッキーを取り込む）
    Login {
        /// REVEL_SESSION の値。省略時は対話入力（伏せ字）
        #[arg(long, value_name = "REVEL_SESSION")]
        cookie: Option<String>,
    },
    /// 保存済みのセッションを破棄する
    Logout,
    /// ログイン状態・設定の場所・ジャッジ環境のバージョンを表示する
    Status {
        /// AtCoder に問い合わせずローカルの情報だけを表示する
        #[arg(long)]
        offline: bool,
    },
    /// カレントのリポジトリに .acrust/ と rust-toolchain.toml を生成する
    Init {
        /// 対象ディレクトリ（既定: カレントディレクトリ）
        #[arg(long, value_name = "DIR")]
        path: Option<PathBuf>,
        /// 既存のファイルを上書きする
        #[arg(long)]
        force: bool,
    },
    /// cargo-compete 形式のリポジトリを acrust 形式へ移行する（往復検証つき）
    Migrate {
        /// 実際に書き込む（既定は差分レポートのみ）
        #[arg(long)]
        write: bool,
        /// git の working tree が汚れていても実行する
        #[arg(long)]
        allow_dirty: bool,
    },
    /// コンテストのパッケージを作り、サンプルを取得する
    New {
        /// コンテスト ID（例: abc474）
        contest: String,
    },
    /// サンプルを取得し直す
    Fetch {
        /// コンテスト ID（省略時はカレントのパッケージ）
        contest: Option<String>,
        /// 手で足したケースや手で直した match を残さず、取得した内容で置き換える
        #[arg(long)]
        overwrite: bool,
    },
    /// ビルドしてサンプルテストを実行する
    Test {
        /// 問題（例: a）。省略時は mtime が最新のものを推定する
        problem: Option<String>,
        /// release プロファイルでビルドする
        #[arg(long)]
        release: bool,
    },
    /// 標準入力を素通しして実行する
    Run {
        /// 問題（例: a）。省略時は mtime が最新のものを推定する
        problem: Option<String>,
    },
    /// テストしてから提出し、結果を追跡する
    Submit {
        /// 問題（例: a）。省略時は推定し、y/N の確認を入れる
        problem: Option<String>,
        /// 提出前のテストをスキップする
        #[arg(short, long)]
        force: bool,
        /// 提出後の結果追跡をしない
        #[arg(long)]
        no_watch: bool,
    },
    /// ブラウザで問題を開く
    Open {
        /// 問題（例: a）。省略時は全問
        problem: Option<String>,
    },
    /// AtCoder のジャッジ環境に追従する
    Env {
        #[command(subcommand)]
        command: EnvCommand,
    },
}

#[derive(Debug, Subcommand)]
enum EnvCommand {
    /// 依存クレート・Cargo.lock・edition・rustc バージョンを取得して更新する
    Update,
}

pub fn run() -> Result<ExitCode> {
    let cli = Cli::parse();
    match cli.command {
        Command::Login { cookie } => commands::auth::login(cookie)?,
        Command::Logout => commands::auth::logout()?,
        Command::Status { offline } => commands::auth::status(offline)?,
        Command::Init { path, force } => commands::init::run(path, force)?,
        Command::Migrate { .. } => unimplemented("acrust migrate", "M5")?,
        Command::New { contest } => commands::contest::new(&contest)?,
        Command::Fetch { contest, overwrite } => commands::contest::fetch(contest, overwrite)?,
        Command::Test { problem, release } => return commands::test::run(problem, release),
        Command::Run { .. } => unimplemented("acrust run", "M3")?,
        Command::Submit { .. } => unimplemented("acrust submit", "M4")?,
        Command::Open { .. } => unimplemented("acrust open", "M5")?,
        Command::Env { command } => match command {
            EnvCommand::Update => unimplemented("acrust env update", "M5")?,
        },
    }
    Ok(ExitCode::SUCCESS)
}

/// 未実装のコマンドは黙って何もせず終わるのではなく、はっきり落とす。
fn unimplemented(command: &str, milestone: &str) -> Result<()> {
    bail!("`{command}` はまだ実装されていません（{milestone} で実装予定）");
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory as _;

    #[test]
    fn the_cli_definition_is_valid() {
        Cli::command().debug_assert();
    }

    #[test]
    fn submit_takes_the_documented_flags() {
        let cli = Cli::try_parse_from(["acrust", "submit", "c", "-f", "--no-watch"]).unwrap();
        match cli.command {
            Command::Submit {
                problem,
                force,
                no_watch,
            } => {
                assert_eq!(problem.as_deref(), Some("c"));
                assert!(force);
                assert!(no_watch);
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn the_problem_argument_is_optional_everywhere_it_is_inferred() {
        for command in ["test", "run", "submit", "open"] {
            Cli::try_parse_from(["acrust", command]).unwrap_or_else(|e| panic!("{command}: {e}"));
        }
    }

    #[test]
    fn env_update_is_a_nested_subcommand() {
        let cli = Cli::try_parse_from(["acrust", "env", "update"]).unwrap();
        assert!(matches!(
            cli.command,
            Command::Env {
                command: EnvCommand::Update
            }
        ));
    }
}
