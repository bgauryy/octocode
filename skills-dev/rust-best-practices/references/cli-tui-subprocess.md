# CLI, terminal UI and child processes

Load for arguments, output streams, exits, terminal state, signals or subprocess lifetimes. Reuse the project's parser/runtime/UI stack; no named crate below is a required dependency.

## Command-line boundary

| Concern | Action and check |
|---|---|
| Arguments | Preserve documented flags, paths and exits. Accept OS paths with `PathBuf`/`OsString`; use typed validation. For clap, `CommandFactory::command().debug_assert()` checks parser consistency. |
| Streams | Keep results on stdout and diagnostics/progress on stderr. Machine output must not contain prompts, color or spinners. For stdio protocols, reserve stdout entirely for protocol messages. |
| Output volume | Buffer writes when useful. Preserve complete requested records; paginate/stream or explicitly report a policy limit. Do not silently clip rows or replace evidence with ellipses. |
| Broken pipe | Write through fallible I/O, handle `BrokenPipe` according to the CLI contract and avoid a print panic. Early consumer closure often means a successful pipeline, but preserve the documented policy. |
| Color and prompts | Detect the relevant stream's terminal status and follow explicit flags/environment conventions. Prompt only in an interactive flow; provide documented noninteractive behavior. |
| Config | Follow the established config owner and precedence. Do not independently load env/home/project config at each adapter. |
| Shutdown | Prefer returning an exit status after cleanup; deep `process::exit` skips destructors. Signals request owned shutdown; an immediate second-signal exit is a product policy, not a universal requirement. |
| Startup | Avoid unnecessary heavy initialization/network for help/version. Test the real binary's exit status and both streams. |

Use argument APIs rather than a shell command string. If shell interpretation is explicitly required, treat untrusted arguments as data with a verified quoting mechanism. Configure child env/cwd directly instead of mutating process globals for tests.

## Terminal ownership

- Establish one terminal-state owner and restore raw mode, alternate screen, cursor and mouse capture on every supported exit path. Audit normal return, early errors, cancellation and panic hooks; `Drop` alone does not cover abort or forced termination.
- Use the locked UI library's initialization/restoration contract; do not assume a newer helper exists. Install/chains hooks according to that version's requirements.
- Keep input/action handling, application state and rendering separately testable. Move blocking/long-running work off the UI event loop and redraw for actual changes or an owner-defined refresh policy.
- Handle resize, tiny/zero regions, key-event differences, display width and Unicode. Test state transitions and rendering with the library's test backend when available.

## Child process lifecycle

1. Assign an explicit owner before spawning. Keep a handle and decide stdout/stderr draining, stdin closure, cancellation, signal handling and descendant ownership.
2. Drain piped stdout and stderr concurrently where needed. Waiting for exit while a child blocks on a full pipe can deadlock. An accumulating `output()` call is appropriate only when its memory policy fits.
3. On completion, inspect the status and reap the child. On error/cancellation, perform the protocol's graceful shutdown if available, then apply the owner's escalation policy and wait for termination.
4. Distinguish APIs: std `Child::kill` sends termination and still needs `wait`; Tokio `Child::kill().await` also waits, while `start_kill` does not. Check the actual API before claiming reaping is complete.
5. Treat kill-on-drop as a backstop. Dropping a handle is not a portable guarantee that work and descendants have stopped. Async shutdown must be explicit because `Drop` cannot await.
6. If descendants are in scope, use an owned process group/job or an established process-tree abstraction. Preserve ownership while signaling; reaped/recycled process IDs must not become fresh kill targets. Never kill by process-name pattern.

## Resource policy and verification

Use caller/protocol deadlines and resource limits from the owning layer, with explicit failure/unread scope. Do not introduce guessed frame caps, fixed shutdown sleeps or truncated stderr as complete evidence. Validate declared frame lengths before allocation. RSS observation, address-space limits and OS job/container limits enforce different things; choose according to the threat model and platform.

Test failure to start, nonzero exit, full pipes, consumer closure, malformed frames, cancellation during work and shutdown while descendants exist. Verify the owned processes actually exited and were reaped, not only that the request returned. Use synchronization and injected policy deadlines rather than arbitrary sleeps.

Sources: [Rust CLI Book](https://rust-cli.github.io/book/), [std Child](https://doc.rust-lang.org/std/process/struct.Child.html), [Tokio process caveats](https://docs.rs/tokio/latest/tokio/process/#caveats), [Tokio Child](https://docs.rs/tokio/latest/tokio/process/struct.Child.html).
