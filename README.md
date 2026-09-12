# acrust

**AtCoder × Rust 専用**の競技プログラミング支援ツール。[cargo-compete](https://github.com/qryxip/cargo-compete) の後継として、AtCoder と Rust だけに割り切って作り直したもの。

```console
$ cargo install acrust
$ acrust login          # ブラウザのセッションクッキーを貼り付ける
$ acrust new abc474
$ acrust test a
$ acrust submit a
```

> **状態: 開発中（v0.1 未リリース）**
> コマンドは一通り実装済みです。crates.io への公開はこれからです（[ロードマップ](#ロードマップ)）。

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

提出ページの `<select name="data.LanguageId">` から `Rust (rustc ...)` を自動で選び、`~/.cache/acrust/` に覚えます。言語アップデートで ID が変わっても壊れません。

実際、cargo-compete が書き込む `5054` に対して、現在の AtCoder が使う ID は **6088** です。ハードコードは既に壊れています。

### 4. AtCoder に優しい

- 1 コンテスト分のサンプルを **2 リクエスト**で取得（`/tasks` と `/tasks_print`）
- リクエスト間隔は最低 1 秒、429 / 5xx は指数バックオフ（`Retry-After` に従う）
- User-Agent で素性を明示

### 5. セッションを 0600 で保存する

AtCoder のセッションクッキーは有効期限が長く、持っていれば本人として提出できるパスワード同等の資格情報です。acrust は `~/.local/share/acrust/session.json` に **0600 で**保存します。パスワードはそもそも受け取りません。パーミッションが緩いファイルを見つけたら警告して直します。

## ログインについて

AtCoder の `/login` は **Cloudflare Turnstile (CAPTCHA)** で守られているため、ID / パスワードをプログラムから POST してもログインできません（csrf_token が正しくても「エラーが発生しました。」で弾かれます）。CAPTCHA を迂回するのは筋が悪いので、acrust はブラウザで取得済みのセッションクッキーを取り込みます。

```console
$ acrust login
```

`acrust login` が https://atcoder.jp/login をブラウザで開くので、

1. ログインする
2. DevTools を開く
3. Application（Safari は ストレージ）→ Cookies → `https://atcoder.jp`
4. `REVEL_SESSION` の Value をコピーして貼り付ける（伏せ字で入力されます）

ブラウザを開きたくないときは `acrust login --no-open`。

`REVEL_SESSION=...` の形のまま貼っても、Cookie ヘッダを丸ごと貼っても受け付けます。スクリプトから使うときは `acrust login --cookie <値>`。

一度取り込めば有効期限まで（実測で約 180 日）そのまま使えます。

## コマンド

| コマンド | 説明 |
|---|---|
| `acrust init` | 初期化処理を実施。`.acrust/` と `rust-toolchain.toml` を生成する |
| `acrust login` / `logout` | AtCoder のセッションを取得・破棄する |
| `acrust status` | 設定の場所・ジャッジ環境・ログイン状態・いま対象になる問題を表示し、問題がないかを言い切る |
| `acrust new <contest>` | パッケージを生成し、サンプルを取得する |
| `acrust fetch [contest]` | サンプルを取得し直す |
| `acrust test [problem]` | ビルドしてサンプルテストを実行する |
| `acrust run [problem]` | 標準入力を素通しして実行する |
| `acrust submit [problem]` | テストしてから提出し、結果を追跡する |
| `acrust open [problem]` | ブラウザで問題を開く |
| `acrust env update` | ジャッジ環境から依存・`Cargo.lock`・`edition`・rustc を再生成する |
| `acrust migrate` | cargo-compete 形式のリポジトリを acrust 形式へ移行する（往復検証つき・dry-run 既定） |

### いまどうなっているか

```console
$ acrust status
acrust 0.1.0

  ✓ 設定          /Users/okaponta/repos/atcoder-rust/.acrust/config.toml
  ✓ ジャッジ環境  2025-10 / edition 2024 / 68 クレート / Cargo.lock あり
  ✓ rustc         1.89.0（rust-toolchain.toml と一致）
  ✓ セッション    /Users/okaponta/.local/share/acrust/session.json（600）
  ✓ ログイン      okaponta
  ✓ パッケージ    abc474
  ✓ 問題          c（src/bin/c.rs）

✓ 異常なし
```

足りないものがあれば、打つべきコマンドをそのまま並べます。

```console
  ! ジャッジ環境  2025-10 / edition 2024 / 68 クレート / Cargo.lock なし
  ✗ ログイン      未ログイン

! やることが 2 つあります
    acrust env update   ジャッジと同じ Cargo.lock を取得する
    acrust login        AtCoder にログインする
```

AtCoder に問い合わせずローカルの情報だけ見るなら `acrust status --offline`。

### `new` / `fetch` が壊さないもの

取得し直しても手を加えたものは残ります。

- `src/bin/*.rs` は**絶対に上書きしません**（解答が入っているため）
- `Cargo.toml` は `toml_edit` で必要な項目だけ足します。手で足した依存もコメントも残ります
- テストケースは、手で足した `[[cases]]` と手で直した `match` / `[float]` を残し、サンプルだけを更新します
  （複数解を許す問題で `match = "words"` に直す運用は自動判定では再現できないため）

まっさらにしたいときは `acrust fetch --overwrite`。

コンテスト開始前に `acrust new` を打つと、まだ始まっていないことを伝えて**何も作りません**。
問題 URL もサンプルも取れない段階で `src/bin/` だけ置いても、開始後にもう一度打つことに
なるためです。開始後に `acrust new` を打てば、既にあるものは壊さずに足りないものだけ埋まります。

### テスト

```console
$ acrust test a

  AC   sample1      6 ms
  AC   sample2      5 ms

✓ 2/2 AC
```

ビルドは1回、ケースの実行は並列（既定で論理コア数）。判定は `AC` / `WA` / `RE` / `TLE`。
全 AC なら終了コード 0、そうでなければ 1 を返します。

失敗したケースは `input` / `expected` / `output` を別々に並べ、食い違う行に `✗` を付けます。

```console
── sample1 WA
  input:
    8
    greentea
  expected:
    ✗ 1  Yes
  output:
    ✗ 1  No
    ✗ 2  余計な行
```

`RE` ではパニックの位置とメッセージを出します（バックトレースは長いので出しません。
`RUST_BACKTRACE=1` を自分で立てていればそれに従います）。

打ち切りは問題の TL に `timeout-margin`（既定 1.5）を掛けた値、**ただし最低 5 秒**です。
手元のマシンはジャッジより遅いことがあり、TL 2 秒 × 1.5 = 3 秒で切ると
ジャッジでは通る解答を TLE と言ってしまうためです。

インタラクティブ問題はサンプルテストの形にならないので、その旨を表示してスキップします。

### 提出

```console
$ acrust submit c
  → abc474 c (src/bin/c.rs)
  AC   sample1      12 ms
  AC   sample2      11 ms
✓ 2/2 AC
  ログイン:      okaponta
  言語:          Rust (rustc 1.89.0) (id=6088)
✓ 提出しました  WJ  https://atcoder.jp/contests/abc474/submissions/12345678
  WJ   2s
  AC   5s           312 ms / 4.2 MB
```

提出するのは常に `src/bin/{alias}.rs` そのもので、差し替え口はありません。「提出したもの = リポジトリの中身」が常に成り立ちます。

- 提出前にサンプルテストを通します（`-f` で省略）
- **問題を省略したときだけ** y/N の確認が入ります。明示指定なら確認なしで即提出します
- 提出後は結果を追跡します（`--no-watch` で無効化）。間隔は 2 秒から 1.5 倍ずつ、上限 10 秒、1 分で打ち切り。確定したら即やめ、`Retry-After` に従い、連続で失敗したら URL を出して諦めます
- 追跡には AtCoder のページ自身が使う軽量な API を叩きます（提出一覧ページ 27KB に対して 651 バイト）

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

## ジャッジ環境への追従

```console
$ acrust env update

  → 言語一覧: https://img.atcoder.jp/file/language-update/2025-10/language-list.html
  → インストールスクリプト: .../088-1-82-0_rustc.toml
  → Cargo.lock: https://raw.githubusercontent.com/rust-lang-ja/atcoder-proposal/.../Cargo.lock

── Rust (rustc 1.89.0) との差分
  rustc:         1.75.0 → 1.89.0
  edition:       2021 → 2024
  Cargo.lock:    なし → 1682 行
  依存クレート:  追加 64 / 削除 0 / 変更 2
    ~ itertools =0.11.0 → =0.14.0
    ~ proconio =0.4.5 → =0.5.0

この内容で書き換えますか？ [y/N]:
```

書き換えるのは `.acrust/template/dependencies.toml`、`.acrust/template/Cargo.lock`、`rust-toolchain.toml`、`.acrust/config.toml` の `edition` です。設定は `toml_edit` で書き換えるのでコメントも書式も残ります。

新しい言語アップデートが出たら `acrust env update --language-list <URL>` でその URL を指すと、以後は設定に覚えます。

## cargo-compete からの移行

`acrust migrate` が一度きりの変換を行います。dry-run が既定で、`--write` を付けて初めて書き込みます。実行前にクリーンな working tree を要求します（`--allow-dirty` で回避可）。

変換前に **往復検証**を行い、1 件でも不一致があれば何も書かずに中断します。

- 移行後のメタデータから再構成した問題 URL が、移行前の全 bin と文字列一致すること
- 変換後の TOML から読み直した入出力が、変換前の YAML とバイト単位で一致すること

cargo-compete 形式を読むのは `acrust migrate` の中だけです。`test` や `submit` は acrust 形式しか見ないので、互換のためのコードがツール全体に散らばりません。

```console
$ acrust migrate

  対象:          /Users/okaponta/repos/atcoder-rust

── 移行の内容
  パッケージ:    426 個
  bin:           2716 本
  テストケース:  2716 ファイル / 7534 ケース

これは下見です。実際に書き換えるには --write を付けてください
```

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
| M2 | `new` / `fetch`（`/tasks` と `/tasks_print` のパース、testcases TOML 生成） | ✅ |
| M3 | `test`（ビルド・並列実行・TL・判定・差分表示） | ✅ |
| M4 | `run` / `submit`（言語 ID 自動判定、結果追跡） | ✅ |
| M5 | `env update`、`open`、`migrate` | ✅ |
| M6 | crates.io / GitHub Releases での公開 | リリースワークフローと `cargo publish --dry-run` は済み。公開はこれから |

## 開発

```console
$ cargo fmt --all --check
$ cargo clippy --all-targets --all-features -- -D warnings
$ cargo test
```

リポジトリの外に依存するテストは `live` feature で切り離してあります。
普段の `cargo test` ではビルドもされないので、結果に `ignored` が並びません。

```console
# AtCoder に実際にアクセスする
$ cargo test --features live --test network -- --test-threads=1

# 手元の実リポジトリの実データと突き合わせる
$ cargo test --features live --test acceptance_abc418 -- --nocapture
```

テストの fixture に **AtCoder の問題文を含めないでください**。問題文の著作権は AtCoder と作問者にあります。パーサのテストに必要なのは HTML の構造だけなので、構造を再現した合成 HTML を使います。

## 謝辞

- [qryxip/cargo-compete](https://github.com/qryxip/cargo-compete) と [qryxip/snowchains](https://github.com/qryxip/snowchains) — 長らくお世話になりました。acrust の設計はこの2つが解いた問題を出発点にしています
- [tanakh/cargo-atcoder](https://github.com/tanakh/cargo-atcoder) — Rust 専用 AtCoder ツールの先行事例
- [online-judge-tools/api-client](https://github.com/online-judge-tools/api-client) — AtCoder の HTTP 仕様の参考にしました

コードは持ち込んでおらず、テストケース形式も独自のものです。

## ライセンス

`MIT OR Apache-2.0`（[LICENSE-MIT](LICENSE-MIT) / [LICENSE-APACHE](LICENSE-APACHE)）
