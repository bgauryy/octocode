Load when debugging a Python program or test.

# Python

## Break and inspect

```bash
python3 -m pdb -c "b app/parse.py:42" -c c -c "p rows[:3], total" -c "w" -c c -c "p total" -c q app/main.py arg1 </dev/null
```

Run `-c` commands in order and close stdin with `</dev/null` so pdb exits when they run out. For a module or console script, use `python3 -m pdb -m package.module`.

| Need | pdb command |
|---|---|
| Continue / stack / quit | `c` / `w` / `q` |
| Conditional breakpoint | `b file.py:42, total > 100` |
| Function breakpoint | `b module.function` |
| Inspect | `pp value` (pretty print), `p locals()` |
| Move frame / step | `up`, `down` / `n` (next line), `s` (step in) |
| Observe every stop | `display expr` |

## Exception

An uncaught exception puts pdb into post-mortem at the failing frame, and the remaining `-c` commands run there:

```bash
python3 -m pdb -c c -c "p done, total" -c up -c "p row" -c q app/report.py </dev/null
```

List comprehensions and generators are their own frames; when `p` reports a name as undefined, move `up` or `down`.

## pytest

- `pytest -x -l tests/test_parse.py::test_empty` stops at the first failure and prints each frame's locals in the traceback; this often answers the question without a debugger.
- `--pdb` and `breakpoint()` open an interactive prompt; use them only with pdb commands piped in: `printf 'p total\nw\nq\n' | pytest -x --pdb tests/test_parse.py -p no:cacheprovider`.
- `-s` shows prints from the code under test; `--lf` reruns the last failures.

## Hang

- Add `faulthandler.dump_traceback_later(10, exit=True)` near the start, or run `python3 -X faulthandler` and send `SIGABRT`: every thread's stack prints to stderr.
- With py-spy installed: `py-spy dump --pid <pid>` prints the live stacks, local variables with `--locals`. On macOS it needs `sudo`.
- `pytest --timeout=30` (pytest-timeout) dumps the stacks when a test hangs.

## Imports and environment

When a fix seems to have no effect, check which file actually runs: `python3 -c "import pkg, sys; print(pkg.__file__, sys.executable)"`. A stale virtual environment or an installed copy of the package shadowing the source is a common cause.
