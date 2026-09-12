//! acrust itself; the binary does nothing but call `cli::run`.
//!
//! It is a library so that `tests/` can drive things like package resolution
//! against real `Cargo.toml` files.

pub mod atcoder;
pub mod browser;
pub mod cache;
pub mod cli;
pub mod clipboard;
pub mod commands;
pub mod config;
pub mod judge;
pub mod manifest;
pub mod package;
pub mod runner;
pub mod session;
pub mod snowchains;
pub mod testcases;
pub mod ui;
pub mod workspace;
