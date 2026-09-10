//! acrust の実体。バイナリ `acrust` はこのクレートの `cli::run` を呼ぶだけ。
//!
//! ライブラリとして切り出しているのは、`tests/` からパッケージ解決などを
//! 実際の `Cargo.toml` に対して検証できるようにするため。

pub mod atcoder;
pub mod cache;
pub mod cli;
pub mod commands;
pub mod config;
pub mod judge;
pub mod manifest;
pub mod package;
pub mod runner;
pub mod session;
pub mod testcases;
pub mod ui;
pub mod workspace;
