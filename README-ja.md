# acrust

[English](README.md) | 日本語

**AtCoder × Rust** の競技プログラミング支援ツール。

```console
$ cargo install acrust   # ビルド済みバイナリなら cargo binstall acrust
$ acrust init            # リポジトリを acrust の管理下に置く
$ acrust env update      # AtCoder のジャッジ環境に合わせる
$ acrust login           # ブラウザのセッションクッキーを貼り付ける
$ acrust new abc474
$ acrust test a
$ acrust submit a
```

## コマンド

| コマンド | 何をするか |
|---|---|
| `acrust init` | `.acrust/` と `rust-toolchain.toml` を生成する |
| `acrust login` / `logout` | AtCoder のセッションを取得 / 破棄する |
| `acrust status` | 設定の場所・ジャッジ環境・ログイン状態と、やるべきことを表示する |
| `acrust new <contest>` | パッケージを作りサンプルを取得する |
| `acrust fetch [contest]` | サンプルを取得し直す |
| `acrust test [problem]` | ビルドしてサンプルテストを走らせる |
| `acrust run [problem]` | 標準入力を素通しして実行する |
| `acrust submit [problem]` | テストしてから提出し、結果を追う |
| `acrust copy [problem]` | 解答をクリップボードに入れる |
| `acrust open [problem]` | 問題をブラウザで開く |
| `acrust env update` | クレート・`Cargo.lock`・`edition`・rustc をジャッジ環境から再生成する |
| `acrust migrate` | cargo-compete のリポジトリを acrust の形に変換する |

`test` / `run` / `submit` / `copy` は問題の省略が可能で、最後に編集した問題を対象に実行します
（`submit` の場合は操作が取り消せないため確認します）。

## ログイン

ブラウザが既に持っているセッションクッキーを取り込みます。

ブラウザでログイン後、DevTools → Application（Safari では Storage）→ Cookies →
`https://atcoder.jp` を開き、`REVEL_SESSION` の Value を貼り付けてください。

## 配置

```
atcoder-rust/
├── .acrust/
│   ├── config.toml
│   └── template/
│       ├── main.rs
│       ├── dependencies.toml
│       ├── Cargo.lock
│       └── copy/
├── rust-toolchain.toml           # ジャッジと同じ rustc に固定
└── abc474/
    ├── Cargo.toml
    ├── src/bin/{a..g}.rs
    └── testcases/{a..g}.toml
```

`.acrust/` の下は好きに書き換えて構いません。

| | |
|---|---|
| `template/main.rs` | 新しい問題の `src/bin/*.rs` になる |
| `template/copy/` | ここに置いたものがパッケージ直下にそのままコピーされる。既定では存在しないので、`.vscode/launch.json` や `.envrc`、`justfile` などを置きたい場合は自分で作る |
| `template/dependencies.toml` | パッケージに入る依存。`env update` が再生成するが、書き換える前に差分を見せる |
| `template/Cargo.lock` | `env update` が取得する、ジャッジと同じバージョン |
| `config.toml` | パッケージの配置先、`test` のプロファイルと並列数、タイムアウト、問題の推定可否などの設定 |

資格情報はリポジトリの外に置きます。セッションは `~/.local/share/acrust/session.json`
（パーミッション 0600、`ACRUST_SESSION_FILE` で差し替え可）、キャッシュは `~/.cache/acrust/` です。

## 提出

```console
$ acrust submit c
  → abc474 c (src/bin/c.rs)
✓ 2/2 AC
  language:      Rust (rustc 1.89.0) (id=6088)
✓ submitted  WJ  https://atcoder.jp/contests/abc474/submissions/12345678
  AC   5s           312 ms / 4.2 MB
```

提出前にサンプルテストが走り（`-f` で飛ばせます）、提出後は結果を追います（`--no-watch` で
止められます）。送るのは常に `src/bin/{alias}.rs` そのものです。

> **コンテストが終了した後は、コマンドから提出できません。** そのときは `acrust copy` を
> 使ってください。

`acrust copy` は解答を加工せずそのままクリップボードに入れ、貼り付け先を表示します。

```console
$ acrust copy c
✓ copied (5 lines / 86 bytes, via pbcopy)

Paste it here to submit:
  https://atcoder.jp/contests/abc418/tasks/abc418_c
```

## cargo-compete からの移行

`acrust migrate` は一度きりの変換です。`--write` を付けるまでは dry-run で、git の working tree が
綺麗であることを求めます（`--allow-dirty` で回避）。変換前に往復検証を行い、1 件でも合わなければ
何も書かずに止まります。

## 開発

```console
$ cargo fmt --all --check
$ cargo clippy --all-targets --all-features -- -D warnings
$ cargo test
```

リポジトリの外に依存するテストは `live` feature の後ろに置いてあり、普段の `cargo test` では
ビルドもされません。

## ライセンス

`MIT OR Apache-2.0`（[LICENSE-MIT](LICENSE-MIT) / [LICENSE-APACHE](LICENSE-APACHE)）
