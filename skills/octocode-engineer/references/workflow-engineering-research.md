# Workflow — Engineering Research Recipes

Step-focused recipes built on the native octocode toolset. Prefer the CLI quick commands; use MCP/raw tools only when the CLI is unavailable or a schema-exact field is needed. For flags and the CLI/MCP fallback map, see [context-cli-mcp-commands.md](./context-cli-mcp-commands.md). For presenting findings, see [template-artifact-report.md](./template-artifact-report.md). For AST patterns, see [context-ast-pattern-cookbook.md](./context-ast-pattern-cookbook.md). For the graph research six-step algorithm and safe-delete decision logic, see [workflow-graph.md](./workflow-graph.md).

Notation: each step names the **job**; default to CLI quick commands and use MCP/raw tools only when needed. Shorthand: `search`=`octocode search`/`localSearchCode`, `semantics …`=`octocode search --op …`/`lspGetSemantics`, `ast`=`octocode search --pattern/--rule --lang <lang>`/`localSearchCode(mode:"structural")`, `symbols`=`octocode search --symbols` or `octocode search --content-view symbols`/`documentSymbols`, `read`=`octocode search --content-view exact`/`localGetFileContent`, `tree`=`octocode search --tree`/structure tools, file discovery=`octocode search --search path`/target:"files".

---

## Quick decision table

| Question | Approach |
|----------|----------|
| "What does this codebase look like?" | `tree --depth 2` → `search . --search path --size-greater 10k --sort size` (largest) → `search . --search path --modified-within 30d` (churn) |
| "Does pattern X exist?" | `ast` with a pattern from [context-ast-pattern-cookbook.md](./context-ast-pattern-cookbook.md) |
| "Where is X defined?" | `search X` → `semantics definition` |
| "Who calls function X?" | `search X` → `semantics callers` |
| "All usages of type/var X?" | `search X` → `semantics references` |
| "Is export X dead?" | `search target:"research"` candidate row → `semantics references` (exclude decl) + `ast` import search proof |
| "Are there unused files/deps?" | `search target:"research"` candidate rows → knip or exact manifest/import proof before deletion |
| "Read this function" | `search <file> --match-string "X" --content-view exact` |
| "Trace flow A → B" | `search` → `semantics definition` → `semantics callers/callees` → hop |
| "Architecture hotspots?" | fan-in (`semantics references` count) ∩ large span (`symbols`) ∩ churn — *approximation, flag it* |
| "Structural smells?" | batch `ast` patterns |
| "Did my fix work?" | re-run the relevant `ast`/`semantics` checks + project lint/test/build |

---

## Core workflows

> **Semantic relational reliability — read before Workflows 3, 4, 11, 12.** `references`/`callers`/`implementation` are bounded by open files, so an **empty or low result ≠ unused/safe**. Three habits: (1) **Load the consumers first** — batch a `documentSymbols`/`definition` query on likely callers before trusting a zero. (2) **Prefer `callers` for cross-package blast radius**; `references` is same-package only; Python/C++ → use `references`. (3) **Reuse the anchor** — carry the `search`/`ast` match line as both `matchString` in `read` and `lineHint` in semantic search; batch up to 5 independent lookups per call. When a relational result can't be verified, **lower confidence — don't assert "dead" or "safe".**

### 1 — New codebase orientation

1. `tree --depth 2` → top-level shape.
2. `search . --search path --size-greater 10k --sort size` → largest files (hotspot candidates).
3. `search . --search path --modified-within 30d` → recent churn.
4. `search` for entry points (`main`, `index`, server bootstrap, `process.argv`).
5. `symbols` on the 2–3 biggest/most-central files → API surface.
6. `ast` `class_declaration` / default-export patterns → conventions.

### 2 — Symbol deep dive

1. `search <symbol>` → file + a real `line`.
2. `read --match-string` → read the body.
3. `semantics definition` → canonical declaration.
4. `semantics callers` → who depends on it.
5. `semantics callees` → what it depends on.

### 3 — Impact analysis (quick blast radius)

1. `search <symbol>` → `line`.
2. `semantics references` (exclude declaration) → split test vs production. For cross-package reach use `semantics callers`; `references` only covers the same package (see the reliability note above — load consumers before trusting a low count).
3. Few prod refs + tests present = safe. Many = plan carefully → Workflow 11.

### 4 — Dead export / package-drift validation

1. Run Smart OQL first for the candidate universe:
   `octocode search --query '{"target":"research","from":{"kind":"local","path":"."},"params":{"goal":"find unused exports, transitive dead code, unused files, and package drift","mode":"analyze"}}' --json`.
2. Use `data.symbols` (`symbol`, `kind`, `file`, `line`, `directRefs`, `externalRefs`, `retainedBy`, `verdict`) to choose high-value rows. Treat the envelope as candidate evidence, especially when counts greatly exceed knip.
3. For each deletion candidate, run `semantics references` excluding declaration — **but a zero may just be open-file scope**, so load likely consumers first (batch `documentSymbols`/`definition` on them, or `symbols`/`read`) and re-query before treating it as evidence.
4. `ast` import-statement search for the name → none = no static import.
5. `search` the name broadly → catch dynamic/computed/re-exported usage.
6. 0 across Smart OQL + LSP + AST + broad search = confirmed dead. Any reachable/static hit = dismiss or lower confidence.
7. For unused files/dependencies, confirm with knip or exact manifest/import evidence before deleting; Smart OQL's entrypoint graph is useful but not framework-complete.

### 5 — Code-smell sweep

1. Batch `ast` patterns (empty-catch, any/assertion, console, nested-ternary, await-in-loop) from [context-ast-pattern-cookbook.md](./context-ast-pattern-cookbook.md).
2. Read context around each `file:line`.
3. `semantics callers` on the enclosing function to gauge blast radius before recommending a fix.

### 6 — Dependency cycle tracing (native, provable)

1. Suspect A ↔ B: `search`/`ast` import lines in A for B, and in B for A.
2. Both directions present = a real cycle (this one needs no metric tool).
3. `semantics definition` to hop through the chain and find the back-edge.
4. Fix by inverting one edge through a shared contract.
   *For SCC clusters across many modules, native search only finds them one pair at a time — flag and offer `dep-cruiser` ([measurement-tools.md](./measurement-tools.md)).*

### 7 — Security sink validation

1. `ast` for sinks (`eval`, `exec`, innerHTML assignment, command exec).
2. `search` for guards (validate/sanitize/normalize) near the sink.
3. `read` the sink context.
4. `semantics callers` → trace who feeds data in (the source).
5. `semantics references` on the sink → exposure breadth.

### 8 — Coupling hotspot (approximation — flag it)

1. Candidate module: `semantics references` count on its exports ≈ fan-in (`Ca`).
2. `search` its import lines ≈ fan-out (`Ce`).
3. `search . --search path --size-greater <threshold>` / `symbols` → module size.
4. High fan-in + large = decomposition candidate. **Say it's reasoned from counts, not a measured metric**; offer `dep-cruiser` for the real graph.

### 9 — Fix verification loop

1. Re-run the `ast` patterns over changed dirs → smell gone.
2. `read` the fixed code → looks right.
3. `semantics references`/`callers` on moved/renamed symbols → still resolve.
4. Project toolchain: lint (`--fix`), tests, build/type-check.

---

## Change-safety workflows

### 10 — Smart coding (impact-aware change)

**Before** — define the behavior contract (current/desired/invariants/non-goals); `tree`/`read`/`semantics definition` to understand the area; **blast radius** via `semantics references` (prod vs test) + `semantics callers`; check for cycle risk (Workflow 6); find an existing pattern to follow (`ast`/`search`).

**Make the change.**

**After** — run tests; `ast` sweep for new smells; `semantics references`/`callers` intact; verify user-facing contracts (CLI `--help` / API checks); update docs if behavior changed; lint + build.

**Gates:** >5 prod consumers → blast-radius gate; touches a cycle member → extra caution; new smells → fix before commit.

### 11 — Refactoring plan (safe restructure)

1. `search <symbol> --view discovery` → every file mentioning it.
2. `semantics references` (exclude decl) → consumer count, split test vs prod (0 test = coverage risk).
3. `ast` import search → static import map.
4. `semantics callers`/`callees` → call graph around the target.
5. Coupling/cycle risk via Workflow 6 + 8 (flag approximations).
   **Output:** file list + consumer split + import graph + (approximated) coupling risk = confidence level.

### 12 — Code review / change-impact

1. `search --content-view exact` for the changed regions.
2. Per changed symbol: `search` → `semantics references` (prod/test) → `semantics callers`.
3. `ast` on changed dirs → new import patterns + new smells.
4. `semantics references` filtered to test dirs → is each change tested?
   **Output:** consumer impact + new quality issues + test coverage = review verdict.

### 13 — CLI / API contract safety

- **CLI:** `search` for the arg parser (`process.argv`, commander, yargs) + `search --search path` for `cli/bin/command` files; `read` command/option defs; `search` flag names across tests + docs; run with `--help`, happy path, bad input → check stdout/stderr/exit codes. Checklist: names, aliases, defaults, positional args, exit codes, machine output, backward compat.
- **API:** `search` router/handler/resolver + schema/contract files; `read` route defs; `search` response/DTO types → `semantics references` on shared types → `semantics callees` handler→service; run integration/contract tests. Checklist: request schema, response shape, status codes, error bodies, auth, pagination, idempotency, versioning, migration notes.

### 14 — Docs & rollout sync

1. `search --search path` README / markdown / OpenAPI / `*.example` + `search` changed names in docs.
2. Update docs/help/examples/migration notes; decide feature-flag/rollout/telemetry/rollback when behavior changed.
3. Run docs build/check if present.
   **Output:** updated docs list + compatibility note + rollout/rollback plan, or an explicit "no public docs/rollout work needed."

---

## History recipes (the "why")

For history and PR archaeology patterns, see [research-external.md](./research-external.md) §History and PR archaeology. Key rule: use history to recover the rationale behind a contract before you change it.
