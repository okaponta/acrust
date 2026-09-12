//! Package and problem resolution, against real `Cargo.toml` files.
//!
//! This pins down the shape `acrust new` generates. Nothing here touches the
//! network.

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

/// Sets each bin's mtime explicitly, as an offset in seconds from a fixed base.
fn set_mtimes(package_dir: &Path, offsets: &[(&str, u64)]) {
    let base = std::time::SystemTime::now() - std::time::Duration::from_secs(600);
    for (alias, offset) in offsets {
        let file = std::fs::OpenOptions::new()
            .write(true)
            .open(package_dir.join(format!("src/bin/{alias}.rs")))
            .unwrap();
        file.set_modified(base + std::time::Duration::from_secs(*offset))
            .unwrap();
    }
}

/// What `acrust new abc042` produces. A real contest, whose C and D point at the
/// ARC held alongside it.
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

    // C of abc042 is arc058_a, and only the metadata can say so.
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
    assert!(err.contains("still the template"), "{err}");
    assert!(err.contains("a, b, c, d"), "{err}");

    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn the_most_recently_edited_bin_wins() {
    let root = scratch("mtime");
    let package_dir = root.join("abc042");
    write_package(&package_dir);

    // Edit c.rs, then set the mtimes by hand: on some machines all four writes
    // land in the same timestamp, and the implied order cannot be relied on.
    std::fs::write(
        package_dir.join("src/bin/c.rs"),
        "fn main() { /* solved */ }\n",
    )
    .unwrap();
    set_mtimes(&package_dir, &[("a", 0), ("b", 0), ("c", 60), ("d", 0)]);

    let package = Package::load(&package_dir.join("Cargo.toml")).unwrap();
    let resolved = resolve_problem(&package, None, ResolveMode::Mtime, Some(TEMPLATE)).unwrap();
    assert_eq!(resolved.bin.alias, "c");
    assert_eq!(resolved.origin, Origin::Inferred);
    assert_eq!(
        resolved.describe(&package),
        "abc042 c (src/bin/c.rs)",
        "an inferred problem has to be printable"
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

    // An unmigrated package fails with instructions rather than half-working.
    let err = Package::load(&package_dir.join("Cargo.toml"))
        .unwrap_err()
        .to_string();
    assert!(err.contains("acrust migrate"), "{err}");

    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn a_tie_on_the_newest_mtime_is_ambiguous_rather_than_arbitrary() {
    let root = scratch("tie");
    let package_dir = root.join("abc042");
    write_package(&package_dir);

    // Both c and d were edited, and saved within the same timestamp.
    for alias in ["c", "d"] {
        std::fs::write(
            package_dir.join(format!("src/bin/{alias}.rs")),
            "fn main() { /* solved */ }\n",
        )
        .unwrap();
    }
    set_mtimes(&package_dir, &[("a", 0), ("b", 0), ("c", 60), ("d", 60)]);

    let package = Package::load(&package_dir.join("Cargo.toml")).unwrap();
    let err = resolve_problem(&package, None, ResolveMode::Mtime, Some(TEMPLATE))
        .unwrap_err()
        .to_string();
    assert!(err.contains("share an mtime"), "{err}");
    assert!(err.contains("c, d"), "{err}");

    std::fs::remove_dir_all(&root).unwrap();
}
