# acrust

English | [日本語](README-ja.md)

A competitive programming CLI **built only for AtCoder and Rust**.

```console
$ cargo install acrust
$ acrust login          # paste the session cookie from your browser
$ acrust new abc474
$ acrust test a
$ acrust submit a
```

It keeps your setup in step with the judge. With every language update AtCoder publishes an install
script containing the exact `Cargo.toml` the judge builds with, and `acrust env update` regenerates
`[dependencies]`, `Cargo.lock`, `edition` and the pinned rustc from it.

Set up a repository with `acrust init`, then `acrust env update`. `cargo binstall acrust` installs
a prebuilt binary if you would rather not compile it.

## Logging in

AtCoder's `/login` is behind Cloudflare Turnstile, so no program can log in with an id and a
password. acrust imports the session cookie your browser already holds instead.

`acrust login` opens https://atcoder.jp/login. Log in there, then open DevTools ->
Application (Storage in Safari) -> Cookies -> `https://atcoder.jp`, and paste the Value of
`REVEL_SESSION` (the input is hidden). Pasting `REVEL_SESSION=...`, or a whole Cookie header, works
just as well; from a script, use `acrust login --cookie <value>`. The cookie lasts until it expires,
measured at around 180 days.

It is saved to `~/.local/share/acrust/session.json` with mode 0600. The cookie is as good as a
password, so if acrust finds that file readable by others it says so and tightens it.

## Commands

| Command | What it does |
|---|---|
| `acrust init` | Generate `.acrust/` and `rust-toolchain.toml` |
| `acrust login` / `logout` | Get or discard the AtCoder session |
| `acrust status` | Where the config is, which judge environment is in use, whether you are logged in, and what needs doing |
| `acrust new <contest>` | Create the package and fetch the samples |
| `acrust fetch [contest]` | Fetch the samples again |
| `acrust test [problem]` | Build and run the sample tests |
| `acrust run [problem]` | Run with stdin passed straight through |
| `acrust submit [problem]` | Test, then submit and follow the result |
| `acrust copy [problem]` | Copy the solution to the clipboard |
| `acrust open [problem]` | Open the problem in a browser |
| `acrust env update` | Regenerate the crates, `Cargo.lock`, `edition` and rustc from the judge environment |
| `acrust migrate` | Convert a cargo-compete repository to the acrust layout (dry-run by default) |

Leave the problem off and `test` / `run` / `submit` / `copy` take the most recently modified
`src/bin/*.rs`, always printing which one they picked. `submit` also asks y/N in that case, because
a mis-submission costs a penalty you cannot take back.

Fetching again never overwrites `src/bin/*.rs`, and keeps hand-added test cases, a hand-edited
`match`, and anything you added to `Cargo.toml`. Use `acrust fetch --overwrite` for a clean slate.

## Testing

`acrust test` builds once and runs the cases in parallel. All AC exits 0 and anything else exits 1,
so it drops into a shell `&&`. A failing case lays out `input` / `expected` / `output` in separate
blocks, unindented so you can copy them straight out, with `✗` against the lines that differ.

A case is killed at the problem's time limit times `timeout-margin` (1.5 by default), but never
sooner than 5 seconds: your machine can be slower than the judge, and cutting a 2-second problem off
at 3 seconds would call a solution TLE that the judge accepts.

## Submitting

```console
$ acrust submit c
  → abc474 c (src/bin/c.rs)
✓ 2/2 AC
  language:      Rust (rustc 1.89.0) (id=6088)
✓ submitted  WJ  https://atcoder.jp/contests/abc474/submissions/12345678
  AC   5s           312 ms / 4.2 MB
```

The sample tests run first (`-f` skips them), and the verdict is followed afterwards (`--no-watch`
turns that off). What is sent is always `src/bin/{alias}.rs` as it stands. The language id is read
off the submit page rather than hardcoded, so a language update does not break it.

> **You cannot submit from the command line once a contest has ended.**
> Since March 2025 Cloudflare Turnstile guards the submit form of a finished contest as well as
> `/login`. The hidden `cf-turnstile-response` field is filled in by the browser's JavaScript, so a
> plain POST is refused however correct its `csrf_token`. **Submitting during a live contest still
> works.** acrust does not work around CAPTCHAs.

Use `acrust copy` for those. It puts `src/bin/{alias}.rs` on the clipboard byte for byte — the same
bytes `submit` would send — and prints where to paste them.

```console
$ acrust copy c
✓ copied (5 lines / 86 bytes, via pbcopy)

Paste it here to submit:
  https://atcoder.jp/contests/abc418/tasks/abc418_c
```

## Layout

```
atcoder-rust/
├── .acrust/config.toml           # settings
├── .acrust/template/             # main.rs, dependencies.toml, Cargo.lock, copy/
├── rust-toolchain.toml           # pinned to the judge's rustc
└── abc474/                       # Cargo.toml, src/bin/{a..g}.rs, testcases/{a..g}.toml
```

Credentials live outside it: `~/.local/share/acrust/session.json` (0600, overridden by
`ACRUST_SESSION_FILE`) and the cache in `~/.cache/acrust/`.

## Migrating from cargo-compete

`acrust migrate` converts a cargo-compete repository once. It is a dry run until you pass `--write`,
wants a clean working tree (`--allow-dirty` overrides), and verifies that the conversion round-trips
before writing anything — one mismatch and it stops having written nothing.

## Development

```console
$ cargo fmt --all --check
$ cargo clippy --all-targets --all-features -- -D warnings
$ cargo test
```

Tests that reach outside the repository sit behind the `live` feature, so a plain `cargo test` does
not build them. **Do not put AtCoder problem statements in test fixtures** — they belong to AtCoder
and the problem setters, and synthetic HTML with the same structure is enough.

## Acknowledgements

[cargo-compete](https://github.com/qryxip/cargo-compete) and
[snowchains](https://github.com/qryxip/snowchains), which served me for years and which acrust's
design starts from; [cargo-atcoder](https://github.com/tanakh/cargo-atcoder) as prior art for a
Rust-only tool; [online-judge-tools/api-client](https://github.com/online-judge-tools/api-client) as
a reference for AtCoder's HTTP behaviour. No code was taken from any of them.

## License

`MIT OR Apache-2.0` ([LICENSE-MIT](LICENSE-MIT) / [LICENSE-APACHE](LICENSE-APACHE))
