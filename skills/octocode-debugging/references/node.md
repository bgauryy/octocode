Load when a Node or TypeScript program needs a breakpoint, exception stop or hang stack without an interactive debugger.

# Node and TypeScript

For an unattended launch, [node-break.mjs](../scripts/node-break.mjs) runs the program under the inspector, prints each pause as JSON, resumes and exits with the program’s exit code.

```bash
node "$skill/scripts/node-break.mjs" --break src/parse.ts:42 -- src/main.ts arg1
node "$skill/scripts/node-break.mjs" --break src/parse.ts:42:"rows.length > 100" --eval "rows[0]" --eval total -- src/main.ts
node "$skill/scripts/node-break.mjs" --exceptions all --max-hits 5 -- --enable-source-maps dist/main.js
```

Everything after `--` is what you would pass to `node`: Node flags, the script and its arguments. Node 22.18 and later strip erasable TypeScript syntax directly; use the project loader/build for unsupported transforms or tsconfig path mappings.

| Option | Effect |
|---|---|
| `--break file:line[:condition]` | Pause at that line, when the condition holds; repeatable, one per line |
| `--eval expression` | Print these at each pause instead of the top frame's locals; repeatable |
| `--exceptions all\|uncaught` | `uncaught` filters noise. `all` also reveals throws caught by the runtime or application. |
| `--steps n` | After each hit, step over n lines and report each |
| `--max-hits n` | After n pauses, stop pausing and let the program finish. Default: every pause; bound a hot loop with a condition or this flag |
| `--timeout s` | For a hang: request stacks from attached JS targets, then terminate the launched child. Default: none, the program runs to its end |

Each output line has `event`: `hit`, `step`, `exception`, `timeout`, `max-hits`, `unbound` or `exit`. Pauses carry `thread` (`main` or `worker N`), `at`, `values` and `stack`, printed whole; exceptions add `exception`. An object value is V8's preview, where `…` marks omitted properties: `--eval` the property to read it. Redirect stdout to a file when a breakpoint fires many times, then read it in ranges. `unbound` names a breakpoint that matched no loaded code: a wrong path or line, or a file that was never loaded. If the breakpoint bound but recorded no hits, check the executed worker, generated location and condition before inferring which path ran.

## Tests and transformed code

A test runner or bundler may transform the file before it runs. Check the loaded script and its source map if a breakpoint misses. On a disposable reproduction, a temporary `debugger;` statement at the suspect boundary can stop in the generated code so you can inspect values with `--eval`.

```bash
node "$skill/scripts/node-break.mjs" --eval got --eval input -- node_modules/vitest/vitest.mjs run --pool=threads test/parse.test.ts
node "$skill/scripts/node-break.mjs" --eval got -- node_modules/jest/bin/jest.js --runInBand test/parse.test.ts
```

Run the test runner's file directly from `node_modules`, not through `npx` or a package script, so the inspector attaches to the runner. Worker threads attach automatically; child processes do not, so keep tests in threads (`--pool=threads` for Vitest) or in the main process (`--runInBand` for Jest). Remove each `debugger;` statement afterwards.

## Without the script

- `node --stack-trace-limit=50` lengthens stack traces; `--enable-source-maps` maps built files back to sources.
- `node --trace-uncaught` prints where an uncaught non-error value was thrown; `--trace-warnings` does the same for warnings, and `--unhandled-rejections=strict` turns a silent rejection into a crash.
- A hang with an idle CPU is usually a promise that never settles or an open handle: `--timeout` shows where the main thread waits, and `process.getActiveResourcesInfo()` lists what keeps the process alive.

To attach Chrome DevTools or another inspector client to a launched or running process, load [Node inspector and Chrome DevTools](node-devtools.md).
