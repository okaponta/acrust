//! `acrust open` — ブラウザで問題を開く。

use crate::browser;
use crate::ui;
use crate::workspace::Package;
use anyhow::{Context as _, Result};

pub fn run(problem: Option<String>) -> Result<()> {
    let package = Package::find()?;
    let aliases: Vec<String> = match &problem {
        Some(query) => {
            let bin = package
                .find_bin(query)
                .with_context(|| format!("{} has no problem {query}", package.name))?;
            vec![bin.alias.clone()]
        }
        // 引数なしなら全問。
        None => package.bins.iter().map(|bin| bin.alias.clone()).collect(),
    };

    for alias in &aliases {
        let url = package.task_url(alias)?;
        ui::arrow(&format!("{alias}: {url}"));
        browser::open(&url)?;
    }
    Ok(())
}
