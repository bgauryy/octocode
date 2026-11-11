Load when debugging a compiled program: C, C++, Rust, Swift or Go.

# Native programs

## Break and inspect

```bash
lldb -b -o "b parse.c:42" -o run -o "frame variable" -o bt -o c -- ./prog arg1 arg2
gdb -batch -ex "break parse.c:42" -ex run -ex "info locals" -ex bt -ex continue --args ./prog arg1 arg2
```

`-o` (lldb) and `-ex` (gdb) run in order, then the debugger quits; the program's own output is interleaved. Useful commands:

| Need                         | lldb                                           | gdb                           |
| ---------------------------- | ---------------------------------------------- | ----------------------------- |
| Break when a condition holds | `breakpoint set -f parse.c -l 42 -c 'n > 100'` | `break parse.c:42 if n > 100` |
| Break on a function          | `b parse_header`                               | `break parse_header`          |
| Locals, one variable         | `frame variable`, `p items[1]`                 | `info locals`, `p items[1]`   |
| Stack, then a caller frame   | `bt 10`, `frame select 2`                      | `bt 10`, `frame 2`            |
| Every thread                 | `thread backtrace all`                         | `thread apply all bt`         |
| Next line, into a call       | `next`, `step`                                 | `next`, `step`                |
| Stop when memory changes     | `watchpoint set variable total`                | `watch total`                 |

To see a value change across iterations, repeat `-o c -o "p total"` pairs; each `c` stops at the next hit. The short `b` takes no options: use `breakpoint set` for a condition.

## Crash

In batch mode lldb stops running `-o` commands when the program crashes. Put the commands for that case after `-k` (gdb keeps running `-ex` commands):

```bash
lldb -b -o run -k "bt 10" -k "frame select 1" -k "frame variable" -- ./prog
```

The top frame is often a library (`strlen`, `memcpy`); select the first frame in your code and print its arguments. For a crash that happens only outside the debugger, enable core files (`ulimit -c unlimited`), reproduce, then load the core: `lldb -c /cores/core.<pid> ./prog -b -o "bt all"` or `gdb ./prog core -batch -ex "thread apply all bt"`. Address sanitizer often names the bug faster than a debugger: rebuild with `-fsanitize=address,undefined -g` and run normally.

## Running process and hangs

Attach, print, detach; the process keeps running afterwards.

```bash
lldb -p <pid> -b -o "thread backtrace all" -o detach
gdb -p <pid> -batch -ex "thread apply all bt"
sample <pid> 3          # macOS: 3-second call tree of every thread, no debugger needed
```

Attaching may need permission: on Linux, `ptrace_scope` or root; on macOS, a binary signed without the get-task-allow entitlement refuses. Run the program under the debugger from the start instead.

## Rust

`RUST_BACKTRACE=1` (or `full`) prints a panic's stack without a debugger. `rust-lldb` and `rust-gdb` wrap the debuggers with Rust pretty printers; the commands above work unchanged. Test binaries are under `target/debug/deps/`; `cargo test --no-run` prints their paths. Break on a panic with `b rust_panic`.

## Go

Send `SIGQUIT` (`kill -QUIT <pid>`) to a hung Go program: it prints every goroutine's stack and exits; `GOTRACEBACK=all` widens a crash dump the same way. With Delve installed, `dlv exec ./prog --headless` needs a client, so prefer `dlv debug --init <file>` with commands such as `break main.go:42`, `continue`, `locals`, `stack`, `exit` in the file.
