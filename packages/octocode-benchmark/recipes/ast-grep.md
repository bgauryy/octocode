# ast-grep Comparison Recipe

Validates Octocode structural search against the
[ast-grep](https://github.com/ast-grep/ast-grep) CLI.
Octocode does **not** link ast-grep crates — this recipe shells out to an
installed `ast-grep` binary. Each section is a discrete check you can run and
verify independently.

---

## Check 1 — Prove ast-grep is not linked

```bash
cargo tree --manifest-path packages/octocode-engine/Cargo.toml --edges normal \
  | rg 'ast-grep|ast_grep'
rg -n 'ast_grep_core|ast_grep_config|ast-grep-core|ast-grep-config' \
  packages/octocode-engine
```

**Expected:** no output. The only ast-grep integration is the external CLI used
from this benchmark package. Any output here is a regression.

---

## Check 2 — Verify ast-grep CLI is available

Install one of:

```bash
brew install ast-grep
npm install --global @ast-grep/cli
pip install ast-grep-cli
cargo install ast-grep --locked
cargo binstall ast-grep
```

Verify:

```bash
ast-grep --version
# ast-grep 0.44.0 (or newer)
```

The binary may also be available as `sg`. Prefer `ast-grep` in all benchmark
commands. Use `AST_GREP_BIN` to point at a custom binary:

```bash
AST_GREP_BIN=/path/to/ast-grep \
  yarn workspace @octocodeai/octocode-benchmark ast:compare
```

---

## Check 3 — Pattern match text must be identical (not just count)

`benchmark/ast/compare-ast-grep-cli.mjs` shells out to `ast-grep run` and calls
Octocode `structuralSearch` on the same content string. It asserts that match
**text** arrays are identical after `trim()` + `sort()` — not just counts.

```bash
yarn workspace octocode build:dev
yarn workspace @octocodeai/octocode-benchmark ast:compare
```

Covered cases (each is a `pattern` or `kind` match):

| Case | Lang | Query | What it checks |
|---|---|---|---|
| `typescript-call` | TS | `foo($X)` | single metavar capture |
| `javascript-multi-capture` | JS | `log($$$ARGS)` | variadic `$$$` capture |
| `python-call` | Py | `print($X)` | Python grammar |
| `rust-call-kind` | Rust | `kind: call_expression` | kind rule, no pattern |
| `css-declaration` | CSS | `.btn { color: $C; }` | CSS grammar + metavar |

**Pass condition:** `PASS: 5 CLI-compatible structural cases matched ast-grep output.`

**Failure means** the Octocode matcher diverges from ast-grep on match text for
at least one case. The error line prints both sorted arrays for direct diff.

---

## Check 4 — Kind match counts must be identical across all four timing lanes

`benchmark/ast-grep/compare-upstream-scenarios.mjs` runs `ast-grep run --kind`
and three Octocode execution paths on an identical deterministic corpus and
asserts all four lanes return the same match count.

```bash
yarn workspace octocode build:dev
yarn workspace @octocodeai/octocode-benchmark ast:compare:upstream -- --sync-repos
```

The four lanes:

| Lane | What it executes |
|---|---|
| `ast-grep CLI` | `ast-grep run --json=stream --kind <kind> <corpus>` — external Rust process, zero Octocode code |
| `octocode raw native` | `engine.structuralSearchFiles({...})` — Rust/NAPI, no validation, no result shaping |
| `octocode localSearchCode tool` | `executeDirectTool('localSearchCode', {...})` — full agent-safe path with schema validation, path security, sanitizer, pagination |
| `octocode search CLI` | `node octocode.js search ... --pattern/--rule --json` — full public CLI including Node startup and JSON serialization |

**Pass condition:** all six scenarios show `MATCH` in the status column (0 DIFF,
0 errors). A `DIFF +X%` row means Octocode found more or fewer matches than
ast-grep on the same files — that is a correctness gap.

To fail fast on the first mismatch instead of completing all scenarios:

```bash
yarn workspace @octocodeai/octocode-benchmark ast:compare:upstream \
  -- --sync-repos --strict
```

One scenario only:

```bash
yarn workspace @octocodeai/octocode-benchmark ast:compare:upstream \
  -- --sync-repos --scenario gin-middleware-routing
```

---

## Check 5 — Corpus identity must be stable across runs

Each run prints a SHA-256 prefix (first 8 hex chars) for the corpus. Verify:

1. The same 8-char prefix appears on every run of the same scenario with the
   same `--files-per-scenario` and `--max-file-bytes`.
2. If the corpus hash changes between runs, a repo's pinned commit has drifted.
   Re-run with `--sync-repos` to re-anchor the shallow clone.

Corpus determinism guards (built into the runner):
- Scenario repos are pinned to exact commits in `upstream-outline-scenarios.json`.
- Files are selected by sorted `git ls-files` output — not filesystem order.
- `--files-per-scenario` and `--max-file-bytes` are fixed inputs.
- Hidden path segments (`.git`, dotfiles) are excluded from both tools.
- The SHA-256 covers file paths AND contents, not just names.

Match-count stability is also enforced: any variation across the `--repeats`
runs within a single lane is a hard error that aborts the run.

---

## Check 6 — Feature parity coverage

This table maps ast-grep's public CLI features to what our checks cover:

| ast-grep feature | CLI flag | Coverage |
|---|---|---|
| Node kind matching | `--kind <kind>` | ✓ Check 4 (all six scenarios) |
| Single metavar pattern | `--pattern 'foo($X)'` | ✓ Check 3 (typescript-call, python-call, css-declaration) |
| Variadic metavar `$$$` | `--pattern 'log($$$ARGS)'` | ✓ Check 3 (javascript-multi-capture) |
| Named metavar (e.g. `$BODY`) | `--pattern '...$BODY...'` | ✓ implied by `$X` and `$C` in Check 3 |
| Explicit language flag | `--lang <lang>` | covered implicitly — corpus is extension-filtered, both tools agree |
| YAML rule with `kind` | `--rule file.yaml` | ✓ Check 4 uses `rule:\n  kind: ...` in Octocode; ast-grep uses `--kind` directly |
| YAML rule with `pattern` | `rule:\n  pattern: ...` | internal only (check-ast.mjs); NOT compared against ast-grep CLI |
| Composite rules (`all`, `any`, `not`, `has`, `inside`, `follows`, `precedes`) | YAML `rule:` | NOT compared — out of scope for this recipe |
| Rewrite / transform | `--rewrite` | NOT applicable — Octocode is read-only |
| `ast-grep scan` (multi-rule lint) | `scan` subcommand | NOT applicable — Octocode has no scan/lint surface |
| `ast-grep outline` (code structure view) | `outline` subcommand | DIFFERENT tool — Octocode has no equivalent (see Check 7) |

**Adding a comparison case:**

1. Add a case object to `CASES` in `benchmark/ast/compare-ast-grep-cli.mjs` with
   `pattern` set to the ast-grep pattern string and `content` containing source
   that will match.
2. Run `ast:compare` and confirm `PASS` before landing.
3. The comparison function requires `trim()` + `sort()` on match text arrays —
   do not compare only counts; text equality is the contract.
4. For `kind`-only comparisons on real corpora, add to `COMPARISON_CASES` in
   `compare-upstream-scenarios.mjs` instead.

---

## Check 7 — What the upstream benchmark actually measures

The `benchmark/ast-grep/upstream-outline-scenarios.json` manifest is a compact
copy of ast-grep's own benchmark at commit
`0af4b77cb07366a52f72180b2c850f64e9f6e455` of `ast-grep/ast-grep`. Understanding
what that benchmark does and does **not** measure matters for interpreting our
results correctly.

**What ast-grep's upstream benchmark measures:**
- `benchmarks/outline-benchmark.md` — an *agent/LLM benchmark* that runs real
  `claude -p` sessions with and without `ast-grep outline` to compare answer
  quality, tool-call count, and token cost on architecture questions.
- `benchmarks/outline_claude_benchmark.py` — the Python runner for those sessions.
- This is **not** a structural grep performance or correctness benchmark.

**What we use from it:**
- The same seven scenario repositories (VS Code, Excalidraw, Django, Tokio,
  OkHttp, Gin, Alamofire) pinned to the same exact commits.
- We treat those repos as a deterministic structural-grep corpus and compare
  both CLIs on identical file subsets.

**Why this is valid:**
- The repos are large, diverse, multi-language, and publicly pinned — good
  corpus properties for a structural search comparison.
- ast-grep and Octocode agree on file enumeration when given the same directory
  and extension filter — confirmed by matching corpus hashes.
- ast-grep's contributing guide says *"ast-grep's benchmarking suite is not well
  developed yet. The result may fluctuate too much."* No formal structural search
  benchmark exists from them. This benchmark fills that gap.

**What it does NOT tell you:**
- How `ast-grep outline` compares to anything in Octocode — that is a separate
  agent-side feature Octocode does not expose.
- Performance of composite YAML rules (`all`/`any`/`has`/`inside`) — only `kind`
  and `pattern` queries are compared.

---

## Check 8 — Grammar correctness (no ast-grep required)

```bash
yarn workspace octocode build:dev
yarn workspace @octocodeai/octocode-benchmark ast:check
```

This validates all 19 shipped tree-sitter grammars through the NAPI binary on
real source samples WITHOUT invoking ast-grep. Per grammar it proves:

- **PARSE** — `structuralSearch(<real sample>, "$$$")` returns nodes (confirms
  grammar loads and parses real code; catches ABI mismatches).
- **MATCH** — `structuralSearch(<canonical snippet>, <pattern-or-rule>)` returns
  the expected node(s) with `min` count.
- **SIGNATURE** — `extractSignatures(<real sample>)` returns a non-empty
  skeleton for signature-tier grammars.

A coverage assertion ensures every extension in
`getSupportedStructuralExtensions()` is covered by exactly one grammar entry —
new grammars without a sample fail the check before shipping.

---

## Quick reference — all ast-grep check commands

```bash
# Check 1: prove no crate link
cargo tree --manifest-path packages/octocode-engine/Cargo.toml --edges normal \
  | rg 'ast-grep|ast_grep'

# Check 2: verify CLI
ast-grep --version

# Check 3: match text parity on 5 mini cases (requires ast-grep CLI)
yarn workspace octocode build:dev
yarn workspace @octocodeai/octocode-benchmark ast:compare

# Check 4: kind match count parity on 6 upstream corpora (requires ast-grep CLI)
yarn workspace @octocodeai/octocode-benchmark ast:compare:upstream -- --sync-repos

# Check 4, one scenario:
yarn workspace @octocodeai/octocode-benchmark ast:compare:upstream \
  -- --sync-repos --scenario gin-middleware-routing

# Check 4, strict (fail on any mismatch):
yarn workspace @octocodeai/octocode-benchmark ast:compare:upstream \
  -- --sync-repos --strict

# Check 8: grammar correctness, no ast-grep needed
yarn workspace @octocodeai/octocode-benchmark ast:check
```
