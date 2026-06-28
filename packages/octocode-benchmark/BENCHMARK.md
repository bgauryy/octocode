# Octocode Benchmark Suite

Internal benchmark package (`@octocodeai/octocode-benchmark`) that proves every engine layer works correctly. Run by CI, developers, and agents to verify a build before shipping.

---

## Quick Reference

```bash
# ── CI suite (no network, no servers) ───────────────────────
yarn benchmark          # matrix + AST + LSP + minify + CLI metadata

# ── Individual checks ───────────────────────────────────────
yarn ast:check          # tree-sitter grammar coverage (all languages)
yarn lsp:check          # LSP config wiring (no servers needed)
yarn lsp:live           # live TypeScript LSP protocol test (needs server)
yarn minify:check       # minifier over every configured format
yarn matrix:check       # full extension × feature support matrix
yarn cli:check          # CLI/tool/OQL schema metadata gate

# ── External comparison (needs ast-grep CLI installed) ──────
yarn ast:compare        # correctness comparison against ast-grep CLI
yarn ast:compare:upstream   # upstream ast-grep benchmark scenarios

# ── Cross-repo real-world probes (needs network, one-time) ──
yarn repo:clone         # clone 5 repos at pinned tags into target/
yarn repo:bench         # run text/AST/symbols probes → results/repo/<name>/results.md

# ── Live eval: GitHub / MCP tools / npm / OQL / local flows ─
# Run manually as an agent benchmark — see benchmark/octocode/README.md
```

---

## Package Layout

```
packages/octocode-benchmark/
  BENCHMARK.md              ← this file — benchmark guide for agents
  package.json              ← yarn scripts for all benchmarks
  benchmark/                ← scripts, fixtures, samples (source)
    _engine.mjs             ← shared engine loader (napi)
    run-all.mjs             ← CI orchestrator (checks 1–2, 4–6)
    ast/                    ← AST grammar benchmark
    lsp/                    ← LSP wiring benchmark
    minify/                 ← minifier benchmark
    check-matrix.mjs        ← full support matrix
    cli/                    ← CLI metadata gate
    octocode/               ← live CLI/raw-tool/OQL question catalog
    ast-grep/               ← upstream scenario comparison
    repo/                   ← cross-repo clone + probe scripts
  recipes/                  ← agent runbooks and check recipes
    agent-benchmark-runbook.md   ← required output layout + determinism rules
    ast-grep.md             ← ast-grep correctness check recipes
    dead-code.md            ← dead code / knip + Octocode recipes
    cli-tools-and-flows.md  ← compatibility pointer → benchmark/octocode/
  results/                  ← benchmark output (separate from source)
    README.md
    ci/latest.md            ← last CI suite run
    ast-grep/               ← ast-grep timing results
    repo/                   ← per-repo probe results (yarn repo:bench)
  output/                   ← full timestamped run artifacts
    <benchmark-name>-<YYYYMMDDTHHMMSSZ>/
      README.md  manifest.json  summary.json  commands.ndjson
      results.md  reflection.md  ratings.json  raw/  schemes/  artifacts/
  target/                   ← cloned repos (generated, not committed)
```

---

## Benchmark Index

| # | Script / Doc | Yarn command | Needs server? | Needs network? | In CI? | Results location |
|---|-------------|-------------|---------------|----------------|--------|-----------------|
| 1 | `benchmark/ast/check-ast.mjs` | `ast:check` | No | No | ✅ | `results/ci/latest.md` |
| 2 | `benchmark/lsp/check-lsp.mjs` | `lsp:check` | No | No | ✅ | `results/ci/latest.md` |
| 3 | `benchmark/lsp/check-lsp-live.mjs` | `lsp:live` | Yes (ts-server) | No | Manual | console |
| 4 | `benchmark/minify/check-minify.mjs` | `minify:check` | No | No | ✅ | `results/ci/latest.md` |
| 5 | `benchmark/check-matrix.mjs` | `matrix:check` | No | No | ✅ | `docs/LSP_SERVER_LIFECYCLE.md` (support-matrix markers) |
| 6 | `benchmark/cli/check-cli-metadata.mjs` | `cli:check` | No | No | ✅ | `results/ci/latest.md` |
| 7 | `benchmark/ast/compare-ast-grep-cli.mjs` | `ast:compare` | No | No | Optional | console |
| 8 | `benchmark/ast-grep/compare-upstream-scenarios.mjs` | `ast:compare:upstream` | No | Optional | Optional | `results/ast-grep/` + `output/` |
| 9 | `benchmark/repo/clone.mjs` + `run.mjs` | `repo:clone` + `repo:bench` | No | Yes (clone) | Manual | `results/repo/<name>/results.md` |
| 10 | `benchmark/octocode/` | manual agent run | No | Yes (GitHub/npm) | Manual | `output/<name>-<ts>/` |

`yarn benchmark` (alias `yarn test`) runs checks 1–2, 4–6 in order: matrix → AST → LSP → minify → CLI metadata.

---

## 1. `ast:check` — AST Grammar Coverage

**Script**: `benchmark/ast/check-ast.mjs`

Proves every tree-sitter grammar loaded by `octocode-engine` works end-to-end through the napi binary. For each grammar it runs three probes:

| Probe | What it tests |
|-------|--------------|
| PARSE | `structuralSearch(realSample, "$$$")` returns nodes — confirms the grammar loaded and the ABI is intact |
| MATCH | `structuralSearch(snippet, pattern/rule)` returns the expected count — confirms metavars and node-kind rules work |
| SIGNATURE | `extractSignatures()` returns a non-empty skeleton for `sig:true` grammars |

Also asserts every extension in `getSupportedStructuralExtensions()` is claimed by exactly one grammar entry — a new grammar without a sample here fails the check.

**Samples**: `benchmark/ast/samples/` — one real file per grammar from popular open-source repos (provenance in `ast/manifest.json`).

---

## 2. `lsp:check` — LSP Config Wiring (No Servers)

**Script**: `benchmark/lsp/check-lsp.mjs`

Verifies the engine's LSP config layer resolves correctly for every language, without spawning a server:

| Probe | What it tests |
|-------|--------------|
| LANGUAGE-ID | `detectLanguageId(sample)` returns the expected LSP language id |
| SERVER | `getLanguageServerForFile(sample)` resolves a non-empty command and correct languageId |
| SEMANTICS | In-process: `getSemanticBoundaryOffsets` (tree-sitter) + `extractJsSymbols` for JS/TS (OXC) |

Server availability (`isCommandAvailable`) is **reported** but never fails the check — servers are optional per developer.

**Samples**: `benchmark/lsp/samples/` — real files covering every configured server language.

---

## 3. `lsp:live` — Live LSP Protocol (Needs Server)

**Script**: `benchmark/lsp/check-lsp-live.mjs`
**Run manually**: `yarn lsp:live` — not in CI.

Spawns a real `typescript-language-server` through `NativeLspClient` and exercises every LSP operation against a known TypeScript file:

- `documentSymbols` — file outline
- `definition` — go-to-definition
- `references` — find all usages
- `hover` — signature / type info
- `typeDefinition` — resolve underlying type
- `implementation` — interface → concrete class
- `callHierarchy` — callers and callees

Exits 0 (skip) if `typescript-language-server` is not on PATH.

---

## 4. `minify:check` — Minifier Coverage

**Script**: `benchmark/minify/check-minify.mjs`

Runs the engine minifier over every language sample in `benchmark/minify/<lang>/` and asserts it produces output without errors. Exits non-zero if any sample returns empty output or an error.

**Languages**: JS/TS, Python, Go, Rust, Java, C/C++/H/HPP, C#, CSS/SCSS/Less, HTML, JSON/JSONC, YAML/TOML, Bash, Ruby, PHP, Kotlin, Elixir, Erlang, Swift, Scala, Lua, SQL, R, Zig, OCaml, HCL, Proto, GraphQL, Clojure, Dart, Haskell, INI, and more (~70+ formats).

---

## 5. `matrix:check` — Full Extension × Feature Matrix

**Script**: `benchmark/check-matrix.mjs`

Probes every extension the engine knows (union of minify config, structural grammars, signature list, and LSP server specs) and verifies each feature live:

| Column | What it tests |
|--------|--------------|
| MINIFY | Configured strategy + live `minifyContentResult` probe |
| AST | In `getSupportedStructuralExtensions()` AND `structuralSearch "$$$"` returns nodes |
| SIG | In `getSupportedSignatureExtensions()` |
| LSP | `detectLanguageId` + `getLanguageServerForFile` resolve a server |

Exits non-zero on any anomaly (claimed-but-not-working, or list/behavior mismatch).

Regenerate the support matrix (into `docs/LSP_SERVER_LIFECYCLE.md`, between the `support-matrix` markers) with the live data:

```bash
yarn support:gen
```

---

## 6. `cli:check` — CLI Metadata Gate

**Script**: `benchmark/cli/check-cli-metadata.mjs`

Offline gate (no auth, no network, no tool execution). Validates:

- All `octocode-core` tool descriptions, schema texts, and instructions load correctly
- `octocode help`, `octocode context`, `octocode tools <name> --scheme`, `octocode search --scheme` render without errors
- Direct tool definitions and display fields are complete and undrifted

Run before publishing CLI or core packages.

---

## 7. `ast:compare` — External ast-grep Correctness Comparison

**Script**: `benchmark/ast/compare-ast-grep-cli.mjs`
**Needs**: `ast-grep` CLI installed.

Shells out to an installed `ast-grep` binary and runs identical structural patterns through both ast-grep and Octocode on temp file sets. Compares match counts and timing.

```bash
# Install ast-grep first
brew install ast-grep           # macOS
npm install -g @ast-grep/cli    # via npm
cargo install ast-grep --locked # via cargo

# Run
yarn ast:compare
# or: AST_GREP_BIN=/path/to/ast-grep yarn ast:compare
```

---

## 8. `ast:compare:upstream` — Upstream Scenario Benchmark

See [full documentation below](#octocode-vs-ast-grep--structural-grep-benchmark).

Short form:

```bash
# Quick run (repos already cached in target/ast-grep-upstream/repos/)
node benchmark/ast-grep/compare-upstream-scenarios.mjs --repeats 3 --warmups 1

# Full run with repo sync
node benchmark/ast-grep/compare-upstream-scenarios.mjs --sync-repos --repeats 3 --warmups 1
```

---

## 9. `repo:clone` + `repo:bench` — Cross-Repo Real-World Probes

**Scripts**: `benchmark/repo/clone.mjs`, `benchmark/repo/run.mjs`
**Run manually**: not in CI (slow, requires network for clone).

Clones five popular repos at pinned tags and runs text/AST/symbols probes at repo scale:

| Key | Repo | Tag | Language |
|-----|------|-----|----------|
| `react` | facebook/react | v19.1.0 | JavaScript/TypeScript |
| `tokio` | tokio-rs/tokio | tokio-1.45.0 | Rust |
| `spring-boot` | spring-projects/spring-boot | v3.5.3 | Java |
| `chromium` | chromium/src `base/` sparse | HEAD | C++ |
| `nextjs` | vercel/next.js | v15.3.3 | JavaScript/TypeScript |

Chromium uses `--filter=blob:none --sparse` to check out `base/` only (~250 MB vs 35 GB).

```bash
yarn repo:clone                 # clone all repos (one-time, ~minutes)
yarn repo:bench                 # run all probes
yarn repo:bench react nextjs    # specific repos only
```

**Probes per repo**: `engine.searchRipgrep` (text), `engine.structuralSearchFiles` (AST), `engine.structuralSearch("$$$")` (parse check). Results written to `results/repo/<name>/results.md`.

**Reproducibility**: `benchmark/repo/pins.json` records the exact SHA cloned. Commit it after cloning to lock future runs.

---

## 10. `benchmark/octocode/` — Live Octocode Tool Benchmark

**Doc**: [`benchmark/octocode/README.md`](https://github.com/bgauryy/octocode/blob/main/packages/octocode-benchmark/benchmark/octocode/README.md)
**Run**: manually by an agent following the runbook — not automated.
**Needs**: GitHub token (`OCTOCODE_TOKEN`), network access, built CLI.

This is the canonical question catalog for proving that Octocode's agent-facing surface works end-to-end across all tool surfaces — not just internal engine correctness. It covers:

| Surface | What is measured |
|---------|-----------------|
| **GitHub tools** | `ghSearchRepos`, `ghSearchCode`, `ghGetFileContent`, `ghViewRepoStructure`, `ghHistoryResearch` — repo search, code search, file fetch, PR list/detail/comments/reviews/commits, commit history |
| **MCP tools** | All 14 shared MCP/CLI tool runners via `tools <name> --queries` — schema, routing, pagination, error honesty |
| **npm / packages** | `npmSearch` — package lookup and source-repo handoff |
| **OQL (`octocode search`)** | All active search targets, OQL-to-tool transformations, proof grades, parity with raw tools |
| **Local search** | Text/regex, structural (AST), file finding, content ranges, minification |
| **LSP flows** | `lspGetSemantics` — definitions, references, call hierarchy, symbols within research flows |
| **Binary/archive** | `localBinaryInspect`, `localUnzip` — native inspect/strings, archive list/extract, follow-up local research |
| **Cross-repo flows** | Compare related implementations across LangChain, LangGraph, Zustand, Hermes repos |
| **Pagination** | `page`, `matchPage`, `charOffset`/`charLength`, `responseCharOffset` — lossless paging across all tools |
| **Token efficiency** | Triage with `--compact`/`--discovery` before deep reads; `--mode symbols` vs full content |

### Corpus

| Alias | Repo | Why |
|-------|------|-----|
| `LCJS` | `langchain-ai/langchainjs` | TypeScript, PR archaeology, streaming APIs |
| `LCPY` | `langchain-ai/langchain` | Python comparison, large-repo pagination |
| `LGJS` | `langchain-ai/langgraphjs` | Cross-repo comparison with LangChain.js |
| `LGPY` | `langchain-ai/langgraph` | Python graph/runtime, large tree structure |
| `ZUSTAND` | `pmndrs/zustand` | Small real TS package, npm-to-repo handoff |
| `HERMES_ENGINE` | `facebook/hermes` | Native/runtime repo, CMake/content, non-TS code |
| `LOCAL` | This monorepo root | Dogfooding all local tools |

### Running the eval

Follow `recipes/agent-benchmark-runbook.md` exactly. The runbook defines the required output layout, determinism rules, and schema contract. Every completed run writes:

```
output/<benchmark-name>-<YYYYMMDDTHHMMSSZ>/
  README.md          manifest.json       summary.json
  commands.ndjson    results.md          reflection.md
  ratings.json       raw/  schemes/  artifacts/
```

```bash
# Quick start
BENCHMARK_NAME="octocode-live-tools"
BENCH_TIMESTAMP="$(date -u +%Y%m%dT%H%M%SZ)"
BENCH_OUT="packages/octocode-benchmark/output/${BENCHMARK_NAME}-${BENCH_TIMESTAMP}"
mkdir -p "$BENCH_OUT/raw" "$BENCH_OUT/artifacts" "$BENCH_OUT/schemes"
```

---

## Recipes

Runbooks and check recipes live in `recipes/`:

| File | Purpose |
|------|---------|
| `recipes/agent-benchmark-runbook.md` | **Required** before any manual benchmark run. Defines output layout, determinism rules, `manifest.json` contract, and evidence quality standards. |
| `recipes/ast-grep.md` | Step-by-step checks proving Octocode structural search against ast-grep: link verification, CLI availability, correctness checks, known gaps. |
| `recipes/dead-code.md` | Dead code and transitive dependency check: knip (entrypoint-aware deletion audit) + Octocode (symbol-level candidate triage and LSP proof). |
| `recipes/cli-tools-and-flows.md` | Compatibility pointer to `benchmark/octocode/`. Do not add flow rows here. |

---

## Results Location

| Benchmark | Where results land |
|-----------|-------------------|
| CI suite (1–2, 4–6) | `results/ci/latest.md` |
| Support matrix | `docs/LSP_SERVER_LIFECYCLE.md` § Full format support matrix (regenerated by `yarn support:gen`) |
| ast-grep comparison | `results/ast-grep/comparison.md` + `results/ast-grep/summary.md` |
| Cross-repo probes | `results/repo/<name>/results.md` |
| Unified eval (GitHub/MCP/npm/OQL) | `output/<benchmark-name>-<YYYYMMDDTHHMMSSZ>/results.md` |

See [`results/README.md`](./results/README.md) for the full results tree.

---

## Engine Loader

All benchmark scripts load the engine via `benchmark/_engine.mjs`:

```js
import { engine, engineRoot, benchmarkRoot, packageRoot } from '../_engine.mjs'
```

If the engine binary is missing, rebuild first:

```bash
cd packages/octocode-engine && yarn build
```

---

## Adding a New Benchmark

1. Add a `.mjs` file under `benchmark/<domain>/`.
2. Import `engine` from `../_engine.mjs`.
3. Exit non-zero on failure; print a clear summary.
4. Add a `yarn <name>:check` script to `package.json`.
5. If it belongs in CI, add it to `benchmark/run-all.mjs`.
6. If it needs real samples, add them to `samples/` and update `manifest.json`.

---

---

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
| **Octocode search CLI** | + Node process startup, CLI routing, JSON output | 1,372 ms | 40x slower than ast-grep |

`localSearchCode` and the command-line text-search baseline are slower by design
because they exercise agent-safe paths. Use `octocode raw native` to compare
matcher performance only.

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

Every reported benchmark run writes to a timestamped directory:

```text
packages/octocode-benchmark/output/<benchmark-name>-<YYYYMMDDTHHMMSSZ>/
```

Required files:

| File | What it contains |
|---|---|
| `README.md` | Human index: benchmark name, verdict, reproduction command |
| `manifest.json` | Git state, environment, fixed inputs, cache/network mode |
| `summary.json` | Machine-readable result that follows [`output-run.schema.json`](https://github.com/bgauryy/octocode/blob/main/packages/octocode-benchmark/benchmark/output-run.schema.json) |
| `commands.ndjson` | One command record per line: command, exit code, duration, raw output files |
| `results.md` | Human-readable measurements, evidence anchors, and pass/fail details |
| `reflection.md` | What worked, what did not, missing pieces, possible improvements, praises, ratings, next fix |
| `ratings.json` | 1-10 scores with reasons for tools, OQL, CLI, data, schema, research, and output quality |
| `raw/` | Raw stdout/stderr/tool JSON, one file per command |
| `schemes/` | Captured help/schema text used by the run |
| `artifacts/` | Optional derived tables, corpus manifests, or comparison files |

Read the agent output contract before running manual or live CLI benchmarks:
[`recipes/agent-benchmark-runbook.md`](https://github.com/bgauryy/octocode/blob/main/packages/octocode-benchmark/recipes/agent-benchmark-runbook.md).

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

octocode search CLI     →  node packages/octocode/out/octocode.js search <corpus>
                          --rule <rule> --lang <ext> --json
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

Swift is skipped because Octocode structural search does not support Swift yet.
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
| AST | `benchmark/ast/check-ast.mjs` | All 33 tree-sitter grammars parse real samples and match canonical patterns |
| LSP | `benchmark/lsp/check-lsp.mjs` | Language-id resolution and server wiring for 18 languages |
| Minify | `benchmark/minify/check-minify.mjs` | Minifier output for 149 samples across 70+ formats |
| CLI metadata | `benchmark/cli/check-cli-metadata.mjs` | All raw tool descriptions/schemes, CLI command descriptions/schemes, agent context instructions, and OQL schema render from canonical metadata |

These do not invoke ast-grep at all. Matrix, AST, LSP, and minify validate the
shipped engine binary; CLI metadata validates the built agent-facing CLI surface
without network or auth.

Live CLI/tool/flow benchmarks beyond metadata are intentionally documented as a
manual question catalog rather than part of `benchmark/run-all.mjs`, because they may
require GitHub network access, npm registry access, clone permissions, and
token-dependent rate limits. Use
[`benchmark/octocode/`](https://github.com/bgauryy/octocode/blob/main/packages/octocode-benchmark/benchmark/octocode/README.md)
for the GitHub, npm, clone, local-tool, minify, OQL, and search-to-fetch flow
matrix.

To regenerate the support matrix doc:

```bash
yarn workspace @octocodeai/octocode-benchmark support:gen
# writes the matrix into docs/LSP_SERVER_LIFECYCLE.md (support-matrix markers)
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
| `benchmark/output-run.schema.json` | Required `summary.json` schema for all timestamped benchmark runs |
| `target/ast-grep-upstream/repos/` | Cached scenario repos (git shallow clones) |
| `target/ast-grep-upstream/latest.json` | Raw JSON from last run |
| `output/comparison.md` | Full human-readable comparison table |
| `recipes/ast-grep.md` | Check-by-check ast-grep comparison and validation recipes |
| `recipes/dead-code.md` | Dead code and transitive check recipes (knip + Octocode) |
| `benchmark/octocode/` | Split live CLI/raw-tool/OQL/workflow question catalog for local, GitHub, npm, clone-backed proof, artifacts, pagination, and parity |
| `recipes/cli-tools-and-flows.md` | Compatibility pointer to the split live question catalog |
| `recipes/agent-benchmark-runbook.md` | Agent instructions for deterministic runs, timestamped output, reflection, ratings, and required result schema |
