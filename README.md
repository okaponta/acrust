# acrust

English | [日本語](README-ja.md)

A competitive programming CLI **built only for AtCoder and Rust**. A successor to
[cargo-compete](https://github.com/qryxip/cargo-compete), rebuilt around the idea that giving up
every other judge and every other language buys you a lot.

```console
$ cargo install acrust
$ acrust login          # paste the session cookie from your browser
$ acrust new abc474
$ acrust test a
$ acrust submit a
```

> **Status: in development (v0.1 not released yet)**
> Every command is implemented. Publishing to crates.io is still ahead ([roadmap](#roadmap)).

## Why

`cargo-compete` has not been updated since 2024-06, so every time AtCoder refreshes its judge
environment you have to chase the configuration by hand. acrust automates exactly that.

### 1. It follows the judge environment on its own (the main difference)

With every language update AtCoder publishes an install script that embeds the very `Cargo.toml`
the judge uses. `acrust env update` reads it and regenerates

- `[dependencies]` (68 crates, versions matched exactly)
- the same `Cargo.lock` the judge has
- `edition`
- **the rustc version**

With `cargo-compete` you update these by hand, which is how repositories end up still configured
for the 2023 environment.

### 2. It pins the toolchain to the judge too

`acrust init` writes a `rust-toolchain.toml` at the root of the repository and pins the same rustc
the judge runs. Without it, one `rustup update` leaves only your machine on a newer rustc, and the
difference shows up in the worst possible way: **everything builds, every sample is AC, and the
submission is a CE**.

### 3. It does not hard-code `language_id`

acrust picks `Rust (rustc ...)` off the `<select name="data.LanguageId">` on the submit page and
remembers it in `~/.cache/acrust/`. A language update that changes the ID does not break it.

For the record: `cargo-compete` writes `5054`, and the ID AtCoder currently uses is **6088**. The
hard-coded value is already wrong.

### 4. It is polite to AtCoder

- One contest's samples in **two requests** (`/tasks` and `/tasks_print`)
- At least one second between requests; exponential backoff on 429 / 5xx, honouring `Retry-After`
- A User-Agent that says who it is

### 5. It stores the session as 0600

An AtCoder session cookie is long-lived, and anyone holding it can submit as you: it is as good as
a password. acrust saves it to `~/.local/share/acrust/session.json` **with mode 0600**, and never
asks for your password at all. If it finds the file readable by others, it says so and tightens it.

## Logging in

AtCoder's `/login` is behind **Cloudflare Turnstile (CAPTCHA)**, so POSTing an ID and password from
a program does not log you in — even with a correct `csrf_token` you get "エラーが発生しました。".
Working around a CAPTCHA is the wrong move, so acrust imports a session cookie you already have in
your browser.

The same Turnstile also guards **the submit form of a contest that has ended** (see
[Submitting](#submitting)).

```console
$ acrust login
```

`acrust login` opens https://atcoder.jp/login in your browser, then:

1. Log in
2. Open DevTools
3. Application (Storage in Safari) -> Cookies -> `https://atcoder.jp`
4. Copy the Value of `REVEL_SESSION` and paste it (input is hidden)

Pass `acrust login --no-open` if you would rather it did not open a browser.

Pasting `REVEL_SESSION=...` as-is works, and so does pasting a whole Cookie header. From a script,
use `acrust login --cookie <value>`.

Once imported it keeps working until it expires (measured at around 180 days).

## Commands

| Command | What it does |
|---|---|
| `acrust init` | Initialize. Generates `.acrust/` and `rust-toolchain.toml` |
| `acrust login` / `logout` | Get or discard the AtCoder session |
| `acrust status` | Show where the config is, the judge environment, the login state and the current problem, and say whether anything needs doing |
| `acrust new <contest>` | Create the package and fetch the samples |
| `acrust fetch [contest]` | Fetch the samples again |
| `acrust test [problem]` | Build and run the sample tests |
| `acrust run [problem]` | Run with stdin passed straight through |
| `acrust submit [problem]` | Test, then submit and follow the result |
| `acrust copy [problem]` | Copy the solution to the clipboard (for submitting by hand) |
| `acrust open [problem]` | Open the problem in a browser |
| `acrust env update` | Regenerate the crates, `Cargo.lock`, `edition` and rustc from the judge environment |
| `acrust migrate` | Migrate a cargo-compete repository to the acrust layout (round-trip verified, dry-run by default) |

### Where things stand

```console
$ acrust status
acrust 0.1.0

  ✓ config     /Users/you/repos/atcoder-rust/.acrust/config.toml
  ✓ judge env  2025-10 / edition 2024 / 68 crates / Cargo.lock present
  ✓ rustc      1.89.0 (matches rust-toolchain.toml)
  ✓ session    /Users/you/.local/share/acrust/session.json (600)
  ✓ login      okaponta
  ✓ package    abc474
  ✓ problem    c (src/bin/c.rs)

✓ all good
```

When something is missing, it lists the commands to run.

```console
  ! judge env  2025-10 / edition 2024 / 68 crates / no Cargo.lock
  ✗ login      not logged in

! next steps (2)
    acrust env update   fetch the same Cargo.lock the judge uses
    acrust login        log in to AtCoder
```

Use `acrust status --offline` to look only at local information, without asking AtCoder.

### What `new` / `fetch` will not break

Fetching again keeps everything you touched by hand.

- `src/bin/*.rs` is **never overwritten** (your solution lives there)
- `Cargo.toml` is edited through `toml_edit`, adding only what is missing. Hand-added dependencies
  and comments survive
- Test case files keep hand-added `[[cases]]` and a hand-edited `match` / `[float]`, and only the
  samples are refreshed (for a problem with several valid answers, switching to `match = "words"`
  is a judgement call no autodetection can reproduce)

Use `acrust fetch --overwrite` to go back to a clean slate.

Running `acrust new` before a contest starts tells you it has not started and **creates nothing**.
At that point there is no problem URL and there are no samples, so putting `src/bin/` down would
only mean running it again later. Run `acrust new` once the contest is live and it fills in what is
missing without disturbing what is already there.

### Testing

```console
$ acrust test a

  AC   sample1      6 ms
  AC   sample2      5 ms

✓ 2/2 AC
```

One build, then the cases run in parallel (one per logical core by default). Verdicts are `AC` /
`WA` / `RE` / `TLE`. All AC exits 0, anything else exits 1.

A failing case lays out `input` / `expected` / `output` separately and marks the differing lines
with `✗`.

```console
── sample1 WA
input:
  8
  greentea
expected:
✗ 1  Yes
output:
✗ 1  No
✗ 2  one line too many
```

The body of a failing case is **not indented**, so you can copy it straight out. cargo-compete
prints it from the left margin for the same reason.

For `RE` you get the panic location and message (no backtrace, which is only long — if you have set
`RUST_BACKTRACE=1` yourself, that is honoured).

A case is killed at the problem's time limit times `timeout-margin` (1.5 by default), **but never
sooner than 5 seconds**. Your machine can be slower than the judge, and cutting a 2-second problem
off at 2 x 1.5 = 3 seconds would call a solution TLE that the judge accepts.

Interactive problems do not fit the sample-test shape, so acrust says so and skips them.

### Submitting

```console
$ acrust submit c
  → abc474 c (src/bin/c.rs)
  AC   sample1      12 ms
  AC   sample2      11 ms
✓ 2/2 AC
  login:         okaponta
  language:      Rust (rustc 1.89.0) (id=6088)
✓ submitted  WJ  https://atcoder.jp/contests/abc474/submissions/12345678
  WJ   2s
  AC   5s           312 ms / 4.2 MB
```

> **You cannot submit from the command line once a contest has ended.**
> AtCoder introduced Cloudflare Turnstile in 2025-03 and now puts it on **the submit form of a
> contest that has finished** as well. The hidden `cf-turnstile-response` field is injected by
> JavaScript in the browser, so a plain POST is rejected with "エラーが発生しました。" even when
> `csrf_token` is correct. **Submitting during a live contest still works.** acrust does not work
> around CAPTCHAs.
>
> Trying to submit to a finished contest stops like this:
>
> ```console
> warning: the contest is over, so submit cannot run. Use copy and submit it by hand
> error: the submission was not accepted
> ```
>
> Use [`acrust copy`](#submitting-by-hand-to-a-finished-contest) instead.

### Submitting by hand to a finished contest

`acrust copy` puts `src/bin/{alias}.rs` on the clipboard as-is and prints where to paste it.

```console
$ acrust copy c
  → abc418 c (src/bin/c.rs)
✓ copied (5 lines / 86 bytes, via pbcopy)

Paste it here to submit:
  https://atcoder.jp/contests/abc418/tasks/abc418_c
  or run `acrust open c` to open it
```

It copies **the same bytes** `submit` would send, with no processing. Omit the problem and it infers
the most recently modified `src/bin/*.rs`, exactly as `test` / `submit` do, and always shows what it
picked.

Copying goes through whatever the OS provides (`pbcopy` on macOS, `clip` on Windows, otherwise
`wl-copy` then `xclip` then `xsel`). No crate was added for it, so nothing here asks you to install
X11 or Wayland development packages on Linux.

What gets submitted is always `src/bin/{alias}.rs` itself; there is no hook to substitute something
else. "What was submitted is what is in the repository" always holds.

- The sample tests run before submitting (`-f` skips them)
- A y/N confirmation appears **only when you omit the problem**. Name it and it submits immediately
- The result is followed afterwards (`--no-watch` turns this off). The interval starts at 2 seconds
  and grows 1.5x up to 10 seconds, giving up after a minute. It stops the moment the verdict is
  final, honours `Retry-After`, and prints the URL and gives up after repeated failures
- Following the result hits the same lightweight API the AtCoder page itself polls (651 bytes,
  against 27KB for the submissions page)

When you omit the problem, `test` / `run` target the **most recently modified** `src/bin/*.rs` and
always print which one they chose. `submit` additionally asks y/N in that case, because a
mis-submission carries a penalty you cannot take back.

## Layout

```
atcoder-rust/
├── .acrust/
│   ├── config.toml
│   └── template/
│       ├── main.rs
│       ├── dependencies.toml     # generated by env update
│       ├── Cargo.lock            # fetched by env update
│       └── copy/                 # copied into the package by new
├── .cargo/config.toml            # target-dir = "target"
├── rust-toolchain.toml           # pinned to the judge's rustc
└── abc474/
    ├── Cargo.toml
    ├── src/bin/{a..g}.rs
    └── testcases/{a..g}.toml
```

Credentials live outside the repository.

```
~/.local/share/acrust/session.json    # 0600; override with ACRUST_SESSION_FILE
~/.cache/acrust/                      # cache
```

## Following the judge environment

```console
$ acrust env update

  → language list: https://img.atcoder.jp/file/language-update/2025-10/language-list.html
  → install script: .../088-1-82-0_rustc.toml
  → Cargo.lock: https://raw.githubusercontent.com/rust-lang-ja/atcoder-proposal/.../Cargo.lock

── Difference from Rust (rustc 1.89.0)
  rustc:         1.75.0 -> 1.89.0
  edition:       2021 -> 2024
  Cargo.lock:    none -> 1682 lines
  crates:        64 added / 0 removed / 2 changed
    ~ itertools =0.11.0 → =0.14.0
    ~ proconio =0.4.5 → =0.5.0

Write these changes? [y/N]:
```

It writes `.acrust/template/dependencies.toml`, `.acrust/template/Cargo.lock`,
`rust-toolchain.toml`, and the `edition` in `.acrust/config.toml`. The config goes through
`toml_edit`, so comments and formatting survive.

When a new language update lands, point at it with `acrust env update --language-list <URL>` and
acrust remembers the URL from then on.

## Migrating from cargo-compete

`acrust migrate` performs a one-time conversion. Dry-run is the default; nothing is written until
you pass `--write`. It requires a clean working tree (`--allow-dirty` to bypass that).

Before converting it **verifies the round trip**, and stops without writing anything if even one
item does not match.

- The problem URL rebuilt from the migrated metadata matches every pre-migration bin, as a string
- The input and output read back from the converted TOML match the original YAML, byte for byte

Reading the cargo-compete layout happens only inside `acrust migrate`. `test` and `submit` know
nothing but the acrust layout, so compatibility code does not spread across the tool.

```console
$ acrust migrate

  target:        /Users/you/repos/atcoder-rust

── What will be migrated
  packages:      426
  bins:          2716
  test cases:    2716 files / 7534 cases

This was a dry run. Pass --write to actually change things
```

## Out of scope

acrust sticks to "talk to AtCoder, then build and test". The following are left out **on purpose**.

- git operations (committing, pushing), article templates, a hooks mechanism
- Bundling your own library (what cargo-equip does). `submit` always sends `src/bin/{alias}.rs`
  as-is, to keep "what was submitted is what is in the repository" an invariant
- Judges other than AtCoder, languages other than Rust

## Roadmap

| | Contents | Status |
|---|---|---|
| M0 | CLI skeleton, reading `config.toml`, resolving packages and problems, `init` | ✅ |
| M1 | `login` / `logout` / `status`, persisting the session | ✅ |
| M2 | `new` / `fetch` (parsing `/tasks` and `/tasks_print`, generating the testcases TOML) | ✅ |
| M3 | `test` (build, parallel run, time limit, judging, diff display) | ✅ |
| M4 | `run` / `submit` (language ID detection, following the result) | ✅ |
| M5 | `env update`, `open`, `migrate` | ✅ |
| M6 | Publishing on crates.io and GitHub Releases | The release workflow and `cargo publish --dry-run` are done; publishing is still ahead |

## Development

```console
$ cargo fmt --all --check
$ cargo clippy --all-targets --all-features -- -D warnings
$ cargo test
```

Tests that depend on anything outside the repository are split off behind the `live` feature. A
plain `cargo test` does not even build them, so no `ignored` lines show up in the results.

```console
# actually talks to AtCoder
$ cargo test --features live --test network -- --test-threads=1

# checks against real data in a local repository
$ cargo test --features live --test acceptance_abc418 -- --nocapture
```

**Do not put AtCoder problem statements in test fixtures.** They are copyrighted by AtCoder and the
problem setters. A parser test only needs the shape of the HTML, so use synthetic HTML that
reproduces the structure.

## Acknowledgements

- [qryxip/cargo-compete](https://github.com/qryxip/cargo-compete) and
  [qryxip/snowchains](https://github.com/qryxip/snowchains) — they served me well for years. The
  design of acrust starts from the problems those two solved
- [tanakh/cargo-atcoder](https://github.com/tanakh/cargo-atcoder) — prior art for a Rust-only
  AtCoder tool
- [online-judge-tools/api-client](https://github.com/online-judge-tools/api-client) — a reference
  for AtCoder's HTTP behaviour

No code was taken from any of them, and the test case format is acrust's own.

## License

`MIT OR Apache-2.0` ([LICENSE-MIT](LICENSE-MIT) / [LICENSE-APACHE](LICENSE-APACHE))
