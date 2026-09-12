//! `acrust copy` — 解答をクリップボードに入れる。
//!
//! AtCoder はコンテスト終了後の提出フォームを Cloudflare Turnstile で守っているので、
//! 終了後の練習提出は `submit` では通らない（`commands::submit` 参照）。
//! そのときにブラウザへ貼るための道具。
//!
//! 貼るのは `src/bin/{alias}.rs` そのもので、加工しない。`submit` が送るものと
//! 同じにしておかないと「提出したもの = リポジトリの中身」が崩れる（決定 D13）。

use crate::clipboard;
use crate::config::LoadedConfig;
use crate::ui;
use crate::workspace::{self, Package};
use anyhow::{bail, Context as _, Result};

pub fn run(problem: Option<String>) -> Result<()> {
    let config = LoadedConfig::find()?;
    let package = Package::find()?;
    let resolved = workspace::resolve_problem(
        &package,
        problem.as_deref(),
        config.config.test.resolve,
        std::fs::read_to_string(config.template_src())
            .ok()
            .as_deref(),
    )?;
    // 何をクリップボードに載せたかは必ず見せる。貼る直前に取り違えると気づけない。
    ui::arrow(&resolved.describe(&package));

    let source = std::fs::read_to_string(&resolved.bin.src_path)
        .with_context(|| format!("{} を読めませんでした", resolved.bin.src_path.display()))?;
    if source.trim().is_empty() {
        bail!("{} が空です", resolved.bin.src_path.display());
    }

    let via = clipboard::copy(&source)?;
    ui::ok(&format!(
        "コピーしました（{} 行 / {} バイト・{via}）",
        source.lines().count(),
        source.len()
    ));

    let task = package.task_url(&resolved.bin.alias)?;
    ui::info("");
    ui::info("貼り付けて提出してください:");
    ui::info(&format!("  {task}"));
    ui::info(&format!(
        "  ブラウザで開くなら `acrust open {}`",
        resolved.bin.alias
    ));
    Ok(())
}
