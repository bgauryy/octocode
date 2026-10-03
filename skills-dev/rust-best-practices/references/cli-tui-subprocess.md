# CLI, TUI, and subprocesses

Load when you build or review a Rust command-line tool (args, output, exit codes, config, errors, progress, signals, CLI tests), an interactive terminal UI (dashboards, pickers, editors), or code that spawns child processes (language servers, compilers, CLIs) or reads their output. CLI canon: the Command Line Book (`references/sources-and-crates.md`) and clig.dev. TUI stack: `ratatui` (0.30 verified) + `crossterm` backend; `tui-realm`/`cursive` only if you want a retained widget framework.

## CLI: Shape
- `main.rs` stays thin: parse args → call the library → map the result to an exit code. Logic lives in `lib.rs`/a core crate so it's testable without spawning (`references/crate-structure.md`).
- `fn main() -> ExitCode` (or `Result<(), E>` for simple tools). Grounded: `octocode-native/crates/cli/src/main.rs` returns `std::process::ExitCode`. Avoid `std::process::exit` deep in code — it skips destructors (unflushed buffers, temp-file cleanup).
- Exit codes: `0` ok, `1` general failure, `2` usage error (clap's default), documented extras for distinct outcomes (e.g. "found nothing" vs "error"). Scripts depend on them — treat them as API.

## CLI: Arguments (`clap` derive)
- `#[derive(Parser)]` struct + `#[derive(Subcommand)]` enum; doc comments become help. Use `ValueEnum` for closed choices, `value_parser` for typed validation (ranges, paths), `env = "APP_TOKEN"` for env fallback.
- Conventions: `-h/--help`, `-V/--version`, `-v`/`-q` verbosity (`clap-verbosity-flag`), `--` ends flags, `-` means stdin/stdout, `--json` for machine output, `--dry-run` + `--yes` for destructive actions.
- Test the definition once: `#[test] fn verify_cli() { Cli::command().debug_assert(); }`.
- Ship completions and man pages generated at build/release time: `clap_complete`, `clap_mangen`.

## CLI: Output
- **stdout = the result, stderr = everything else** (progress, logs, warnings, prompts). Piping `app | jq` must never get a spinner.
- Machine output is a contract: stable `--json` (serde), one record per line (NDJSON) for streams, no color or decoration.
- Detect the terminal with `std::io::IsTerminal`; color only when stdout is a TTY, honor `NO_COLOR`, `CLICOLOR_FORCE`, and `--color auto|always|never`. `anstream`/`anstyle` (what clap uses) strip ANSI automatically when not a terminal.
- Lock and buffer hot output: `let mut out = BufWriter::new(io::stdout().lock()); writeln!(out, …)?;` — `println!` re-locks and line-flushes each call.
- **Broken pipe:** Rust ignores SIGPIPE, so `println!` panics under `app | head`. Write with `writeln!` and treat `ErrorKind::BrokenPipe` as success (exit 0) — this repo does it in `crates/cli/src/cli/mod.rs`.
- Progress: `indicatif` on stderr, hidden automatically when stderr isn't a TTY; prompts (`dialoguer`/`inquire`) only when stdin is a TTY, with a flag to skip them in CI.

## CLI: Errors & logs
- Apps: `anyhow` + `.context("reading config {path}")` → print `error: …` plus the cause chain, not a `Debug` dump. `color-eyre`/`miette` for rich diagnostics (miette for source-span errors).
- No panics for user mistakes; a panic is a bug — `human-panic` turns it into a friendly report for release builds.
- Diagnostics via `tracing` + `tracing-subscriber` to stderr, level from `-v` or `RUST_LOG`; never log secrets.

## CLI: Config, paths, signals
- Precedence: flags > env > project config > user config > defaults. Paths from `directories`/`etcetera` (XDG on Linux, proper dirs on macOS/Windows) — never hard-code `~/.app`.
- Accept `PathBuf`/`OsString`, not `String` — paths needn't be UTF-8. Resolve relative paths against the cwd once.
- Ctrl-C: `ctrlc` or `tokio::signal::ctrl_c()` → set a flag/cancel token, clean up, exit `130`. Second Ctrl-C exits immediately.
- Startup time is UX: lazy-init heavy state, no network on `--help`/`--version`, and keep the dependency tree lean.

## CLI: Test it
`assert_cmd` + `predicates` (exit code, stdout, stderr) or `trycmd`/`snapbox` for file-driven cases; `insta` for long output; run with `NO_COLOR=1` and a temp `HOME`/cwd. More in `references/testing-and-tooling.md`.

## TUI: Terminal lifecycle — restore it, always
- `ratatui::run(|terminal| app.run(terminal))` (0.30) or `let t = ratatui::init(); … ratatui::restore();` — both enter raw mode + alternate screen and install a **panic hook that restores the terminal**.
- Install other panic/error hooks (`color-eyre::install()`) **before** `ratatui::init()` so the restore hook runs first.
- Every exit path restores: normal quit, `?` errors (restore before printing the error), Ctrl-C, panics. Test by forcing a panic.
- Refuse to start when stdout isn't a TTY (`IsTerminal`) — offer a plain CLI mode instead (CLI sections above).

## TUI: Architecture — state, update, view
- Keep one `App` state struct; `view(&App, &mut Frame)` is a pure function of state (ratatui is immediate-mode: redraw the whole frame, it diffs for you).
- Input → `Action`/`Message` enum → `update(&mut App, Action)` → redraw. Elm/TEA style keeps keybindings, logic, and rendering separable and unit-testable.
- UI modes as an enum (`Mode::Normal | Mode::Search { query } | Mode::Confirm(Op)`), not boolean flags.
- Components: a struct per pane with `handle_event` + `render`; the top level routes focus. Don't let widgets own business logic.

## TUI: Event loop that never freezes
- Never block the UI thread on I/O or heavy work. Run work in threads/tokio tasks; send results back over a channel; the loop `select!`s input events, work results, and a tick.
- Async input: crossterm `EventStream` (feature `event-stream`) with tokio; sync: `event::poll(timeout)` then `event::read()`.
- Redraw on change or at a capped frame rate (~30–60 fps), not in a hot spin. Render cost scales with visible rows — virtualize long lists (`List`/`Table` with `ListState` offset), never format 100k items per frame.
- Handle `Event::Resize`; handle `KeyEventKind::Press` only (Windows also reports releases).

## TUI: Rendering & UX
- Layout with `Layout` + `Constraint` (`Length`, `Min`, `Percentage`, `Fill`); test at 80×24 and tiny sizes — show a "terminal too small" message instead of panicking on zero-area rects.
- Colors: respect `NO_COLOR` and limited terminals; don't encode meaning in color alone. Unicode width: use ratatui's `Line`/`Span` (width-aware) rather than byte slicing strings.
- Keys: `q`/`Esc` quits, `?` shows help, vim + arrow keys, a visible key hint bar. Confirm destructive actions.
- Mouse is opt-in (`EnableMouseCapture`) and must be disabled on exit.

## TUI: Test it
- Unit-test `update` with plain `Action`s — no terminal needed.
- Render tests: `Terminal::new(TestBackend::new(80, 24))`, draw, then `insta::assert_snapshot!(terminal.backend())` to snapshot the buffer.
- Logs can't go to stdout (it's the screen): `tracing` to a file (`tracing-appender`) or `tui-logger` in a pane.

## Subprocess: Kill AND reap — every exit path
- A killed child is not gone until it's **reaped** (`wait`/`try_wait`). Skipping the wait leaves a zombie (Unix) holding a PID slot.
- Track the child (`Option<Child>`), and on stop/error/timeout: attempt graceful shutdown, then `kill()` **and** `wait()`.
- **Graceful-then-forceful**: for protocol servers (LSP), send the protocol shutdown (`shutdown`+`exit`), wait a bounded time (~1–2s) for self-exit, then force-kill. Wire this into *automatic* teardown, not just an explicit call — the leak happens on the path you forgot.
- **`Drop` can't `await`.** In an async `Drop` you can only `start_kill()` + `try_wait()`; a still-running child then depends on the runtime's orphan reaper. Set `Command::kill_on_drop(true)` as the backstop, but don't rely on it as the primary — route real teardown through an explicit async `stop()`.
- **Group-kill** a process and its grandchildren: put it in its own process group (Unix `setsid`/`pre_exec`) or a Windows **Job Object**, and kill the group. `kill()` on the parent alone leaks grandchildren.

## Subprocess: Bound every resource
- **OS memory cap on the child** (not just your buffers): `setrlimit(RLIMIT_AS)` via `pre_exec` (Unix) or a Job Object memory limit (Windows). Without it a runaway child OOMs the *host* — your internal caps don't help.
- **Cap every read from a child**: max frame/message size, max header size, bounded stdout accumulation, a ring buffer for stderr (last N lines). Check a declared length (e.g. `Content-Length`) **before** allocating a buffer for it.
- **Pool bounds**: max live servers, LRU + idle-timeout eviction, dedup concurrent starts so N callers spawn one process.
- **Timeouts** on every request/write; on timeout cancel the request and mark the connection dead so the pool evicts it — don't wedge.

## Subprocess: Async hygiene
- Never call blocking `std::fs`/`std::process` (sync `open`/`read`/`wait`) on an async executor thread — use `tokio::fs`/`tokio::process` or `spawn_blocking`. One sync `File::open` in an async `start()` is an easy-to-miss stall.
- Sandbox the child env: `Command::env_clear()` + an explicit allowlist; pipe stdio you control.

## Subprocess: Verify
- Leak test: spawn→drop N children in a loop, assert no lingering PIDs. Timeout test: a hung child is killed+reaped within the budget. See `references/safety-and-ffi.md` for the untrusted-output angle.
