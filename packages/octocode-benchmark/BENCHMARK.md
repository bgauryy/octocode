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
| 11 | `benchmark/rtk-gh/` — agent toolchain comparisons (`rtk-gh-vs-octocode-flows/`) | manual multi-agent run | No | Yes (GitHub) | Manual | `output/<name>-<ts>/` |

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

Full report, methodology, and options: [`results/ast-grep/comparison.md`](./results/ast-grep/comparison.md).

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
| `zustand` | pmndrs/zustand | v5.0.5 | TypeScript (state management) |
| `tokio` | tokio-rs/tokio | tokio-1.45.0 | Rust |
| `spring-boot` | spring-projects/spring-boot | v3.5.3 | Java |
| `chromium` | chromium/src `base/` sparse | HEAD | C++ |
| `nextjs` | vercel/next.js | v15.3.3 | JavaScript/TypeScript |

Chromium uses `--filter=blob:none --sparse` to check out `base/` only (~250 MB vs 35 GB).

```bash
yarn repo:clone                 # clone all repos (one-time, ~minutes)
yarn repo:bench                 # run all probes
yarn repo:bench zustand nextjs  # specific repos only
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
| **MCP tools** | All 13 shared MCP/CLI tool runners via `tools <name> --queries` — schema, routing, pagination, error honesty |
| **npm / packages** | `npmSearch` — package lookup and source-repo handoff |
| **OQL (`octocode search`)** | All active search targets, OQL-to-tool transformations, proof grades, parity with raw tools |
| **Local search** | Text/regex, structural (AST), file finding, content ranges, minification |
| **LSP flows** | `lspGetSemantics` — definitions, references, call hierarchy, symbols within research flows |
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

## `ast:compare:upstream` — Full Report

The full ast-grep-vs-Octocode report (results table, all four timing lanes,
run instructions, methodology, scenario manifest, and context on why ast-grep
has no official benchmark of its own) lives at
[`results/ast-grep/comparison.md`](./results/ast-grep/comparison.md) — that
file is the canonical, up-to-date source (the runner itself only writes raw
JSON to `target/ast-grep-upstream/latest.json`; the markdown report is
written from that JSON after each reported run). A shorter numeric summary is
at [`results/ast-grep/summary.md`](./results/ast-grep/summary.md).

Headline (see the linked report for the full table and methodology):
structural match correctness is 6/6 scenarios matched with zero count
differences; raw-matcher timing is ~2x faster than the ast-grep CLI, while the
full agent-safe tool path trades that for validation/sanitization/pagination
overhead that scales with match count, not file count.
