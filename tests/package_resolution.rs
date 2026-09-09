//! 実際の `Cargo.toml` に対するパッケージ・問題解決の検証（設計 §4.6 / §4.8）。
//!
//! `acrust new`（M2）が生成する形をここで先に固定しておく。ネットワークは使わない。

use acrust::config::ResolveMode;
use acrust::workspace::{resolve_problem, Origin, Package};
use std::path::{Path, PathBuf};

const TEMPLATE: &str = "fn main() {}\n";

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "acrust-it-{name}-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// `acrust new abc042` が作る想定の形。C・D が ARC 側を指す実在のケースを使う。
fn write_package(dir: &Path) {
    std::fs::create_dir_all(dir.join("src/bin")).unwrap();
    std::fs::write(
        dir.join("Cargo.toml"),
        r#"[package]
name = "abc042"
version = "0.1.0"
edition = "2024"

[package.metadata.acrust]
contest = "abc042"

[package.metadata.acrust.tasks]
a = "abc042_a"
b = "abc042_b"
c = "arc058_a"
d = "arc058_b"

[[bin]]
name = "abc042-a"
path = "src/bin/a.rs"

[[bin]]
name = "abc042-b"
path = "src/bin/b.rs"

[[bin]]
name = "abc042-c"
path = "src/bin/c.rs"

[[bin]]
name = "abc042-d"
path = "src/bin/d.rs"

[dependencies]
"#,
    )
    .unwrap();
    for alias in ["a", "b", "c", "d"] {
        std::fs::write(dir.join(format!("src/bin/{alias}.rs")), TEMPLATE).unwrap();
    }
}

#[test]
fn reads_bins_and_task_screen_names_from_the_manifest() {
    let root = scratch("read");
    let package_dir = root.join("abc042");
    write_package(&package_dir);

    let package = Package::load(&package_dir.join("Cargo.toml")).unwrap();
    assert_eq!(package.name, "abc042");
    assert_eq!(package.contest, "abc042");
    assert_eq!(
        package
            .bins
            .iter()
            .map(|b| b.alias.as_str())
            .collect::<Vec<_>>(),
        ["a", "b", "c", "d"]
    );
    assert_eq!(package.bins[2].name, "abc042-c");

    // 導出できない対応（abc042 の C = arc058_a）がメタデータから引けること。
    assert_eq!(
        package.task_url("c").unwrap(),
        "https://atcoder.jp/contests/abc042/tasks/arc058_a"
    );
    assert_eq!(
        package.task_url("a").unwrap(),
        "https://atcoder.jp/contests/abc042/tasks/abc042_a"
    );

    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn finds_the_package_from_a_nested_directory() {
    let root = scratch("nested");
    let package_dir = root.join("abc042");
    write_package(&package_dir);

    let package = Package::find_from(&package_dir.join("src").join("bin")).unwrap();
    assert_eq!(package.name, "abc042");

    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn a_freshly_generated_package_refuses_to_guess() {
    let root = scratch("fresh");
    let package_dir = root.join("abc042");
    write_package(&package_dir);

    let package = Package::load(&package_dir.join("Cargo.toml")).unwrap();
    let err = resolve_problem(&package, None, ResolveMode::Mtime, Some(TEMPLATE))
        .unwrap_err()
        .to_string();
    assert!(err.contains("テンプレートのまま"), "{err}");
    assert!(err.contains("a, b, c, d"), "{err}");

    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn the_most_recently_edited_bin_wins() {
    let root = scratch("mtime");
    let package_dir = root.join("abc042");
    write_package(&package_dir);

    // c.rs だけ書き換える。
    std::fs::write(
        package_dir.join("src/bin/c.rs"),
        "fn main() { /* solved */ }\n",
    )
    .unwrap();

    let package = Package::load(&package_dir.join("Cargo.toml")).unwrap();
    let resolved = resolve_problem(&package, None, ResolveMode::Mtime, Some(TEMPLATE)).unwrap();
    assert_eq!(resolved.bin.alias, "c");
    assert_eq!(resolved.origin, Origin::Inferred);
    assert_eq!(
        resolved.describe(&package),
        "abc042 c (src/bin/c.rs)",
        "推定したら対象を必ず表示できること"
    );

    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn a_cargo_compete_package_is_told_to_migrate() {
    let root = scratch("compete");
    let package_dir = root.join("abc418");
    std::fs::create_dir_all(package_dir.join("src/bin")).unwrap();
    std::fs::write(
        package_dir.join("Cargo.toml"),
        r#"[package]
name = "abc418"
version = "0.1.0"
edition = "2021"

[package.metadata.cargo-compete.bin]
abc418-a = { alias = "a", problem = "https://atcoder.jp/contests/abc418/tasks/abc418_a" }

[[bin]]
name = "abc418-a"
path = "src/bin/a.rs"

[dependencies]
"#,
    )
    .unwrap();
    std::fs::write(package_dir.join("src/bin/a.rs"), TEMPLATE).unwrap();

    // 未移行のパッケージは黙って動かさず、移行を案内して落とす（決定 D4）。
    let err = Package::load(&package_dir.join("Cargo.toml"))
        .unwrap_err()
        .to_string();
    assert!(err.contains("acrust migrate"), "{err}");

    std::fs::remove_dir_all(&root).unwrap();
}
