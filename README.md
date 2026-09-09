# acrust

**AtCoder × Rust 専用**の競技プログラミング支援ツール。[cargo-compete](https://github.com/qryxip/cargo-compete) の後継として、AtCoder と Rust だけに割り切って作り直したもの。

```console
$ cargo install acrust
$ acrust login
$ acrust new abc474
$ acrust test a
$ acrust submit a
```

> **状態: 開発中（v0.1 未リリース）**
> 現在動くのは `init` / `login` / `logout` / `status` です。実装状況は下の[ロードマップ](#ロードマップ)を参照してください。

## なぜ作るか

`cargo-compete` は 2024-06 を最後に更新が止まっており、AtCoder のジャッジ環境が更新されるたびに設定を手で追いかける必要があります。acrust はそこを自動化します。

### 1. ジャッジ環境に自動追従する（最大の差別化）

AtCoder は言語アップデートごとに、ジャッジが実際に使う `Cargo.toml` をそのまま含んだインストールスクリプトを公開しています。`acrust env update` はこれを読んで

- `[dependencies]`（68 クレート、バージョン完全一致）
- ジャッジと同じ `Cargo.lock`
- `edition`
- **rustc のバージョン**

を再生成します。`cargo-compete` ではこれらを手で更新する必要があり、実際に「設定は 2023 年の環境のまま」という状態が起きます。

### 2. ツールチェインもジャッジに固定する

`acrust init` はリポジトリ直下に `rust-toolchain.toml` を生成し、ジャッジと同じ rustc に固定します。これが無いと `rustup update` を打った瞬間に手元だけ新しい rustc になり、**手元では通りテストも全 AC、提出して初めて CE** という最悪の形で差が出ます。

### 3. `language_id` をハードコードしない

提出ページの `<select name="data.LanguageId">` から `Rust (rustc ...)` を自動で選びます。言語アップデートで ID が変わっても壊れません。

### 4. AtCoder に優しい

- 1 コンテスト分のサンプルを **2 リクエスト**で取得（`/tasks` と `/tasks_print`）
- リクエスト間隔は最低 1 秒、429 / 5xx は指数バックオフ（`Retry-After` に従う）
- User-Agent で素性を明示

### 5. セッションを 0600 で保存する

AtCoder のセッションクッキーは有効期限 1 年半、持っていれば本人として提出できるパスワード同等の資格情報です。acrust は `~/.local/share/acrust/session.json` に **0600 で**保存し、パスワードは保存しません。パーミッションが緩いファイルを見つけたら警告して直します。

## コマンド

| コマンド | 説明 |
|---|---|
| `acrust init` | カレントのリポジトリに `.acrust/` と `rust-toolchain.toml` を生成する |
| `acrust login` / `logout` | AtCoder のセッションを取得・破棄する |
| `acrust status` | 設定の場所・ジャッジ環境・ログイン状態・いま対象になる問題を表示する |
| `acrust new <contest>` | パッケージを生成し、サンプルを取得する |
| `acrust fetch [contest]` | サンプルを取得し直す |
| `acrust test [problem]` | ビルドしてサンプルテストを実行する |
| `acrust run [problem]` | 標準入力を素通しして実行する |
| `acrust submit [problem]` | テストしてから提出し、結果を追跡する |
| `acrust open [problem]` | ブラウザで問題を開く |
| `acrust env update` | ジャッジ環境から依存・`Cargo.lock`・`edition`・rustc を再生成する |
| `acrust migrate` | cargo-compete 形式のリポジトリを移行する（往復検証つき・dry-run 既定） |

`test` / `run` は問題を省略すると `src/bin/*.rs` のうち **mtime が最新のもの**を対象にし、選んだ問題を必ず表示します。`submit` は省略時のみ y/N の確認が入ります（誤提出はペナルティが付いて取り消せないため）。

## ディレクトリ構成

```
atcoder-rust/
├── .acrust/
│   ├── config.toml
│   └── template/
│       ├── main.rs
│       ├── dependencies.toml     # env update が生成
│       ├── Cargo.lock            # env update が取得
│       └── copy/                 # new 時にパッケージ直下へコピーされる
├── .cargo/config.toml            # target-dir = "target"
├── rust-toolchain.toml           # ジャッジの rustc に固定
└── abc474/
    ├── Cargo.toml
    ├── src/bin/{a..g}.rs
    └── testcases/{a..g}.toml
```

認証情報はリポジトリの外に置きます。

```
~/.local/share/acrust/session.json    # 0600。ACRUST_SESSION_FILE で上書き可
~/.cache/acrust/                      # キャッシュ
```

## cargo-compete からの移行

`acrust migrate` が一度きりの変換を行います。dry-run が既定で、`--write` を付けて初めて書き込みます。

変換前に **往復検証**を行い、1 件でも不一致があれば何も書かずに中断します。

- 移行後のメタデータから再構成した問題 URL が、移行前の全 bin と文字列一致すること
- 変換後の TOML から読み直した入出力が、変換前の YAML とバイト単位で一致すること

## 範囲外

acrust は「AtCoder とのやり取り + ビルド/テスト」に専念します。以下は**意図的に**作りません。

- git 操作（コミット・push）、記事テンプレの生成、hooks 機構
- 自作ライブラリのバンドル（cargo-equip 相当）。`submit` は常に `src/bin/{alias}.rs` をそのまま提出します（「提出したもの = リポジトリの中身」を不変条件にするため）
- AtCoder 以外のジャッジ、Rust 以外の言語

## ロードマップ

| | 内容 | 状態 |
|---|---|---|
| M0 | CLI の骨組み、`config.toml` の読み込み、パッケージ・問題解決、`init` | ✅ |
| M1 | `login` / `logout` / `status`、セッションの永続化 | ✅ |
| M2 | `new` / `fetch`（`/tasks` と `/tasks_print` のパース、testcases TOML 生成） | |
| M3 | `test`（ビルド・並列実行・TL・判定・差分表示） | |
| M4 | `run` / `submit`（言語 ID 自動判定、結果追跡） | |
| M5 | `env update`、`open`、`migrate` | |
| M6 | crates.io / GitHub Releases での公開 | |

## 開発

```console
$ cargo fmt --all --check
$ cargo clippy --all-targets -- -D warnings
$ cargo test

# AtCoder に実際にアクセスするテスト（CI では走らせない）
$ cargo test --test network -- --ignored --test-threads=1
```

テストの fixture に **AtCoder の問題文を含めないでください**。問題文の著作権は AtCoder と作問者にあります。パーサのテストに必要なのは HTML の構造だけなので、構造を再現した合成 HTML を使います。

## 謝辞

- [qryxip/cargo-compete](https://github.com/qryxip/cargo-compete) と [qryxip/snowchains](https://github.com/qryxip/snowchains) — 長らくお世話になりました。acrust の設計はこの2つが解いた問題を出発点にしています
- [tanakh/cargo-atcoder](https://github.com/tanakh/cargo-atcoder) — Rust 専用 AtCoder ツールの先行事例
- [online-judge-tools/api-client](https://github.com/online-judge-tools/api-client) — AtCoder の HTTP 仕様の参考にしました

コードは持ち込んでおらず、テストケース形式も独自のものです。

## ライセンス

`MIT OR Apache-2.0`（[LICENSE-MIT](LICENSE-MIT) / [LICENSE-APACHE](LICENSE-APACHE)）
