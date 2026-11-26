Load when comparing an old and a new build on a host that other work shares, or measuring an interactive terminal program reproducibly.

# Comparable runs

## Compare builds on a shared host

Keep the old build runnable beside the new one, where it still resolves its dependencies, and run the [run comparison](../scripts/compare-runs.mjs) from the directory the commands expect:

```bash
node "$skill/scripts/compare-runs.mjs" --old "$old_command" --new "$new_command" --rounds 5 --warmup 1
```

It runs both commands through `/bin/sh` in alternating rounds (old, new, old, new) and prints JSON: for each metric (`wallMs`, `cpuMs`, and `parsed` when `--parse '<regex>'` captures a number the command prints) every round's value, the median, the spread and a `verdict`. `lower` or `higher` means the new median differs from the old by more than both spreads; `within-spread` means the rounds do not separate the builds. Exit 1 means a round failed; its stderr tail is in the error. Other work on the host moves wall time and RSS between rounds more than many fixes do, so decide on `cpuMs` and on deterministic counters: bytes allocated or calls made per operation, file and bundle sizes. Rounds measured another way go in with `--file` (JSON, JSON lines or CSV rows with `variant` and metric columns). Report every round's numbers, not only the best. A fix's test is proven at base as the [entry's Fix step](../SKILL.md#procedure) says.

## Drive an interactive program

A terminal UI or REPL runs reproducibly in a private terminal server (`tmux -L <name>`, never the user's own server) with an isolated home and a scripted local backend in place of the network service (a fake model or API whose reply length and pacing the input chooses). End each operation with a unique marker in the output and wait for that marker plus the program's idle indicator before the next one; poll the screen (`tmux capture-pane -p`) rather than sleeping. Take process CPU and RSS (`ps -o time=,rss= -p "$pid"`) around each phase, and the runtime's own profiles from a separate run with the same script, since profilers inflate the counters. Stop the server and the backend when done.
