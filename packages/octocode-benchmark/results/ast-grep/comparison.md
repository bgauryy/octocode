# Octocode vs ast-grep — Structural Grep Benchmark

Recorded: 2026-06-22 · ast-grep 0.44.0 · octocode v2.0.0

---

## What This Is

A deterministic, reproducible comparison of Octocode's structural grep engine
against the [ast-grep](https://github.com/ast-grep/ast-grep) CLI across six
real open-source repositories (TypeScript, Python, Rust, Java, Go, TSX).

**Why it exists.** ast-grep ships no formal public structural grep benchmark.
Their own contributing docs say:

> "ast-grep's benchmarking suite is not well developed yet. The result may
> fluctuate too much."

The outline benchmark they do publish evaluates an agent task (`claude -p`
sessions), not raw structural search speed. This benchmark fills that gap using
the same corpus.

**What it measures.**
- Structural match correctness: do both tools find identical match counts?
- Timing across four layers of the Octocode stack — raw matcher vs. agent-safe
  tool path vs. public CLI — so you can see exactly where time is spent.

This benchmark does not test text grep, LSP navigation, rewriting, or the full
ast-grep rule language. Those are separate capabilities.

---

## Results

**80 files per scenario · 3 measured repeats + 1 warmup**

```
Octocode raw native  ████████████████████   17.1 ms median  │  2.0x faster  │  6/6 matched
ast-grep CLI         ██████████░░░░░░░░░░   34.6 ms median  │  baseline     │  6/6 matched
```

Correctness: **6 of 6 scenarios matched — zero count differences, zero errors.**

`Octocode raw native` = the direct Rust/NAPI `structuralSearchFiles` matcher:
parse and match only, with no tool validation, sanitizer, pagination, JSON
shaping, or Node CLI startup. Use it to compare matcher performance in isolation.

### Per-Scenario Breakdown

| Scenario | Language | Files | Matches | ast-grep CLI | Octocode native | vs ast-grep | Result |
|---|---|---:|---:|---:|---:|---|---|
| VS Code | TypeScript | 80 | 0 | 28.2 ms | 0.5 ms | **57x faster** | MATCH |
| Django | Python | 80 | 337 | 30.8 ms | 10.6 ms | **2.9x faster** | MATCH |
| OkHttp | Java | 71 | 1,072 | 33.5 ms | 12.1 ms | **2.8x faster** | MATCH |
| Tokio | Rust | 80 | 2,164 | 35.7 ms | 22.1 ms | **1.6x faster** | MATCH |
| Gin | Go | 80 | 7,252 | 53.2 ms | 53.0 ms | parity | MATCH |
| Excalidraw | TSX | 80 | 8,062 | 47.1 ms | 58.3 ms | ~parity | MATCH |
| Alamofire | Swift | — | — | — | — | SKIP (no Swift grammar yet) | — |

### All Four Timing Lanes

The benchmark measures the Octocode stack from bottom to top so overhead is
visible at each layer:

| Lane | What it includes | Median ms | vs ast-grep CLI |
|---|---|---:|---|
| **ast-grep CLI** | External Rust CLI process, startup included | 34.6 ms | 1.0x (baseline) |
| **Octocode raw native** | Rust/NAPI matcher only; no validation, no shaping | 17.1 ms | **2.0x faster** |
| **Octocode localSearchCode tool** | + path validation, sanitizer, pagination, YAML shaping | 1,005 ms | 29x slower |
| **Octocode search CLI** | + Node process startup, CLI routing, JSON output | 1,372 ms | 40x slower |

`localSearchCode` and `grep CLI` are **slower by design** — they are the
agent-safe paths. Use `octocode raw native` to compare matcher performance only.

### Result-Shaping Overhead Scales with Match Count

The sanitizer and result-shaper process every match. This makes the tool paths
proportional to match count, not file count:

| Scenario | Matches | native ms | localSearchCode ms | Overhead ratio |
|---|---:|---:|---:|---:|
| VS Code | 0 | 0.5 | 5.3 | 10× |
| Django | 337 | 10.6 | 215.4 | 20× |
| OkHttp | 1,072 | 12.1 | 695.0 | 57× |
| Tokio | 2,164 | 22.1 | 1,315.7 | 60× |
| Gin | 7,252 | 53.0 | 4,608.7 | 87× |
| Excalidraw | 8,062 | 58.3 | 4,954.7 | 85× |

The overhead is roughly linear with match count. The sanitizer's per-match regex
scanning dominates above ~1,000 matches. The raw native path is unaffected.

Node startup cost is visible in the grep CLI path for low-match cases (VS Code:
340.8 ms CLI vs. 0.5 ms native). For high-match cases (Excalidraw 5,388 ms,
Gin 4,977 ms), result shaping overtakes startup as the dominant term.

### Full Raw Timing Table

Values are median wall-clock ms after warmup. `warm ms` = warmup median
(excluded from measured ms). Hash = first 8 hex chars of corpus SHA-256.

| Scenario | Kind | Files | Hash | Matches | Lane | warm ms | ms |
|---|---|---:|---|---:|---|---:|---:|
| vscode-extension-host | call_expression | 80 | 1cc94248 | 0 | ast-grep CLI | 28.8 | 28.2 |
| vscode-extension-host | call_expression | 80 | 1cc94248 | 0 | octocode raw native | 1.7 | 0.5 |
| vscode-extension-host | call_expression | 80 | 1cc94248 | 0 | octocode localSearchCode tool | 215.2 | 5.3 |
| vscode-extension-host | call_expression | 80 | 1cc94248 | 0 | octocode search CLI | 341.0 | 340.8 |
| excalidraw-render-update | call_expression | 80 | 7eb7579d | 8,062 | ast-grep CLI | 46.3 | 47.1 |
| excalidraw-render-update | call_expression | 80 | 7eb7579d | 8,062 | octocode raw native | 59.3 | 58.3 |
| excalidraw-render-update | call_expression | 80 | 7eb7579d | 8,062 | octocode localSearchCode tool | 4,963.2 | 4,954.7 |
| excalidraw-render-update | call_expression | 80 | 7eb7579d | 8,062 | octocode search CLI | 5,370.5 | 5,388.3 |
| django-queryset-execution | call | 80 | be4ebc53 | 337 | ast-grep CLI | 30.1 | 30.8 |
| django-queryset-execution | call | 80 | be4ebc53 | 337 | octocode raw native | 11.9 | 10.6 |
| django-queryset-execution | call | 80 | be4ebc53 | 337 | octocode localSearchCode tool | 224.5 | 215.4 |
| django-queryset-execution | call | 80 | be4ebc53 | 337 | octocode search CLI | 595.7 | 573.8 |
| tokio-runtime-scheduling | call_expression | 80 | c4185e02 | 2,164 | ast-grep CLI | 34.0 | 35.7 |
| tokio-runtime-scheduling | call_expression | 80 | c4185e02 | 2,164 | octocode raw native | 24.8 | 22.1 |
| tokio-runtime-scheduling | call_expression | 80 | c4185e02 | 2,164 | octocode localSearchCode tool | 1,316.8 | 1,315.7 |
| tokio-runtime-scheduling | call_expression | 80 | c4185e02 | 2,164 | octocode search CLI | 1,701.2 | 1,681.3 |
| okhttp-interceptor-chain | method_invocation | 71 | 06e5fc22 | 1,072 | ast-grep CLI | 42.2 | 33.5 |
| okhttp-interceptor-chain | method_invocation | 71 | 06e5fc22 | 1,072 | octocode raw native | 14.6 | 12.1 |
| okhttp-interceptor-chain | method_invocation | 71 | 06e5fc22 | 1,072 | octocode localSearchCode tool | 703.5 | 695.0 |
| okhttp-interceptor-chain | method_invocation | 71 | 06e5fc22 | 1,072 | octocode search CLI | 1,066.4 | 1,063.2 |
| gin-middleware-routing | call_expression | 80 | 8d43b701 | 7,252 | ast-grep CLI | 55.3 | 53.2 |
| gin-middleware-routing | call_expression | 80 | 8d43b701 | 7,252 | octocode raw native | 53.8 | 53.0 |
| gin-middleware-routing | call_expression | 80 | 8d43b701 | 7,252 | octocode localSearchCode tool | 4,621.1 | 4,608.7 |
| gin-middleware-routing | call_expression | 80 | 8d43b701 | 7,252 | octocode search CLI | 4,957.9 | 4,977.0 |

Summary: 6 compared · 0 count differences · 0 errors · 1 skipped (Swift/Alamofire)

### Versions and Inputs

| Item | Value |
|---|---|
| ast-grep | `ast-grep 0.44.0` |
| Octocode CLI | `octocode v2.0.0` |
| Upstream source | `ast-grep/ast-grep` commit `0af4b77c` |
| Upstream source path | `benchmarks/outline-agent-scenarios.json` |
| Scenario repo cache | `packages/octocode-benchmark/target/ast-grep-upstream/repos` |
| Result artifact | `packages/octocode-benchmark/target/ast-grep-upstream/latest.json` |
| Files per scenario | 80 |
| Max file bytes | 350,000 |
| Warmups | 1 |
| Measured repeats | 3 |

---

## How to Run the Benchmark

### Step 1 — Install ast-grep

```bash
brew install ast-grep           # macOS (recommended)
cargo install ast-grep --locked # any platform with Rust
npm install -g @ast-grep/cli    # via npm
```

Verify:

```bash
ast-grep --version
# ast-grep 0.44.0
```

### Step 2 — Build the Octocode CLI

From the monorepo root:

```bash
yarn workspace octocode build:dev
```

Verify:

```bash
node packages/octocode/out/octocode.js --version
# octocode v2.0.0
```

### Step 3 — Run

**Quick run** (scenario repos already cached in `target/`):

```bash
node packages/octocode-benchmark/benchmark/ast-grep/compare-upstream-scenarios.mjs \
  --repeats 3 --warmups 1
```

**First run** (clone repos automatically):

```bash
node packages/octocode-benchmark/benchmark/ast-grep/compare-upstream-scenarios.mjs \
  --sync-repos --repeats 3 --warmups 1
```

`--sync-repos` clones any missing repos and checks out the pinned commit for any
repo whose HEAD has drifted. Repos land in
`target/ast-grep-upstream/repos/` and are reused on subsequent runs.

**Via yarn workspace:**

```bash
yarn workspace @octocodeai/octocode-benchmark ast:compare:upstream
```

**One scenario only:**

```bash
node packages/octocode-benchmark/benchmark/ast-grep/compare-upstream-scenarios.mjs \
  --scenario gin-middleware-routing --repeats 3 --warmups 1
```

Available scenario names: `vscode-extension-host`, `excalidraw-render-update`,
`django-queryset-execution`, `tokio-runtime-scheduling`,
`okhttp-interceptor-chain`, `gin-middleware-routing`.

**Tune corpus size:**

```bash
# Tiny (1 file) — fast, low noise, good for CI smoke
node packages/octocode-benchmark/benchmark/ast-grep/compare-upstream-scenarios.mjs \
  --files-per-scenario 1 --repeats 3 --warmups 1

# Large (80 files, default) — exercises result-shaping overhead
node packages/octocode-benchmark/benchmark/ast-grep/compare-upstream-scenarios.mjs \
  --files-per-scenario 80 --repeats 3 --warmups 1
```

**Custom ast-grep binary:**

```bash
AST_GREP_BIN=/path/to/ast-grep \
  node packages/octocode-benchmark/benchmark/ast-grep/compare-upstream-scenarios.mjs \
  --repeats 3 --warmups 1
```

### All Flags

| Flag | Default | What it does |
|---|---|---|
| `--sync-repos` | off | Clone / fast-forward repos to pinned commits |
| `--scenario <name>` | all | Run one scenario only |
| `--files-per-scenario <n>` | 80 | Files sampled per scenario |
| `--max-file-bytes <n>` | 350000 | Skip files larger than this |
| `--repeats <n>` | 3 | Measured runs per lane; report is median |
| `--warmups <n>` | 1 | Unmeasured warmup runs before measurement |
| `--keep-corpus` | off | Keep temp corpus dir after the run |
| `--json` | off | Emit JSON summary instead of a table |
| `--strict` | off | Exit non-zero when match counts differ |
| `--repo-dir <path>` | `target/ast-grep-upstream/repos` | Repo cache root |
| `--output-dir <path>` | `target/ast-grep-upstream` | Where `latest.json` is written |

### Output Files

| File | Contents |
|---|---|
| `target/ast-grep-upstream/latest.json` | Full JSON — all lanes, scenarios, versions, raw timings |
| `output/comparison.md` | This file — full human-readable comparison |
| `output/summary.md` | Short pass/fail summary |

---

## How the Benchmark Works

### Corpus Selection

For each scenario the runner:

1. Reads `benchmark/ast-grep/upstream-outline-scenarios.json` — ast-grep's
   upstream outline scenario list, pinned to exact repo URLs and commit SHAs.
2. Calls `git ls-files` inside the cached repo to enumerate tracked files.
3. Filters by target extension (`ts`, `tsx`, `py`, `rs`, `java`, `go`), skips
   hidden path segments, skips files over `--max-file-bytes`, and takes the
   first `--files-per-scenario` results (sorted alphabetically for stability).
4. Copies that exact file set into a temp directory under
   `target/ast-grep-upstream/corpus/`.
5. Computes SHA-256 over the corpus (paths + contents), printed as the first 8
   hex chars — the `hash` column — so corpus identity can be verified across
   runs.

### Four Timing Lanes

Each lane runs the **identical query on the identical temp corpus**:

```
ast-grep CLI
  sg run --json=stream --kind <kind> <corpus-dir>
  External Rust process. Zero Octocode code involved.

Octocode raw native
  engine.structuralSearchFiles({ path, rule, include, maxFiles, maxFileBytes })
  Direct Rust/NAPI call. No Node startup, no validation, no sanitizer,
  no pagination, no result shaping.

Octocode localSearchCode tool
  executeDirectTool('localSearchCode', { mode:'structural', rule, include, ... })
  Full tool path: schema validation → path security check → ripgrep pre-filter
  → structural matcher → sanitizer (secret redaction) → result shaper
  → pagination metadata → YAML output.

Octocode search CLI
  node packages/octocode/out/octocode.js search <corpus> --rule <rule> --lang <ext> --json
  Full public CLI: fresh Node process per repeat, CLI argument parsing, lazy
  module loading, all layers from localSearchCode above, plus JSON serialization
  and process exit. Every measured repeat starts a new process, so startup cost
  is included in every timing sample.
```

### Warmup

One warmup run per lane executes before measurement starts. Its timing appears
in the `warm ms` column and is **excluded from the reported median**. This
removes first-run Node/native-module initialization from the measured numbers
without hiding it — you can see warmup cost separately and judge whether the
native module loaded clean.

Example from this run: `localSearchCode` on the VS Code scenario was 215.2 ms
during warmup and 5.3 ms measured (the 0-match scenario; warmup loaded all the
validation/shaper machinery). Gin's `localSearchCode` warmup was 4,621.1 ms and
measured 4,608.7 ms — warmup barely helped because the shaper is doing real
work proportional to 7,252 matches every run.

### Correctness Check

Match counts from **all four lanes** are compared after each scenario. A
mismatch marks the row `DIFF <delta%>` but does not abort the run. Pass
`--strict` to exit non-zero on any mismatch.

Match counts must also be **stable across all `--repeats` runs**. Any variation
within a lane is a hard error — the script throws immediately.

### Scenario Manifest

`benchmark/ast-grep/upstream-outline-scenarios.json` is pinned to commit
`0af4b77c` of `ast-grep/ast-grep`:

| Scenario name | Repo | Language | Approx files |
|---|---|---|---|
| vscode-extension-host | microsoft/vscode | TypeScript | ~10k |
| excalidraw-render-update | excalidraw/excalidraw | TSX | ~640 |
| django-queryset-execution | django/django | Python | ~3k |
| tokio-runtime-scheduling | tokio-rs/tokio | Rust | ~790 |
| okhttp-interceptor-chain | square/okhttp | Java/Kotlin | ~645 |
| gin-middleware-routing | gin-gonic/gin | Go | ~110 |
| alamofire-request-lifecycle | Alamofire/Alamofire | Swift | ~110 (SKIP) |

Swift is skipped because Octocode structural search does not have a Swift grammar
yet. OkHttp selects only `.java` files — `.kt` files are excluded.

### Determinism Controls

- Scenario repos are pinned to **exact commit SHAs** in the manifest.
- File selection uses sorted `git ls-files` output — alphabetical, stable.
- Hidden path segments (any segment starting with `.`) are skipped so both CLIs
  see the same kind of corpus by default.
- `--files-per-scenario`, `--max-file-bytes`, `--warmups`, and `--repeats` are
  fixed inputs, not defaults overridden at runtime.
- Each row prints a corpus SHA-256 prefix — rerunning with the same repos and
  flags must produce the **same hash**.
- Timings are machine/load-dependent; match counts and corpus identity are the
  deterministic parts. Timing variance is expected between machines and runs.

---

## Other Benchmark Checks (No ast-grep Required)

The internal suite validates the Octocode engine independently of any external
tool:

```bash
node packages/octocode-benchmark/benchmark/run-all.mjs
# or:
yarn workspace @octocodeai/octocode-benchmark benchmark
```

| Check | Script | Validates |
|---|---|---|
| Support matrix | `benchmark/check-matrix.mjs` | Every extension in `getSupportedStructuralExtensions()` has a test entry |
| AST | `benchmark/ast/check-ast.mjs` | All 19 tree-sitter grammars parse real samples and resolve canonical patterns |
| LSP | `benchmark/lsp/check-lsp.mjs` | Language-id resolution and server wiring for 18 languages |
| Minify | `benchmark/minify/check-minify.mjs` | Minifier output for 141 samples across 70+ formats |

Regenerate the support matrix doc:

```bash
yarn workspace @octocodeai/octocode-benchmark support:gen
# writes benchmark/SUPPORT.md
```

Small case-by-case correctness comparison (no corpus cloning needed):

```bash
yarn workspace @octocodeai/octocode-benchmark ast:compare
# runs benchmark/ast/compare-ast-grep-cli.mjs
```

---

## Context: Why No Official ast-grep Benchmark?

ast-grep's [contributing guide](https://ast-grep.github.io/contributing/development.html)
explicitly states:

> "ast-grep's benchmarking suite is not well developed yet. The result may
> fluctuate too much."

The `benchmarks/` folder they do ship contains `outline_claude_benchmark.py` —
a benchmark that measures how well `ast-grep outline` helps Claude answer
questions about a codebase. That is an agent-quality benchmark, not a
structural search speed benchmark.

In PR #2763 ("Move outline docs and benchmarks out of repo"), the outline
benchmark harness was moved to a developer's local machine
(`/Users/hd/code/ast-grep-outline-materials/benchmarks`). It is not publicly
accessible.

The `benches/` folder referenced in the dev guide does not exist in the current
`main` branch. No `[[bench]]` entries appear in any Cargo.toml in the repo.
There are no published numbers for ast-grep raw structural search speed.

This Octocode benchmark fills that gap with a reproducible, corpus-pinned
methodology that anyone can re-run.

---

## Artifacts

| Path | Description |
|---|---|
| `benchmark/ast-grep/compare-upstream-scenarios.mjs` | Main benchmark runner |
| `benchmark/ast-grep/upstream-outline-scenarios.json` | Pinned scenario corpus manifest |
| `benchmark/ast/compare-ast-grep-cli.mjs` | Small case-by-case correctness comparison |
| `target/ast-grep-upstream/repos/` | Cached scenario repos (git shallow clones at pinned commits) |
| `target/ast-grep-upstream/latest.json` | Raw JSON from last run |
| `output/comparison.md` | This file |
| `output/summary.md` | Short pass/fail summary |
| `docs/STRUCTURAL-GREP-COMPARISON-RECIPES.md` | Recipes for one-off manual comparisons |
| `BENCHMARK.md` | Quick-reference card (results + how-to) |

---

## External References

- ast-grep benchmark folder: https://github.com/ast-grep/ast-grep/tree/main/benchmarks
- ast-grep outline benchmark: https://github.com/ast-grep/ast-grep/blob/main/benchmarks/outline-benchmark.md
- ast-grep contributing guide: https://ast-grep.github.io/contributing/development.html
- ast-grep CLI reference: https://ast-grep.github.io/reference/cli.html
