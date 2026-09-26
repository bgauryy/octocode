# CLI — command-line apps that behave like good Unix citizens

Load when building or reviewing a Rust command-line tool: args, output, exit codes, config, errors, progress, signals, or CLI tests. Why: users and scripts judge a CLI by its contract (stdout vs stderr, exit codes, pipes, `--help`), not its internals. Canon: the Command Line Book (`references/canonical-sources.md`) and clig.dev.

## Shape
- `main.rs` stays thin: parse args → call the library → map the result to an exit code. Logic lives in `lib.rs`/a core crate so it's testable without spawning (`references/crate-boundaries.md`).
- `fn main() -> ExitCode` (or `Result<(), E>` for simple tools). Grounded: `octocode-native/crates/runtime/src/main.rs` returns `std::process::ExitCode`. Avoid `std::process::exit` deep in code — it skips destructors (unflushed buffers, temp-file cleanup).
- Exit codes: `0` ok, `1` general failure, `2` usage error (clap's default), documented extras for distinct outcomes (e.g. "found nothing" vs "error"). Scripts depend on them — treat them as API.

## Arguments (`clap` derive)
- `#[derive(Parser)]` struct + `#[derive(Subcommand)]` enum; doc comments become help. Use `ValueEnum` for closed choices, `value_parser` for typed validation (ranges, paths), `env = "APP_TOKEN"` for env fallback.
- Conventions: `-h/--help`, `-V/--version`, `-v`/`-q` verbosity (`clap-verbosity-flag`), `--` ends flags, `-` means stdin/stdout, `--json` for machine output, `--dry-run` + `--yes` for destructive actions.
- Test the definition once: `#[test] fn verify_cli() { Cli::command().debug_assert(); }`.
- Ship completions and man pages generated at build/release time: `clap_complete`, `clap_mangen`.

## Output
- **stdout = the result, stderr = everything else** (progress, logs, warnings, prompts). Piping `app | jq` must never get a spinner.
- Machine output is a contract: stable `--json` (serde), one record per line (NDJSON) for streams, no color or decoration.
- Detect the terminal with `std::io::IsTerminal`; color only when stdout is a TTY, honor `NO_COLOR`, `CLICOLOR_FORCE`, and `--color auto|always|never`. `anstream`/`anstyle` (what clap uses) strip ANSI automatically when not a terminal.
- Lock and buffer hot output: `let mut out = BufWriter::new(io::stdout().lock()); writeln!(out, …)?;` — `println!` re-locks and line-flushes each call.
- **Broken pipe:** Rust ignores SIGPIPE, so `println!` panics under `app | head`. Write with `writeln!` and treat `ErrorKind::BrokenPipe` as success (exit 0) — this repo does it in `crates/runtime/src/cli/mod.rs`.
- Progress: `indicatif` on stderr, hidden automatically when stderr isn't a TTY; prompts (`dialoguer`/`inquire`) only when stdin is a TTY, with a flag to skip them in CI.

## Errors & logs
- Apps: `anyhow` + `.context("reading config {path}")` → print `error: …` plus the cause chain, not a `Debug` dump. `color-eyre`/`miette` for rich diagnostics (miette for source-span errors).
- No panics for user mistakes; a panic is a bug — `human-panic` turns it into a friendly report for release builds.
- Diagnostics via `tracing` + `tracing-subscriber` to stderr, level from `-v` or `RUST_LOG`; never log secrets.

## Config, paths, signals
- Precedence: flags > env > project config > user config > defaults. Paths from `directories`/`etcetera` (XDG on Linux, proper dirs on macOS/Windows) — never hard-code `~/.app`.
- Accept `PathBuf`/`OsString`, not `String` — paths needn't be UTF-8. Resolve relative paths against the cwd once.
- Ctrl-C: `ctrlc` or `tokio::signal::ctrl_c()` → set a flag/cancel token, clean up, exit `130`. Second Ctrl-C exits immediately.
- Startup time is UX: lazy-init heavy state, no network on `--help`/`--version`, and keep the dependency tree lean.

## Test it
`assert_cmd` + `predicates` (exit code, stdout, stderr) or `trycmd`/`snapbox` for file-driven cases; `insta` for long output; run with `NO_COLOR=1` and a temp `HOME`/cwd. More in `references/testing.md`.

Next: for full-screen interactive apps, load `references/tui.md`; for release binaries and size, `references/build-profiles.md`.
