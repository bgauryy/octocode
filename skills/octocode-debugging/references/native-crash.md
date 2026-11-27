Load when a native program crashes, leaves a core, hangs, or must be inspected while running. Breakpoints and batch-mode stepping are in [native programs](native.md).

# Native crashes, cores and hangs

## Crash

In batch mode lldb stops running `-o` commands when the program crashes. Put the commands for that case after `-k` (gdb keeps running `-ex` commands):

```bash
lldb -b -o run -k "bt 10" -k "frame select 1" -k "frame variable" -- ./prog
```

The top frame is often a library (`strlen`, `memcpy`); select the first frame in your code and print its arguments. For a crash that happens only outside the debugger, preserve an available core with its matching binary and symbols, then load it with `lldb -c /cores/core.<pid> ./prog -b -o "bt all"` or `gdb ./prog core -batch -ex "thread apply all bt"`. For suspected memory corruption or undefined behavior, reproduce with `-fsanitize=address,undefined -g` on a supported C/C++ build.

## Running process and hangs

Debugger attach pauses the target. Prefer periodic sampling when the live service must keep serving. If a pause is acceptable, capture all thread stacks and detach explicitly.

```bash
lldb -p <pid> -b -o "thread backtrace all" -o detach
gdb -p <pid> -batch -ex "thread apply all bt" -ex detach
sample <pid> 3          # macOS: 3-second call tree of every thread, no debugger needed
```

Linux ptrace policy or macOS signing and entitlements can restrict attachment. If attach is denied, inspect an existing dump or launch a separate reproduction under the debugger.
