Load when a C, C++ or Rust binary needs a breakpoint, watchpoint or stack in batch mode. For a crash, a core dump, a hang or a process that is already running, load [native crashes, cores and hangs](native-crash.md).

# Native programs

## Break and inspect

Keep the original binary and matching symbols for an incident. For a separate stepping build, enable debug info and reduce optimization when it obscures locals. Preserve production optimization for profiling. In unattended runs use batch mode with a finite workload. Close debugger stdin unless the workload itself requires input.

```bash
lldb -b -o "b parse.c:42" -o run -o "frame variable" -o bt -o c -- ./prog arg1 arg2
gdb -batch -ex "break parse.c:42" -ex run -ex "info locals" -ex bt -ex continue --args ./prog arg1 arg2
```

`-o` (lldb) and `-ex` (gdb) run in order, then the debugger quits; the program's own output is interleaved. Useful commands:

| Need | lldb | gdb |
|---|---|---|
| Break when a condition holds | `breakpoint set -f parse.c -l 42 -c 'n > 100'` | `break parse.c:42 if n > 100` |
| Break on a function | `b parse_header` | `break parse_header` |
| Locals, one variable | `frame variable`, `p items[1]` | `info locals`, `p items[1]` |
| Stack, then a caller frame | `bt 10`, `frame select 2` | `bt 10`, `frame 2` |
| Every thread | `thread backtrace all` | `thread apply all bt` |
| Next line, into a call | `next`, `step` | `next`, `step` |
| Stop when memory changes | `watchpoint set variable total` | `watch total` |

To see a value change across iterations, repeat `-o c -o "p total"` pairs; each `c` stops at the next hit. The short `b` takes no options: use `breakpoint set` for a condition.

## Rust

`RUST_BACKTRACE=1` (or `full`) prints a panic's stack without a debugger. `rust-lldb` and `rust-gdb` wrap the debuggers with Rust pretty printers; the commands above work unchanged. Test binaries are under `target/debug/deps/`; `cargo test --no-run` prints their paths. Break on a panic with `b rust_panic`.

For Go goroutines, pprof and Delve, use [Go diagnostics](go.md).
