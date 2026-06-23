# Octocode vs ast-grep — Structural Grep Benchmark

## What This Is

A deterministic, reproducible comparison of Octocode's structural grep engine
against the [ast-grep](https://github.com/ast-grep/ast-grep) CLI across six
real open-source repositories (TypeScript, Python, Rust, Java, Go, TSX).

**Why it exists.** ast-grep ships no formal public structural grep benchmark.
Their own contributing docs say *"ast-grep's benchmarking suite is not well
developed yet. The result may fluctuate too much."* The outline benchmark they
do publish evaluates an agent task (claude -p sessions), not raw structural
search speed. This benchmark fills that gap using the same corpus.

**What it measures.**
- Structural match correctness: do both tools find identical match counts?
- Timing across four layers of the Octocode stack, so you can see exactly where
  time is spent — raw matcher vs. agent-safe tool path vs. public CLI.

---

## Results (2026-06-22)

**80 files per scenario · ast-grep 0.44.0 · octocode v2.0.0 · 3 repeats + 1 warmup**

```
Octocode raw native  ████████████████████   17.1 ms median  │  2.0x faster  │  6/6 matched
ast-grep CLI         ██████████░░░░░░░░░░   34.6 ms median  │  baseline     │  6/6 matched
```

Correctness: **6 of 6 scenarios matched — zero count differences, zero errors.**

### Per-Scenario Breakdown

| Scenario | Language | Files | Matches | ast-grep CLI | Octocode native | vs ast-grep | Result |
|---|---|---:|---:|---:|---:|---|---|
| VS Code | TypeScript | 80 | 0 | 28.2 ms | 0.5 ms | **57x faster** | ✓ MATCH |
| Django | Python | 80 | 337 | 30.8 ms | 10.6 ms | **2.9x faster** | ✓ MATCH |
| OkHttp | Java | 71 | 1,072 | 33.5 ms | 12.1 ms | **2.8x faster** | ✓ MATCH |
| Tokio | Rust | 80 | 2,164 | 35.7 ms | 22.1 ms | **1.6x faster** | ✓ MATCH |
| Gin | Go | 80 | 7,252 | 53.2 ms | 53.0 ms | **parity** | ✓ MATCH |
| Excalidraw | TSX | 80 | 8,062 | 47.1 ms | 58.3 ms | ~parity | ✓ MATCH |
| Alamofire | Swift | — | — | — | — | SKIP (no Swift grammar) | — |

### All Four Timing Lanes

The benchmark measures the Octocode stack top-to-bottom so you can see exactly
where overhead lives:

| Lane | What it includes | Median | vs ast-grep |
|---|---|---:|---|
| **ast-grep CLI** | External Rust CLI, process startup | 34.6 ms | 1.0x (baseline) |
| **Octocode raw native** | Rust/NAPI matcher only; no validation, no result shaping | 17.1 ms | **2.0x faster** |
| **Octocode localSearchCode tool** | + path validation, sanitizer, pagination, YAML shaping | 1,005 ms | 29x slower than ast-grep |
| **Octocode grep CLI** | + Node process startup, CLI routing, JSON output | 1,372 ms | 40x slower than ast-grep |

`localSearchCode` and `grep CLI` are slower by design — they are the agent-safe
paths. Use `octocode raw native` to compare matcher performance only.

### Result-Shaping Overhead Scales With Match Count

The sanitizer and result-shaper process every match. This makes the tool paths
proportional to match count, not file count:

| Scenario | Matches | native | localSearchCode tool | Overhead ratio |
|---|---:|---:|---:|---:|
| VS Code | 0 | 0.5 ms | 5.3 ms | 10× |
| Django | 337 | 10.6 ms | 215.4 ms | 20× |
| OkHttp | 1,072 | 12.1 ms | 695.0 ms | 57× |
| Tokio | 2,164 | 22.1 ms | 1,315.7 ms | 60× |
| Gin | 7,252 | 53.0 ms | 4,608.7 ms | 87× |
| Excalidraw | 8,062 | 58.3 ms | 4,954.7 ms | 85× |

**This is the next optimization target.** The matcher itself is fast and
competitive with ast-grep. The sanitizer's per-match regex scanning dominates at
> 1,000 matches.

### Full Raw Table

Values are median wall-clock ms after warmup. Hash = first 8 chars of corpus SHA-256.

| Scenario | Kind | Files | Hash | Matches | Lane | Warm ms | ms |
|---|---|---:|---|---:|---|---:|---:|
| vscode-extension-host | call_expression | 80 | 1cc94248 | 0 | ast-grep CLI | 28.8 | 28.2 |
| vscode-extension-host | call_expression | 80 | 1cc94248 | 0 | octocode raw native | 1.7 | 0.5 |
| vscode-extension-host | call_expression | 80 | 1cc94248 | 0 | octocode localSearchCode tool | 215.2 | 5.3 |
| vscode-extension-host | call_expression | 80 | 1cc94248 | 0 | octocode grep CLI | 341.0 | 340.8 |
| excalidraw-render-update | call_expression | 80 | 7eb7579d | 8,062 | ast-grep CLI | 46.3 | 47.1 |
| excalidraw-render-update | call_expression | 80 | 7eb7579d | 8,062 | octocode raw native | 59.3 | 58.3 |
| excalidraw-render-update | call_expression | 80 | 7eb7579d | 8,062 | octocode localSearchCode tool | 4,963.2 | 4,954.7 |
| excalidraw-render-update | call_expression | 80 | 7eb7579d | 8,062 | octocode grep CLI | 5,370.5 | 5,388.3 |
| django-queryset-execution | call | 80 | be4ebc53 | 337 | ast-grep CLI | 30.1 | 30.8 |
| django-queryset-execution | call | 80 | be4ebc53 | 337 | octocode raw native | 11.9 | 10.6 |
| django-queryset-execution | call | 80 | be4ebc53 | 337 | octocode localSearchCode tool | 224.5 | 215.4 |
| django-queryset-execution | call | 80 | be4ebc53 | 337 | octocode grep CLI | 595.7 | 573.8 |
| tokio-runtime-scheduling | call_expression | 80 | c4185e02 | 2,164 | ast-grep CLI | 34.0 | 35.7 |
| tokio-runtime-scheduling | call_expression | 80 | c4185e02 | 2,164 | octocode raw native | 24.8 | 22.1 |
| tokio-runtime-scheduling | call_expression | 80 | c4185e02 | 2,164 | octocode localSearchCode tool | 1,316.8 | 1,315.7 |
| tokio-runtime-scheduling | call_expression | 80 | c4185e02 | 2,164 | octocode grep CLI | 1,701.2 | 1,681.3 |
| okhttp-interceptor-chain | method_invocation | 71 | 06e5fc22 | 1,072 | ast-grep CLI | 42.2 | 33.5 |
| okhttp-interceptor-chain | method_invocation | 71 | 06e5fc22 | 1,072 | octocode raw native | 14.6 | 12.1 |
| okhttp-interceptor-chain | method_invocation | 71 | 06e5fc22 | 1,072 | octocode localSearchCode tool | 703.5 | 695.0 |
| okhttp-interceptor-chain | method_invocation | 71 | 06e5fc22 | 1,072 | octocode grep CLI | 1,066.4 | 1,063.2 |
| gin-middleware-routing | call_expression | 80 | 8d43b701 | 7,252 | ast-grep CLI | 55.3 | 53.2 |
| gin-middleware-routing | call_expression | 80 | 8d43b701 | 7,252 | octocode raw native | 53.8 | 53.0 |
| gin-middleware-routing | call_expression | 80 | 8d43b701 | 7,252 | octocode localSearchCode tool | 4,621.1 | 4,608.7 |
| gin-middleware-routing | call_expression | 80 | 8d43b701 | 7,252 | octocode grep CLI | 4,957.9 | 4,977.0 |

---

## How to Run the Benchmark

### Prerequisites

**1. Install ast-grep** (the external CLI being compared against):

```bash
brew install ast-grep          # macOS
cargo install ast-grep --locked  # any platform
npm install -g @ast-grep/cli   # via npm
```

Verify:

```bash
ast-grep --version   # or: sg --version
# → ast-grep 0.44.0
```

**2. Build the Octocode CLI** (if not already built):

```bash
yarn workspace octocode build:dev
```

Verify:

```bash
node packages/octocode/out/octocode.js --version
# → octocode v2.0.0
```

### Quick run (repos already cached)

All scenario repos are pinned at exact commits and cached in
`packages/octocode-benchmark/target/ast-grep-upstream/repos/`. If that folder
exists, no cloning is needed:

```bash
node packages/octocode-benchmark/benchmark/ast-grep/compare-upstream-scenarios.mjs \
  --repeats 3 --warmups 1
```

### Full run (clone repos first)

```bash
node packages/octocode-benchmark/benchmark/ast-grep/compare-upstream-scenarios.mjs \
  --sync-repos --repeats 3 --warmups 1
```

`--sync-repos` clones missing repos and checks out the pinned commit for any
repo whose HEAD has drifted. Repos are stored in
`target/ast-grep-upstream/repos/` and reused across runs.

### Via yarn workspace

```bash
yarn workspace @octocodeai/octocode-benchmark ast:compare:upstream
```

### One scenario only

```bash
node packages/octocode-benchmark/benchmark/ast-grep/compare-upstream-scenarios.mjs \
  --scenario gin-middleware-routing --repeats 3 --warmups 1
```

Available scenarios: `vscode-extension-host`, `excalidraw-render-update`,
`django-queryset-execution`, `tokio-runtime-scheduling`,
`okhttp-interceptor-chain`, `gin-middleware-routing`.

### Tune corpus size

```bash
# Tiny corpus — fast, low noise, good for CI
node packages/octocode-benchmark/benchmark/ast-grep/compare-upstream-scenarios.mjs \
  --files-per-scenario 1 --repeats 3 --warmups 1

# Large corpus (default) — tests result-shaping overhead
node packages/octocode-benchmark/benchmark/ast-grep/compare-upstream-scenarios.mjs \
  --files-per-scenario 80 --repeats 3 --warmups 1
```

### All options

| Flag | Default | What it does |
|---|---|---|
| `--sync-repos` | off | Clone / update repos to pinned commits |
| `--scenario <name>` | all | Run one scenario only |
| `--files-per-scenario <n>` | 80 | How many files to sample per scenario |
| `--max-file-bytes <n>` | 350000 | Skip files larger than this |
| `--repeats <n>` | 3 | Fixed measured runs; reported time is median |
| `--warmups <n>` | 1 | Unmeasured warmup runs before measurement starts |
| `--keep-corpus` | off | Keep temp corpus dir for inspection |
| `--json` | off | Print JSON summary instead of a table |
| `--strict` | off | Exit non-zero when match counts differ |
| `--repo-dir <path>` | `target/ast-grep-upstream/repos` | Repo cache location |
| `--output-dir <path>` | `target/ast-grep-upstream` | Where `latest.json` is written |

Custom ast-grep binary:

```bash
AST_GREP_BIN=/path/to/ast-grep \
  node packages/octocode-benchmark/benchmark/ast-grep/compare-upstream-scenarios.mjs \
  --repeats 3 --warmups 1
```

### Output files

After a run:

| File | What it contains |
|---|---|
| `target/ast-grep-upstream/latest.json` | Full JSON result — all lanes, all scenarios, versions, options |
| `output/comparison.md` | Human-readable comparison table (updated manually after a run) |
| `output/summary.md` | Short summary (updated manually) |

---

## How the Benchmark Works

### Corpus selection

For each scenario, the runner:

1. Reads `benchmark/ast-grep/upstream-outline-scenarios.json` — a compact copy
   of ast-grep's upstream outline scenario list (pinned repo URLs + exact commit
   SHAs).
2. Calls `git ls-files` inside the cached repo to get all tracked files.
3. Filters to the target extension(s) (`ts`, `tsx`, `py`, `rs`, `java`, `go`),
   skips hidden path segments, skips files larger than `--max-file-bytes`, and
   takes the first `--files-per-scenario` sorted results.
4. Copies that deterministic file set into a temp directory (`target/.../corpus/`).
5. Computes a SHA-256 over the corpus (file paths + contents) — printed as the
   first 8 hex chars so you can verify corpus identity across runs.

### Four timing lanes

Each lane runs the same query on the identical temp corpus:

```
ast-grep CLI          →  sg run --json=stream --kind <kind> <corpus-dir>
                          External Rust process, no Octocode code involved.

Octocode raw native   →  engine.structuralSearchFiles({ path, rule, include })
                          Direct Rust/NAPI call. No validation, no sanitizer,
                          no pagination, no result shaping.

localSearchCode tool  →  executeDirectTool('localSearchCode', { ... })
                          Full tool path: schema validation → path security check
                          → ripgrep pre-filter → structural matcher → sanitizer
                          → result shaper → pagination metadata → YAML output.

octocode grep CLI     →  node packages/octocode/out/octocode.js grep <corpus>
                          --rule <rule> --type <ext> --json
                          Full public CLI: fresh Node process, CLI argument
                          parsing, lazy module loading, all tool layers above,
                          plus JSON serialization and process exit.
```

### Warmup

One warmup run per lane is executed and its timing is recorded separately
(`warm ms` column) but excluded from the reported median. This removes first-run
Node/native-module initialization cost from the measured numbers without hiding
it — you can see exactly how big warmup was.

### Correctness check

Match counts from all four lanes are compared after each run. If any lane
disagrees with the others the run still completes but the status column shows
`DIFF <delta%>`. Pass `--strict` to fail on any mismatch.

Match counts must also be stable across all `--repeats` runs. Any variation
within a lane across its own repeated runs is a hard error (the test raises).

### Scenario manifest

`benchmark/ast-grep/upstream-outline-scenarios.json` — pinned at commit
`0af4b77cb07366a52f72180b2c850f64e9f6e455` of `ast-grep/ast-grep`:

| Scenario | Repo | Language | ~Files |
|---|---|---|---|
| vscode-extension-host | microsoft/vscode | TypeScript | ~10k |
| excalidraw-render-update | excalidraw/excalidraw | TSX | ~640 |
| django-queryset-execution | django/django | Python | ~3k |
| tokio-runtime-scheduling | tokio-rs/tokio | Rust | ~790 |
| okhttp-interceptor-chain | square/okhttp | Java/Kotlin | ~645 |
| gin-middleware-routing | gin-gonic/gin | Go | ~110 |
| alamofire-request-lifecycle | Alamofire/Alamofire | Swift | ~110 (SKIP) |

Swift is skipped because Octocode structural grep does not support Swift yet.
Java Kotlin files (`.kt`) are not included — only `.java` files are selected
for okhttp.

---

## Other Benchmark Checks

The runner `benchmark/run-all.mjs` covers the full internal suite, independent
of ast-grep:

```bash
node packages/octocode-benchmark/benchmark/run-all.mjs
# or:
yarn workspace @octocodeai/octocode-benchmark benchmark
```

| Check | Script | What it validates |
|---|---|---|
| Support matrix | `benchmark/check-matrix.mjs` | Every extension in `getSupportedStructuralExtensions()` has a test entry |
| AST | `benchmark/ast/check-ast.mjs` | All 19 tree-sitter grammars parse real samples and match canonical patterns |
| LSP | `benchmark/lsp/check-lsp.mjs` | Language-id resolution and server wiring for 18 languages |
| Minify | `benchmark/minify/check-minify.mjs` | Minifier output for 141 samples across 70+ formats |

These do not invoke ast-grep at all — they validate the shipped engine binary.

To regenerate the support matrix doc:

```bash
yarn workspace @octocodeai/octocode-benchmark support:gen
# writes benchmark/SUPPORT.md
```

---

## Context: Why No Official ast-grep Benchmark?

ast-grep's [contributing guide](https://ast-grep.github.io/contributing/development.html)
says:

> "ast-grep's benchmarking suite is not well developed yet. The result may
> fluctuate too much."

Their shipped `benchmarks/` folder contains an outline benchmark
(`outline_claude_benchmark.py`) that measures how well `ast-grep outline` helps
Claude answer questions about a codebase — not raw structural search speed.
That benchmark was moved out of the public repo in PR #2763 to a local machine.

The `benches/` folder referenced in the dev guide does not exist in the current
`main` branch. No `[[bench]]` entries exist in any Cargo.toml in the repo.

This means there is no canonical published number to cite for ast-grep
structural search performance. This Octocode benchmark is designed to fill that
gap with a reproducible, corpus-pinned methodology.

---

## Artifacts

| Path | Description |
|---|---|
| `benchmark/ast-grep/compare-upstream-scenarios.mjs` | Main benchmark runner |
| `benchmark/ast-grep/upstream-outline-scenarios.json` | Pinned scenario corpus manifest |
| `benchmark/ast/compare-ast-grep-cli.mjs` | Small case-by-case correctness comparison |
| `target/ast-grep-upstream/repos/` | Cached scenario repos (git shallow clones) |
| `target/ast-grep-upstream/latest.json` | Raw JSON from last run |
| `output/comparison.md` | Full human-readable comparison table |
| `docs/STRUCTURAL-GREP-COMPARISON-RECIPES.md` | Recipes for one-off manual comparisons |
