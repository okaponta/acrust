//! `acrust open` — ブラウザで問題を開く。

use crate::ui;
use crate::workspace::Package;
use anyhow::{Context as _, Result};

pub fn run(problem: Option<String>) -> Result<()> {
    let package = Package::find()?;
    let aliases: Vec<String> = match &problem {
        Some(query) => {
            let bin = package
                .find_bin(query)
                .with_context(|| format!("問題 {query} が {} にありません", package.name))?;
            vec![bin.alias.clone()]
        }
        // 引数なしなら全問。
        None => package.bins.iter().map(|bin| bin.alias.clone()).collect(),
    };

    for alias in &aliases {
        let url = package.task_url(alias)?;
        ui::arrow(&format!("{alias}: {url}"));
        open_in_browser(&url)?;
    }
    Ok(())
}

fn open_in_browser(url: &str) -> Result<()> {
    let command = if cfg!(target_os = "macos") {
        "open"
    } else if cfg!(target_os = "windows") {
        "start"
    } else {
        "xdg-open"
    };
    let status = std::process::Command::new(command)
        .arg(url)
        .status()
        .with_context(|| format!("{command} を起動できませんでした"))?;
    if !status.success() {
        anyhow::bail!("{command} が {status} で終了しました");
    }
    Ok(())
}
