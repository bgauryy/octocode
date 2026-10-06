# CLI, TUI, and subprocesses

Load for a Rust command-line tool (args, output, exit codes, config, errors, progress, signals, tests), an interactive terminal UI, or code that spawns child processes (language servers, compilers, CLIs) and reads their output. Why: CLIs fail at the edges (exit codes, broken pipes, signals, orphaned children), not on the happy path. CLI canon: the Command Line Book (`references/sources-and-crates.md`) and clig.dev. TUI stack: `ratatui` (0.30 verified) + `crossterm`; use `tui-realm`/`cursive` only for a retained widget framework.

## CLI: Shape
- `fn main() -> ExitCode` (or `Result<(), E>` for simple tools); `octocode-native/crates/cli/src/main.rs` returns `std::process::ExitCode`. Avoid deep `std::process::exit`: it skips destructors (unflushed buffers, temp-file cleanup).
- Exit codes are API: `0` ok, `1` failure, `2` usage error (clap default), documented extras for distinct outcomes ("found nothing" vs "error").

## CLI: Arguments (`clap` derive)
- `#[derive(Parser)]` struct + `#[derive(Subcommand)]` enum; doc comments become help. `ValueEnum` for closed choices, `value_parser` for typed validation, `env = "APP_TOKEN"` for env fallback.
- Conventions: `-h/--help`, `-V/--version`, `-v`/`-q` (`clap-verbosity-flag`), `--` ends flags, `-` = stdin/stdout, `--json` machine output, `--dry-run` + `--yes` for destructive actions.
- Test the definition once: `#[test] fn verify_cli() { Cli::command().debug_assert(); }`. Generate completions and man pages at build/release time: `clap_complete`, `clap_mangen`.

## CLI: Output
- **stdout = the result, stderr = everything else** (progress, logs, warnings, prompts); `app | jq` never gets a spinner.
- Machine output is a contract: stable `--json` (serde), NDJSON for streams, no color.
- Color only when stdout is a TTY (`std::io::IsTerminal`); honor `NO_COLOR`, `CLICOLOR_FORCE` and `--color auto|always|never`. `anstream`/`anstyle` (used by clap) strip ANSI off-terminal.
- Hot output: `let mut out = BufWriter::new(io::stdout().lock()); writeln!(out, …)?;` (`println!` re-locks and line-flushes each call).
- **Broken pipe:** Rust ignores SIGPIPE, so `println!` panics under `app | head`. Use `writeln!` and treat `ErrorKind::BrokenPipe` as success (exit 0), as `crates/cli/src/cli/mod.rs` does.
- Progress: `indicatif` on stderr (auto-hidden off-TTY); prompts (`dialoguer`/`inquire`) only when stdin is a TTY, with a skip flag for CI.

## CLI: Errors, logs, config, signals
- Apps: `anyhow` + `.context("reading config {path}")`; print `error: …` plus the cause chain, not `Debug`. `color-eyre`/`miette` for rich diagnostics (miette for source spans).
- User mistakes never panic; a panic is a bug (`human-panic` for friendly release reports). `tracing` + `tracing-subscriber` to stderr, level from `-v` or `RUST_LOG`.
- Precedence: flags > env > project config > user config > defaults. Paths from `directories`/`etcetera`; never hard-code `~/.app`.
- Accept `PathBuf`/`OsString`, not `String` (paths needn't be UTF-8); resolve relative paths against the cwd once.
- Ctrl-C: `ctrlc` or `tokio::signal::ctrl_c()` → flag/cancel token, clean up, exit `130`; a second Ctrl-C exits at once.
- Startup time is UX: lazy-init heavy state, no network on `--help`/`--version`, lean dependency tree.
- Test exit code, stdout, and stderr with `NO_COLOR=1` and a temp `HOME`/cwd (tools: `references/testing-and-tooling.md`).

## TUI: Restore the terminal, always
- `ratatui::run(|terminal| app.run(terminal))` (0.30) or `let t = ratatui::init(); … ratatui::restore();`: both enter raw mode + alternate screen and install a **panic hook that restores the terminal**.
- Install other hooks (`color-eyre::install()`) **before** `ratatui::init()` so the restore hook runs first.
- Restore on every exit: quit, `?` errors (restore before printing), Ctrl-C, panics; test by forcing a panic.
- Refuse to start when stdout isn't a TTY; offer the plain CLI mode instead.

## TUI: State, update, view
- One `App` state struct; `view(&App, &mut Frame)` is pure (immediate mode: redraw the whole frame; ratatui diffs).
- Input → `Action` enum → `update(&mut App, Action)` → redraw (Elm/TEA): keybindings, logic and rendering stay separately testable.
- Modes as an enum (`Mode::Normal | Mode::Search { query } | Mode::Confirm(Op)`), not boolean flags. One struct per pane with `handle_event` + `render`; the top level routes focus; widgets own no business logic.

## TUI: Event loop and rendering
- Never block the UI thread: run work in threads/tokio tasks, send results over a channel, `select!` input, results and a tick. Async input: crossterm `EventStream` (feature `event-stream`); sync: `event::poll(timeout)` then `event::read()`.
- Redraw on change or at a capped ~30–60 fps, never a hot spin. Virtualize long lists (`List`/`Table` with `ListState` offset); never format 100k items per frame.
- Handle `Event::Resize`, and only `KeyEventKind::Press` (Windows also reports releases).
- `Layout` + `Constraint` (`Length`, `Min`, `Percentage`, `Fill`); test at 80×24 and tiny sizes, and show "terminal too small" instead of panicking on zero-area rects.
- Support limited terminals; never encode meaning in color alone. Use width-aware `Line`/`Span`, not byte slicing.
- Keys: `q`/`Esc` quits, `?` help, vim + arrows, a visible hint bar, confirmation for destructive actions. Mouse is opt-in (`EnableMouseCapture`) and disabled on exit.
- Test: unit-test `update` with plain `Action`s; render into `Terminal::new(TestBackend::new(80, 24))` and `insta::assert_snapshot!(terminal.backend())`. Logs go to a file (`tracing-appender`) or a `tui-logger` pane, never stdout.

## Subprocess: Kill AND reap on every exit path
- A killed child exists until **reaped** (`wait`/`try_wait`); otherwise a Unix zombie holds a PID slot. Track it (`Option<Child>`); on stop/error/timeout, try graceful shutdown, then `kill()` **and** `wait()`.
- **Graceful-then-forceful**: protocol servers (LSP) get `shutdown`+`exit`, a bounded wait (~1–2s), then force-kill. Wire this into *automatic* teardown: the leak is on the path you forgot.
- **`Drop` can't `await`**: async `Drop` can only `start_kill()` + `try_wait()`, leaving a running child to the runtime's orphan reaper. `Command::kill_on_drop(true)` is the backstop, not the primary: route teardown through an explicit async `stop()`.
- **Group-kill** grandchildren: own process group (Unix `setsid`/`pre_exec`) or a Windows **Job Object**, then kill the group; `kill()` on the parent leaks grandchildren.

## Subprocess: Bound every resource
- **OS memory cap on the child**: `setrlimit(RLIMIT_AS)` via `pre_exec` (Unix) or a Job Object limit (Windows). Your internal caps don't stop a runaway child from OOMing the *host*.
- **Cap every read**: max frame/message and header size, bounded stdout accumulation, stderr ring buffer (last N lines). Check a declared length (`Content-Length`) **before** allocating.
- **Pool bounds**: max live servers, LRU + idle-timeout eviction, deduped concurrent starts (N callers, one process).
- **Timeouts** on every request/write; on timeout cancel and mark the connection dead so the pool evicts it.
- Sandbox the env: `Command::env_clear()` + an allowlist; pipe stdio you control.
- Verify: leak test (spawn→drop N children, assert no lingering PIDs); timeout test (a hung child is killed+reaped within budget). Untrusted output: `references/safety-and-ffi.md`.
