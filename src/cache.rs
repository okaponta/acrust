//! `~/.cache/acrust/` に置く小さなキャッシュ。
//!
//! いまのところ言語 ID だけ。提出のたびに提出ページを取りに行かなくて済むが、
//! 言語アップデートで ID は変わるので、提出が弾かれたら捨てて取り直す。

use crate::atcoder::submit::Language;
use crate::session;
use anyhow::{Context as _, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;

const FILE: &str = "language-ids.json";

#[derive(Debug, Default, Serialize, Deserialize)]
struct LanguageCache {
    /// language-pattern -> 選ばれた言語。
    #[serde(default)]
    languages: BTreeMap<String, Language>,
}

fn path() -> Result<PathBuf> {
    Ok(session::cache_dir()?.join(FILE))
}

pub fn language_for(pattern: &str) -> Option<Language> {
    let path = path().ok()?;
    let text = std::fs::read_to_string(path).ok()?;
    let cache: LanguageCache = serde_json::from_str(&text).ok()?;
    cache.languages.get(pattern).cloned()
}

pub fn remember_language(pattern: &str, language: &Language) -> Result<()> {
    let path = path()?;
    let mut cache: LanguageCache = std::fs::read_to_string(&path)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default();
    cache.languages.insert(pattern.to_owned(), language.clone());
    write(&path, &cache)
}

/// 提出が弾かれたときに呼ぶ。次回は取り直す。
pub fn forget_language(pattern: &str) -> Result<()> {
    let path = path()?;
    let Some(mut cache) = std::fs::read_to_string(&path)
        .ok()
        .and_then(|text| serde_json::from_str::<LanguageCache>(&text).ok())
    else {
        return Ok(());
    };
    if cache.languages.remove(pattern).is_none() {
        return Ok(());
    }
    write(&path, &cache)
}

fn write(path: &std::path::Path, cache: &LanguageCache) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("{} を作れませんでした", parent.display()))?;
    }
    let text = serde_json::to_string_pretty(cache)? + "\n";
    std::fs::write(path, text).with_context(|| format!("{} に書けませんでした", path.display()))
}
