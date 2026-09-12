//! Integration tests for `acrust migrate`.
//!
//! A synthetic cargo-compete repository is built and migrated, to check both that
//! nothing is lost and that the round-trip check really does stop the migration.

use acrust::commands::migrate;
use acrust::testcases::{Matching, SuiteKind, TestSuite};
use acrust::workspace::Package;
use std::path::{Path, PathBuf};

struct Repo(PathBuf);

impl Drop for Repo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn write(path: &Path, contents: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, contents).unwrap();
}

/// The shape cargo-compete actually produces, copied from a real repository.
fn repo(name: &str) -> Repo {
    let root = std::env::temp_dir().join(format!(
        "acrust-migrate-{name}-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();

    write(
        &root.join("compete.toml"),
        r#"test-suite = "{{ manifest_dir }}/testcases/{{ bin_alias }}.yml"

[template]
src = '''
fn main() {
    println!("template");
}
'''

[template.new]
edition = "2021"
dependencies = '''
proconio = { version = "=0.4.5", features = ["derive"] }
itertools = "=0.11.0"
'''

[template.new.copy-files]
"./template-cargo-lock.toml" = "Cargo.lock"

[submit]
kind = "file"
language_id = "5054"
"#,
    );
    write(
        &root.join("template-cargo-lock.toml"),
        "# lock\nversion = 3\n",
    );

    // An ABC held alongside an ARC: C and D point at the other contest.
    write(
        &root.join("abc042/Cargo.toml"),
        r#"[package]
name = "abc042"
version = "0.1.0"
edition = "2021"

[package.metadata.cargo-compete.bin]
abc042-a = { alias = "a", problem = "https://atcoder.jp/contests/abc042/tasks/abc042_a" }
abc042-c = { alias = "c", problem = "https://atcoder.jp/contests/abc042/tasks/arc058_a" }

[[bin]]
name = "abc042-a"
path = "src/bin/a.rs"

[[bin]]
name = "abc042-c"
path = "src/bin/c.rs"

[dependencies]
# added by hand
proconio = "=0.4.5"
"#,
    );
    write(&root.join("abc042/src/bin/a.rs"), "fn main() {}\n");
    write(&root.join("abc042/src/bin/c.rs"), "fn main() {}\n");
    write(
        &root.join("abc042/testcases/a.yml"),
        "---\ntype: Batch\ntimelimit: 2s\nmatch: Lines\n\ncases:\n  - name: sample1\n    in: |\n      3\n      1 2 3\n    out: |\n      6\n\nextend:\n  - type: Text\n    path: \"./a\"\n    in: /in/*.txt\n    out: /out/*.txt\n",
    );
    // Float judging, shaped after a real problem that names no relative bound.
    write(
        &root.join("abc042/testcases/c.yml"),
        "---\ntype: Batch\ntimelimit: 2s 500ms\nmatch:\n  Float:\n    relative_error: ~\n    absolute_error: 1e-9\n\ncases:\n  - name: sample1\n    in: |\n      2\n    out: |\n      0.500000000\n",
    );

    // An interactive problem, which has no cases.
    write(
        &root.join("abc244/Cargo.toml"),
        r#"[package]
name = "abc244"
version = "0.1.0"
edition = "2021"

[package.metadata.cargo-compete.bin]
abc244-c = { alias = "c", problem = "https://atcoder.jp/contests/abc244/tasks/abc244_c" }

[[bin]]
name = "abc244-c"
path = "src/bin/c.rs"

[dependencies]
"#,
    );
    write(&root.join("abc244/src/bin/c.rs"), "fn main() {}\n");
    write(
        &root.join("abc244/testcases/c.yml"),
        "---\ntype: Interactive\ntimelimit: 2s\n",
    );

    Repo(root)
}

#[test]
fn a_dry_run_writes_nothing() {
    let repo = repo("dry");
    let root = &repo.0;
    let before: Vec<PathBuf> = walk(root);

    let summary = migrate::migrate_at(root, false).unwrap();
    assert_eq!(summary.packages, 2);
    assert_eq!(summary.bins, 3);
    assert_eq!(summary.files, 3);
    assert_eq!(summary.cases, 2);

    assert_eq!(walk(root), before, "a dry run wrote something");
    assert!(!root.join(".acrust").exists());
}

#[test]
fn migrating_keeps_every_piece_of_information() {
    let repo = repo("write");
    let root = &repo.0;
    migrate::migrate_at(root, true).unwrap();

    // The config and the templates have moved across.
    assert!(root.join(".acrust/config.toml").is_file());
    let template = std::fs::read_to_string(root.join(".acrust/template/main.rs")).unwrap();
    assert!(template.contains("println!(\"template\")"), "{template}");
    let dependencies =
        std::fs::read_to_string(root.join(".acrust/template/dependencies.toml")).unwrap();
    assert!(
        dependencies.contains("=0.4.5"),
        "existing dependencies carry over"
    );
    assert_eq!(
        std::fs::read_to_string(root.join(".acrust/template/Cargo.lock")).unwrap(),
        "# lock\nversion = 3\n"
    );

    // The edition comes from compete.toml, to match the packages already there.
    let config = acrust::config::LoadedConfig::load(root).unwrap();
    assert_eq!(config.config.package.edition, "2021");
    // The language id is not carried over; acrust reads it off the submit page.
    assert!(config.config.submit.language_id.is_empty());

    // Nothing of cargo-compete is left behind.
    assert!(!root.join("compete.toml").exists());
    assert!(!root.join("template-cargo-lock.toml").exists());
    assert!(walk(root)
        .iter()
        .all(|p| p.extension().and_then(|e| e.to_str()) != Some("yml")));

    // The problem URL that cannot be derived survived the move.
    let package = Package::load(&root.join("abc042/Cargo.toml")).unwrap();
    assert_eq!(package.contest, "abc042");
    assert_eq!(
        package.task_url("c").unwrap(),
        "https://atcoder.jp/contests/abc042/tasks/arc058_a"
    );
    assert_eq!(
        package.task_url("a").unwrap(),
        "https://atcoder.jp/contests/abc042/tasks/abc042_a"
    );

    // The hand-added dependency and its comment are still there.
    let manifest = std::fs::read_to_string(root.join("abc042/Cargo.toml")).unwrap();
    assert!(manifest.contains("# added by hand"), "{manifest}");
    assert!(!manifest.contains("cargo-compete"), "{manifest}");

    // The cases themselves came through unchanged.
    let a = TestSuite::load(&root.join("abc042/testcases/a.toml")).unwrap();
    assert_eq!(a.kind, SuiteKind::Batch);
    assert_eq!(a.timelimit.as_deref(), Some("2s"));
    assert_eq!(a.cases[0].input, "3\n1 2 3\n");
    assert_eq!(a.cases[0].output, "6\n");

    let c = TestSuite::load(&root.join("abc042/testcases/c.toml")).unwrap();
    assert_eq!(c.matching, Matching::Float);
    assert_eq!(c.timelimit.as_deref(), Some("2500ms"));
    let float = c.float.unwrap();
    assert_eq!(float.absolute_error, Some(1e-9));
    assert_eq!(float.relative_error, None, "a bound not named stays unset");

    let interactive = TestSuite::load(&root.join("abc244/testcases/c.toml")).unwrap();
    assert_eq!(interactive.kind, SuiteKind::Interactive);
    assert!(interactive.cases.is_empty());
}

#[test]
fn migrating_twice_is_refused_rather_than_doing_half_the_work() {
    let repo = repo("twice");
    let root = &repo.0;
    migrate::migrate_at(root, true).unwrap();
    // compete.toml is gone, so the second run stops at the front door.
    assert!(migrate::migrate_at(root, true).is_err());
}

#[test]
fn an_unreadable_testcase_stops_everything_before_writing() {
    let repo = repo("broken");
    let root = &repo.0;
    // Slip in a line snowchains would never write.
    write(
        &root.join("abc042/testcases/a.yml"),
        "---\ntype: Batch\ntimelimit: 2s\nmatch: Lines\nsurprise: yes\n",
    );

    let err = migrate::migrate_at(root, true).unwrap_err().to_string();
    assert!(
        err.contains("cannot make sense of") || err.contains("could not read"),
        "{err}"
    );

    // One unreadable file means nothing at all is written.
    assert!(!root.join(".acrust").exists(), "wrote a half-migration");
    assert!(root.join("compete.toml").is_file());
    assert!(root.join("abc042/testcases/c.yml").is_file());
    let manifest = std::fs::read_to_string(root.join("abc042/Cargo.toml")).unwrap();
    assert!(
        manifest.contains("cargo-compete"),
        "the metadata was rewritten"
    );
}

#[test]
fn a_problem_url_that_cannot_be_rebuilt_stops_everything() {
    let repo = repo("badurl");
    let root = &repo.0;
    let manifest = std::fs::read_to_string(root.join("abc042/Cargo.toml")).unwrap();
    write(
        &root.join("abc042/Cargo.toml"),
        &manifest.replace(
            "https://atcoder.jp/contests/abc042/tasks/arc058_a",
            "https://codeforces.com/problemset/problem/1/A",
        ),
    );

    assert!(migrate::migrate_at(root, true).is_err());
    assert!(!root.join(".acrust").exists());
    assert!(root.join("abc042/testcases/a.yml").is_file());
}

fn walk(root: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.filter_map(Result::ok) {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else {
                found.push(path);
            }
        }
    }
    found.sort();
    found
}
