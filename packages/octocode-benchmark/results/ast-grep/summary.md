# Octocode Benchmarks Summary

Recorded: 2026-06-20 16:39:47 IDT

## Result

The deterministic ast-grep comparison passed for every Octocode-supported
upstream scenario that was executed:

- Compared scenarios: 6
- Match-count differences: 0
- Errors: 0
- Skipped: 1, Swift/Alamofire, because Octocode structural search does not
  support Swift yet
- Artifact: `packages/octocode-benchmark/target/ast-grep-upstream/latest.json`

Internal Octocode benchmark suite also passed:

- Support matrix: 150 extensions checked
- AST benchmark: 33 grammars, 61 declared structural extensions
- LSP benchmark: 18 language checks
- Minify benchmark: 148 samples
- CLI metadata benchmark: 14 tools, 21 commands, 60 CLI help/scheme checks
- Final status: all 5 benchmark checks passed

## What Was Compared

The comparison used ast-grep's upstream benchmark scenario list as a shared
real-repository corpus. Upstream ast-grep currently ships an outline/Claude
agent benchmark, not a dedicated structural grep microbenchmark. For this
Octocode-vs-ast-grep run, the same scenario repositories were reused as a
deterministic structural-search corpus.

Plain English: we reused only `benchmarks/outline-agent-scenarios.json`, then
ran our own deterministic structural grep comparison. We did not run upstream's
`outline_claude_benchmark.py`, Claude calls, outline scoring, or agent-answer
rubric.

Source checked:

- `ast-grep/ast-grep` benchmark folder:
  https://github.com/ast-grep/ast-grep/tree/main/benchmarks
- Upstream scenario list reused:
  https://github.com/ast-grep/ast-grep/blob/main/benchmarks/outline-agent-scenarios.json
- Upstream runner not reused:
  https://github.com/ast-grep/ast-grep/blob/main/benchmarks/outline_claude_benchmark.py
- Upstream outline benchmark description:
  https://github.com/ast-grep/ast-grep/blob/main/benchmarks/outline-benchmark.md
- Scenario manifest used by this package:
  `packages/octocode-benchmark/benchmark/ast-grep/upstream-outline-scenarios.json`

## Executed Commands

Internal suite:

```bash
/tmp/octocode-node-v24.17.0/bin/node packages/octocode-benchmark/benchmark/run-all.mjs
```

Deterministic all-scenario structural comparison:

```bash
AST_GREP_BIN=/tmp/octocode-ast-grep-cli-check/bin/ast-grep \
  /tmp/octocode-node-v24.17.0/bin/node \
  packages/octocode-benchmark/benchmark/ast-grep/compare-upstream-scenarios.mjs \
  --sync-repos \
  --files-per-scenario 1 \
  --warmups 1 \
  --repeats 3
```

Smoke verify after benchmark-runner doc/output-note changes:

```bash
AST_GREP_BIN=/tmp/octocode-ast-grep-cli-check/bin/ast-grep \
  /tmp/octocode-node-v24.17.0/bin/node \
  packages/octocode-benchmark/benchmark/ast-grep/compare-upstream-scenarios.mjs \
  --repo-dir packages/octocode-benchmark/target/ast-grep-upstream/repos \
  --output-dir packages/octocode-benchmark/target/verify-smoke \
  --scenario gin-middleware-routing \
  --files-per-scenario 1 \
  --warmups 1 \
  --repeats 1
```

## Benchmark Matrix

Measured values are median wall-clock milliseconds after 1 warmup run.
Bars show relative throughput. Higher is better; lower `ms` is better.

```text
Octocode raw native  ████████████████████    5.0 ms median  │  3.0x faster  │  6/6 matched
ast-grep CLI         ███████░░░░░░░░░░░░░   15.1 ms median  │  baseline     │  6/6 matched
```

`Octocode raw native` means the direct Rust/NAPI `structuralSearchFiles`
matcher: parse and match only, with no tool validation, sanitizer, pagination,
JSON shaping, or Node CLI startup.

This is not the formal upstream ast-grep benchmark. Upstream's
[benchmark](https://github.com/ast-grep/ast-grep/blob/main/benchmarks/outline-benchmark.md)
evaluates `ast-grep outline` as an agent exploration tool using real
`claude -p` runs. This Octocode benchmark reuses ast-grep's
[outline scenario list](https://github.com/ast-grep/ast-grep/blob/main/benchmarks/outline-agent-scenarios.json)
as a deterministic same-corpus structural grep comparison.

| Benchmark lane | What it includes | Median timing | vs ast-grep CLI | Winner / DX take |
| --- | --- | ---: | ---: | --- |
| ast-grep CLI | External Rust CLI baseline | 13.6-17.1ms | 1.0x | Fast standalone structural grep |
| Octocode raw native | Direct matcher path | 4.0-6.6ms | 2.4x-4.1x faster | Fastest matcher path |
| Octocode localSearchCode tool | Validation, sanitization, pagination, result shaping | 91.6ms-3.32s | 5.6x-210.7x slower | Agent-safe result path |
| Octocode search CLI | Public CLI with fresh Node startup | 1.62s-4.98s | 98.8x-316.2x slower | Human CLI path; startup dominates |

Benchmark size and correctness:

Plain English: we took ast-grep's benchmark scenario repo list, picked the same
deterministic file from each supported repo, asked both tools to find the same
AST node kind in that file, verified the match counts were identical, then timed
the median run.

| Benchmark size / coverage | Result |
| --- | --- |
| Benchmark type | Same-corpus structural grep timing and match-count parity |
| ast-grep upstream corpus reused | VS Code, Excalidraw, Django, Tokio, OkHttp, Gin |
| Query shape checked | Broad AST node-kind searches: `call_expression`, `call`, and `method_invocation` |
| Structural match parity | 6/6 supported scenarios matched |
| Count differences | 0 |
| Errors | 0 |
| Internal suite | 150 extensions, 33 AST grammars, 18 LSP checks, 148 minify samples, CLI metadata gate |

Per-scenario timing:

| Scenario | ast-grep CLI | Octocode raw native | Raw vs ast-grep | localSearchCode tool | octocode search CLI |
| --- | ---: | ---: | ---: | ---: | ---: |
| VS Code | 16.4ms | 4.0ms | 4.1x faster | 91.6ms | 1620.4ms |
| Excalidraw | 14.4ms | 5.0ms | 2.9x faster | 656.5ms | 2317.7ms |
| Django | 13.6ms | 4.9ms | 2.8x faster | 616.1ms | 2320.0ms |
| Tokio | 15.8ms | 6.6ms | 2.4x faster | 3320.3ms | 4982.0ms |
| OkHttp | 17.1ms | 5.0ms | 3.4x faster | 727.5ms | 2314.3ms |
| Gin | 13.9ms | 5.6ms | 2.5x faster | 1393.9ms | 3007.4ms |

Warmup materially reduced Node/native first-run cost. Example:
`localSearchCode` on VS Code was 1494.8ms during warmup and 91.6ms measured.
The public `octocode search` CLI still pays process startup for every measured
repeat, so warmup cannot remove that overhead.

## Important Finding

The default larger corpus run, `--files-per-scenario 80`, was attempted and
stopped after about 12 minutes. A macOS `sample` showed the process was
CPU-bound in `mask_sensitive_data` regex scanning during result shaping, not in
the structural matcher. This means large-corpus optimization should focus on
sanitizer/result-shaping behavior in the tool/CLI lanes, while `octocode raw
native` should be used to isolate matcher performance.

See `packages/octocode-benchmark/output/comparison.md` for the full lane table,
hashes, versions, deterministic controls, and rerun instructions.
