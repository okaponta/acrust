# acrust

[English](README.md) | 日本語

**AtCoder × Rust 専用**の競技プログラミング支援ツール。

> **出力は英語です。** AtCoder の利用者は日本国外が 3 分の 2 を占めるので、
> ツールの出力は英語に統一しています（cargo-compete も同じ）。この文書は日本語版です。

```console
$ cargo install acrust
$ acrust login          # ブラウザのセッションクッキーを貼り付ける
$ acrust new abc474
$ acrust test a
$ acrust submit a
```

ジャッジ環境への追従が主眼です。AtCoder は言語アップデートごとに、ジャッジが実際に使う
`Cargo.toml` をそのまま含んだインストールスクリプトを公開しており、`acrust env update` は
そこから `[dependencies]`・`Cargo.lock`・`edition`・固定する rustc を再生成します。

リポジトリの用意は `acrust init`、続けて `acrust env update`。ビルドしたくなければ
`cargo binstall acrust` でビルド済みバイナリが入ります。

## ログイン

AtCoder の `/login` は Cloudflare Turnstile で守られているため、ID / パスワードをプログラムから
POST してもログインできません。acrust はブラウザが既に持っているセッションクッキーを取り込みます。

`acrust login` を打つと https://atcoder.jp/login が開きます。そこでログインしたら、DevTools →
Application（Safari では Storage）→ Cookies → `https://atcoder.jp` を開き、`REVEL_SESSION` の
Value を貼り付けてください（入力は伏せ字になります）。`REVEL_SESSION=...` の形でも、Cookie
ヘッダを丸ごと貼っても通ります。スクリプトからは `acrust login --cookie <値>`。
クッキーは期限が切れるまで使えます（実測で 180 日ほど）。

保存先は `~/.local/share/acrust/session.json`、パーミッションは 0600 です。このクッキーは
パスワードと同等なので、他ユーザーから読める状態を見つけたら警告したうえで締め直します。

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
| `acrust migrate` | cargo-compete のリポジトリを acrust の形に変換する（既定は下見） |

問題を省くと `test` / `run` / `submit` / `copy` は **最後に編集した** `src/bin/*.rs` を対象にし、
どれを選んだか必ず表示します。`submit` はそのとき y/N も確認します。誤提出のペナルティは
取り消せないからです。

取得し直しても `src/bin/*.rs` は**絶対に上書きしません**。手で足したテストケース、手で直した
`match`、`Cargo.toml` に手で足したものも残ります。まっさらに戻したいときは
`acrust fetch --overwrite`。

## テスト

`acrust test` はビルドを1回だけ行い、ケースを並列に実行します。全 AC なら終了コード 0、
そうでなければ 1 なので、シェルの `&&` にそのまま繋げられます。失敗したケースは
`input` / `expected` / `output` を別々のブロックに出し、食い違う行に `✗` を付けます。
そのままコピーできるよう、中身は字下げしません。

打ち切りは「問題の制限時間 × `timeout-margin`（既定 1.5）」ですが、**5 秒より短くはしません**。
手元のマシンはジャッジより遅いことがあり、TL 2 秒の問題を 3 秒で切ると、ジャッジでは通る解答を
TLE と言ってしまうためです。

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
止められます）。送るのは常に `src/bin/{alias}.rs` そのものです。言語 ID はハードコードせず
提出ページから読むので、言語アップデートで壊れません。

> **コンテストが終了したあとは、コマンドから提出できません。**
> AtCoder は 2025-03 に Cloudflare Turnstile を導入し、`/login` だけでなく**終了したコンテストの
> 提出フォーム**にもこれを出すようになりました。隠しフィールド `cf-turnstile-response` は
> ブラウザ上の JavaScript が差し込むため、`csrf_token` が正しくても素の POST は弾かれます。
> **開催中の提出はこれまでどおり通ります。** acrust は CAPTCHA を迂回しません。

そのときは `acrust copy` を使ってください。`src/bin/{alias}.rs` を加工せずそのまま
（`submit` が送るのと同じバイト列を）クリップボードに入れ、貼り付け先を表示します。

```console
$ acrust copy c
✓ copied (5 lines / 86 bytes, via pbcopy)

Paste it here to submit:
  https://atcoder.jp/contests/abc418/tasks/abc418_c
```

## 配置

```
atcoder-rust/
├── .acrust/config.toml           # 設定
├── .acrust/template/             # main.rs, dependencies.toml, Cargo.lock, copy/
├── rust-toolchain.toml           # ジャッジと同じ rustc に固定
└── abc474/                       # Cargo.toml, src/bin/{a..g}.rs, testcases/{a..g}.toml
```

資格情報はリポジトリの外に置きます。セッションは `~/.local/share/acrust/session.json`
（0600、`ACRUST_SESSION_FILE` で差し替え可）、キャッシュは `~/.cache/acrust/` です。

## cargo-compete からの移行

`acrust migrate` は一度きりの変換です。`--write` を付けるまでは下見で、git の working tree が
綺麗であることを求めます（`--allow-dirty` で回避）。変換前に往復検証を行い、1 件でも合わなければ
何も書かずに止まります。

## 開発

```console
$ cargo fmt --all --check
$ cargo clippy --all-targets --all-features -- -D warnings
$ cargo test
```

リポジトリの外に依存するテストは `live` feature の後ろに置いてあり、普段の `cargo test` では
ビルドもされません。**AtCoder の問題文をテスト fixture に置かないでください。** 著作権は AtCoder と
作問者にあります。パーサのテストに必要なのは HTML の構造だけなので、構造を写した合成 HTML で足ります。

## 謝辞

長年お世話になった [cargo-compete](https://github.com/qryxip/cargo-compete) と
[snowchains](https://github.com/qryxip/snowchains)（acrust の設計はこの2つが解いた問題から
出発しています）、Rust 専用ツールの先行例である
[cargo-atcoder](https://github.com/tanakh/cargo-atcoder)、AtCoder の HTTP まわりの挙動を
参照した [online-judge-tools/api-client](https://github.com/online-judge-tools/api-client)。
いずれからもコードは流用していません。

## ライセンス

`MIT OR Apache-2.0`（[LICENSE-MIT](LICENSE-MIT) / [LICENSE-APACHE](LICENSE-APACHE)）
