# Development

This guide is for developers who want to change COAT. **You don't need to know Rust**: the
[Rust in five minutes](#rust-in-five-minutes) section covers everything this project uses, and
[Common changes](#common-changes) walks through the usual edits step by step.

- [Setting up](#setting-up)
- [Rust in five minutes](#rust-in-five-minutes)
- [Pointing COAT at simlog](#pointing-coat-at-simlog)
- [Running COAT while developing](#running-coat-while-developing)
- [A tour of the code](#a-tour-of-the-code)
- [Tests](#tests)
- [Common changes](#common-changes)
- [Building the apps](#building-the-apps)
- [Troubleshooting your setup](#troubleshooting-your-setup)

## Setting up

1. **Install Rust** with rustup, the official installer:
   - Mac or Linux: run the command shown on [rustup.rs](https://rustup.rs/) in a terminal.
   - Windows: download and run `rustup-init.exe` from [rustup.rs](https://rustup.rs/). If it asks
     to install the Visual Studio C++ build tools, say yes.

   Close and reopen your terminal afterwards, then check: `cargo --version`.
2. **Get the code**: `git clone https://github.com/archways404/TVX-SOC-COAT.git` and `cd TVX-SOC-COAT`.
3. **Run the tests**: `cargo test`. The first run downloads the libraries COAT uses and compiles
   everything, which takes a minute or two. Later runs take seconds.
4. **An editor**: [VS Code](https://code.visualstudio.com/) with the **rust-analyzer** extension
   gives you autocomplete, inline errors and "go to definition". Any editor works.

You also need the Telavox VPN, and the simlog addresses (next sections), to fetch real calls.

## Rust in five minutes

| Rust thing | What it is | If you know… |
|---|---|---|
| `cargo` | The build tool and package manager. Builds, runs, tests. | `npm`, `pip`, `mvn`, `gradle` |
| `Cargo.toml` | The project file: name, version, dependencies. | `package.json`, `pom.xml` |
| `Cargo.lock` | The exact versions of every dependency. Committed, so every build is the same. | `package-lock.json` |
| crate | A library (dependency) or program. | an npm package / a jar |
| `src/main.rs` | Where the program starts (`fn main`). Other `src/*.rs` files are modules. | `index.js`, `Main.java` |
| `target/` | Build output. Safe to delete; ignored by git. | `dist/`, `build/` |
| `--release` | Build with optimisations (slower to compile, much faster to run). | a production build |
| `#[cfg(test)] mod tests` | Tests live at the bottom of the file they test. | `*.test.js` next to the code |

A few things you'll see in the code:

- **`Result<T, CoatError>`** is how a function says "this can fail". `?` after a call means
  "if this failed, return the error from here too". `CoatError` holds a message meant for the user.
- **`Option<T>`** is a value that may be missing (`Some(value)` or `None`), Rust's version of `null`.
- **`&str` and `String`** are both text: `&str` borrows text owned by someone else, `String` owns it.
  If the compiler complains about one where it wants the other, `.to_string()` or `&` usually fixes it.
- **`re!(NAME, r"…")`** in `src/trace.rs` defines a regular expression, compiled once the first time
  it's used. `r"…"` is a raw string, so backslashes don't need escaping.
- **`if let Some(c) = REGEX.captures(text) { … &c["name"] … }`** means "if the pattern matches,
  run this with the named capture groups".

The compiler is strict but its error messages are good: read them top to bottom, and the first
one is usually the real problem.

## Pointing COAT at simlog

The simlog addresses are internal, so they're **not in the source code**. Official downloads have
them built in (see [Releasing](releasing.md)). For a build of your own, set them as
environment variables. Ask a colleague for the addresses, and **never commit them**.

| Variable | Simlog environment |
|---|---|
| `COAT_SIMLOG_NORDIC` | Nordic (the default) |
| `COAT_SIMLOG_UAE` | UAE (used for links whose address contains `.ae.`, or with `--env uae`) |

Mac or Linux (for the current terminal; add it to `~/.zshrc` or `~/.bashrc` to keep it):

```sh
export COAT_SIMLOG_NORDIC="http://<nordic simlog address>"
export COAT_SIMLOG_UAE="http://<uae simlog address>"
```

Windows PowerShell (for the current window; use *System → Environment Variables* to keep it):

```powershell
$env:COAT_SIMLOG_NORDIC = "http://<nordic simlog address>"
$env:COAT_SIMLOG_UAE = "http://<uae simlog address>"
```

Set at **run time**, the variables point any build of COAT at simlog. Set at **build time** (while
running `cargo build`), they're compiled into the program as defaults, which is what the release
workflow does. Without them, COAT says it doesn't know the address of simlog.

## Running COAT while developing

`cargo run --release -- <arguments>` builds COAT and runs it. Everything after `--` goes to COAT.

```sh
cargo run --release                             # like double-clicking: background web UI + browser
cargo run --release -- serve                    # web UI in this terminal; Ctrl-C stops it
cargo run --release -- Ab3dE9x                  # terminal summary + reports/coat-Ab3dE9x.html
cargo run --release -- "<simlog link>"          # same, from a full simlog link
cargo run --release -- --help                   # all options
```

The options of the terminal command:

| Option | What it does |
|---|---|
| `--scrub` | ask simlog to scrub personal data (default: unscrubbed) |
| `--env nordic` / `--env uae` | force the simlog environment (default: guessed from the link, then the other one is tried) |
| `--no-follow` | don't fetch linked sessions |
| `--max-sessions N` | fetch at most N sessions, the first one included (default 8) |
| `-o FILE` | where to write the HTML report (default `reports/coat-<id>.html`) |
| `--no-html` | only print the terminal summary |
| `--full-log` | include every log line in the report, even very repetitive ones |
| `--open` | open the report in the browser when done |
| `--save-raw FILE` | also save what was fetched, to re-run later without simlog |
| `--input FILE` | analyse a `--save-raw` file instead of fetching |
| `--no-color` | plain terminal output |

**Work offline with a saved call.** Fetch once with `--save-raw ~/calls/mycall.coat.json`, then
iterate on the analysis with `--input ~/calls/mycall.coat.json`: it's instant and needs no VPN. Keep
these files **outside the repository**: they contain real call data. (`*.coat.json` files are
ignored by git, as a safety net.)

Other variables that help while developing:

| Variable | What it does |
|---|---|
| `COAT_NO_BROWSER=1` | Don't open a browser (handy when testing the double-click mode). |
| `COAT_PORT=7300` | Use another port than 7171, so you can run a test copy next to your normal COAT. |
| `COAT_SETTINGS_DIR=/tmp/coat` | Keep settings somewhere else than your real user settings. |
| `COAT_UPDATE_URL=…` | Ask this address instead of GitHub for the latest release (used by the update test). Must be HTTPS, or HTTP to `127.0.0.1`. |
| `NO_COLOR=1` | No colours in the terminal summary. |

## A tour of the code

```
coat (no arguments) ─► app.rs ──► starts serve.rs in the background, opens the browser
coat serve ──────────► serve.rs ─┐  (the web UI: start page, progress, reports; update.rs keeps it current)
coat <link> ─────────► main.rs ──┤
                                  ▼
                       source.rs   fetch the session(s) from simlog
                                  ▼
                       logparse.rs turn each log line into an Entry
                                  ▼
                       trace.rs    work out steps, reasons, outcome, queue, issues → CallTrace
                                  ▼
                       report.rs (HTML page)    term.rs (terminal summary)
                          │
                       ui.rs       the frame around every page: sidebar, header, footer
```

| File | What it does |
|---|---|
| `src/main.rs` | Reads the command line and decides what to do: open the app, run the web UI, or trace one call in the terminal. |
| `src/app.rs` | Double-click behaviour: finds an already running COAT, or starts one in the background (no console window, no Dock icon) and opens the browser. |
| `src/serve.rs` | The local web server: start page, the "trace this" API, progress, finished reports, Quit. Only answers on `127.0.0.1`. |
| `src/source.rs` | Talks to simlog: the addresses, paging (including the parallel fetch and its safety checks), and following linked sessions. Also the `Bundle` file format for `--save-raw`. |
| `src/logparse.rs` | Splits one raw log line into time, machine, program, level, code location and message. |
| `src/trace.rs` | The analysis. Builds the route from the dialplan, picks the reason for each hop, and collects activity, variables, queue details, the outcome and grouped issues. The heart of COAT. |
| `src/report.rs` | Turns the analysis into the report's sections (route, steps, issues, SIP ladder, log …) and the sidebar entries for them. |
| `src/ui.rs` | The frame every page shares: the sidebar (in the style of shadcn/ui), the header with breadcrumbs, the footer with the version, the developer hover card on the logo, and the icons. |
| `src/assets/` | The look and the browser code, as normal files: `coat.css` (all styles), `shell.js` (sidebar), `served.js` (updates, recent traces, Quit), `report.js`, `landing.js`. They're compiled into the program, so a report is still one self-contained file. |
| `src/update.rs` | Automatic updates: asks GitHub for the latest release, downloads and verifies it, swaps it in and restarts. |
| `src/term.rs` | The coloured terminal summary. |
| `build.rs` | Runs before compiling: embeds the Windows icon, and rebuilds when the simlog address variables change. |
| `packaging/` | App icons, the Mac app bundle script, the version scripts used by the release workflow, and the update test. |
| `tests/fixtures/` | A made-up call used by the tests. |
| `.github/workflows/build.yml` | Builds, tests and releases on GitHub. See [Releasing](releasing.md). |

The analysis in `trace.rs` makes **one pass over all entries in time order**. For each entry it
tries the patterns it knows (`handle`, then `handle_app`, `handle_agi_say`, `handle_script_entry`,
`handle_queue_entry`) and records what it learns: a new step, a cause for the next hop, an
activity, a variable, a fact. After the pass, `distribute` hands the activities and facts to the
step that was active at the time, and `outcome` and `issues` summarise the call.
[How it works](how-it-works.md) explains the ideas behind this.

## Tests

```sh
cargo test                   # all tests
cargo test trace             # only tests whose name contains "trace"
cargo test -- --nocapture    # also show anything the tests print
```

The tests sit at the bottom of each file, in `mod tests`. Two kinds matter most:

- **Unit tests** for small pieces: parsing one log line, paging against a fake simlog (including
  a page that fails and the fallback), reading links and ids.
- **The fixture test** (`matches_the_python_version_on_the_synthetic_call` in `trace.rs`) runs the
  whole analysis on `tests/fixtures/synthetic.json`, a made-up call (menu → user → script → queue →
  agent), and compares the result with `tests/fixtures/synthetic.expected.json`. The expected
  output came from COAT's first (Python) version, so this test proves the analysis still gives the
  same answers. (One exception: SIP times are now lined up with the log clock instead of the
  computer's time zone, so the tests give the same result on every machine.) If you change the analysis on purpose, update the expected file to match and
  review the difference.

All numbers in the fixtures come from ranges the Swedish telecom regulator (PTS) reserves for
fiction (`070-174 06 05…99`, `08-465 004 00…99`), so they can never belong to a real person. **Never
put real call data in the repository**, not even in tests.

**Checking a change against real calls (snapshot test).** To make sure a change doesn't alter the
analysis of real calls, without putting them in the repository:

1. Save a call outside the repo: `cargo run --release -- <id> --save-raw ~/calls/a.coat.json`.
2. **Before** your change, record a snapshot of today's analysis:
   ```sh
   COAT_PARITY_BUNDLE=~/calls/a.coat.json COAT_PARITY_EXPECTED=~/calls/a.snapshot.json cargo test real_call
   ```
   The snapshot file doesn't exist yet, so the test writes it.
3. **After** your change, run the same command again. Now it compares, and names the first
   difference if there is one.

Without these two variables the test does nothing, so a plain `cargo test` is unaffected.

Before opening a pull request, also run `cargo clippy`, Rust's linter. It should report no warnings.

## Common changes

### COAT misses a log message it should understand

Example: a new script logs `"Sending caller to voicemail for 0846500401"` and you want that to
be the reason for the hop.

1. In `src/trace.rs`, add a pattern next to the others at the top:
   ```rust
   re!(VOICEMAIL, r"^Sending caller to voicemail for (?P<n>\S+)");
   ```
2. Find the handler for the program that writes the line. Script messages go through
   `handle_agi_say` (lines like `From X.agi: …`) or `handle_script_entry` (script server lines).
   Add a branch there:
   ```rust
   } else if let Some(c) = VOICEMAIL.captures(text) {
       if on_root {
           self.pending_causes.push((3, "voicemail".into(), format!("{script}: voicemail for {}", &c["n"])));
       }
   }
   ```
   `on_root` limits it to the caller's own session, so lines from an agent's leg don't count as
   reasons for the caller's route. The `3` is the strength: 3 strong, 2 medium, 1 weak. The strongest cause seen since the previous
   step becomes the reason; the others are shown as evidence.
3. Add a test: copy a similar line into the fixture, or write a small test at the bottom of
   `trace.rs`, then run `cargo test`.

### A warning shows up as a problem but is harmless

Add a pattern to `USUALLY_NOISE` in `src/trace.rs`. It's still listed in the report, just behind
the "show … kinds that appear on most calls" toggle.

### A new kind of destination

If the dialplan uses a macro COAT doesn't know (say `voicemail`), steps of that kind still appear,
with the macro's name as the label. To give it a proper name and colour:

1. `kind_label` in `src/trace.rs`: the name shown.
2. The CSS in `src/report.rs`: add `--k-voicemail` colours and a `.k-voicemail{--kc:…}` rule next
   to the others.
3. `kind_color` in `src/term.rs`: the terminal colour.

### Changing how it looks

- **Colours, spacing, fonts**: `src/assets/coat.css`. It's ordinary CSS; the sidebar's colours are
  the `--sidebar…` variables at the start of the app-shell part, for light and dark mode.
- **The sidebar, header and footer**: `src/ui.rs`, with their behaviour in `src/assets/shell.js`.
- **A section of the report**: `src/report.rs`, one function per section (`route_line`,
  `step_card`, `issues`, …), and its browser code in `src/assets/report.js`.
- **The start page**: `landing` in `src/serve.rs` and `src/assets/landing.js`.

The asset files are read when COAT is compiled, so run `cargo run --release -- --input <saved call>
--open` after a change to see it.

### Testing automatic updates

`./packaging/test-update.sh` (on a Mac, or in Git Bash on Windows) does a real update: it builds
COAT as version 0.0.1 and as 0.0.2, publishes 0.0.2 as a fake GitHub release on your computer,
starts 0.0.1, and checks that it downloads, verifies and installs 0.0.2, restarts as 0.0.2, and
cleans up after itself. It uses its own port and settings folder and puts `Cargo.toml` back when
it's done. The release workflow runs it on every build.

## Building the apps

- **Mac**: `./packaging/macos/build-app.sh` builds `dist/COAT.app` and `dist/COAT-macOS.zip`. With
  both Rust targets installed (`rustup target add aarch64-apple-darwin x86_64-apple-darwin`) the
  app runs on Apple Silicon and Intel Macs.
- **Windows**: `cargo build --release` on a Windows machine gives `target\release\coat.exe`, icon
  included. In practice the release workflow builds it.
- **The icon**: the sources are `packaging/icon.svg` and `packaging/icon-small.svg` (a bolder
  version for 16–32 pixels). After editing, export each to its `-1024.png` file and run
  `./packaging/make-icons.sh` (Mac only) to regenerate the Mac, Windows and web icons.

## Troubleshooting your setup

| Problem | Fix |
|---|---|
| `cargo: command not found` | Close and reopen the terminal after installing Rust. |
| Linux: errors about `openssl` | Install OpenSSL's development files (`sudo apt install libssl-dev pkg-config`). Mac and Windows don't need anything. |
| Mac: `linker cc not found` or `xcrun: error` | Install Apple's command-line tools: `xcode-select --install`. |
| Windows: `link.exe not found` | Install the Visual Studio C++ build tools (rustup offers this), then reopen the terminal. |
| COAT says it doesn't know the address of simlog | Set `COAT_SIMLOG_NORDIC` (see [Pointing COAT at simlog](#pointing-coat-at-simlog)). |
| *"Can't reach simlog"* | Connect to the VPN. |
| *"can't listen on 127.0.0.1:7171"* with `serve` | Another COAT is running. Quit it, or use `serve --port 7272`. |
