# TUI — full-screen terminal apps with ratatui

Load when building or reviewing an interactive terminal UI (dashboards, pickers, editors). Why: a TUI owns the terminal — get setup/teardown, the event loop, or state layout wrong and you leave users with a broken shell, flicker, or a frozen UI. Stack: `ratatui` (0.30 verified) + `crossterm` backend; `tui-realm`/`cursive` only if you want a retained widget framework.

## Terminal lifecycle — restore it, always
- `ratatui::run(|terminal| app.run(terminal))` (0.30) or `let t = ratatui::init(); … ratatui::restore();` — both enter raw mode + alternate screen and install a **panic hook that restores the terminal**.
- Install other panic/error hooks (`color-eyre::install()`) **before** `ratatui::init()` so the restore hook runs first.
- Every exit path restores: normal quit, `?` errors (restore before printing the error), Ctrl-C, panics. Test by forcing a panic.
- Refuse to start when stdout isn't a TTY (`IsTerminal`) — offer a plain CLI mode instead (`references/cli.md`).

## Architecture — state, update, view
- Keep one `App` state struct; `view(&App, &mut Frame)` is a pure function of state (ratatui is immediate-mode: redraw the whole frame, it diffs for you).
- Input → `Action`/`Message` enum → `update(&mut App, Action)` → redraw. Elm/TEA style keeps keybindings, logic, and rendering separable and unit-testable.
- UI modes as an enum (`Mode::Normal | Mode::Search { query } | Mode::Confirm(Op)`), not boolean flags.
- Components: a struct per pane with `handle_event` + `render`; the top level routes focus. Don't let widgets own business logic.

## Event loop that never freezes
- Never block the UI thread on I/O or heavy work. Run work in threads/tokio tasks; send results back over a channel; the loop `select!`s input events, work results, and a tick.
- Async input: crossterm `EventStream` (feature `event-stream`) with tokio; sync: `event::poll(timeout)` then `event::read()`.
- Redraw on change or at a capped frame rate (~30–60 fps), not in a hot spin. Render cost scales with visible rows — virtualize long lists (`List`/`Table` with `ListState` offset), never format 100k items per frame.
- Handle `Event::Resize`; handle `KeyEventKind::Press` only (Windows also reports releases).

## Rendering & UX
- Layout with `Layout` + `Constraint` (`Length`, `Min`, `Percentage`, `Fill`); test at 80×24 and tiny sizes — show a "terminal too small" message instead of panicking on zero-area rects.
- Colors: respect `NO_COLOR` and limited terminals; don't encode meaning in color alone. Unicode width: use ratatui's `Line`/`Span` (width-aware) rather than byte slicing strings.
- Keys: `q`/`Esc` quits, `?` shows help, vim + arrow keys, a visible key hint bar. Confirm destructive actions.
- Mouse is opt-in (`EnableMouseCapture`) and must be disabled on exit.

## Test it
- Unit-test `update` with plain `Action`s — no terminal needed.
- Render tests: `Terminal::new(TestBackend::new(80, 24))`, draw, then `insta::assert_snapshot!(terminal.backend())` to snapshot the buffer.
- Logs can't go to stdout (it's the screen): `tracing` to a file (`tracing-appender`) or `tui-logger` in a pane.

Next: for args/exit codes/config shared with the non-interactive mode, load `references/cli.md`; for the async pitfalls behind a frozen loop, `references/gotchas.md`.
