# Workflow — Engineering Research Recipes

Step-focused recipes built on the native octocode toolset. Prefer the CLI quick commands; use MCP/raw tools only when the CLI is unavailable or a schema-exact field is needed. For flags and the CLI/MCP fallback map, see [context_cli_mcp_commands.md](./context_cli_mcp_commands.md). For presenting findings, see [template_artifact_report.md](./template_artifact_report.md). For AST patterns, see [context_ast_pattern_cookbook.md](./context_ast_pattern_cookbook.md). For the graph research six-step algorithm and safe-delete decision logic, see [workflow-graph.md](./workflow-graph.md).

Notation: each step names the **job**; default to CLI quick commands and use MCP/raw tools only when needed. Shorthand: `grep`=`octocode grep`/`localSearchCode`, `lsp …`=`octocode lsp`/`lspGetSemantics`, `ast`=`octocode grep --pattern/--rule`/`localSearchCode(mode:"structural")`, `symbols`=`octocode ls --symbols` or `octocode cat --mode symbols`/`documentSymbols`, `cat`=`octocode cat`/`localGetFileContent`, `ls`/`find`=structure/file tools.

---

## Quick decision table

| Question | Approach |
|----------|----------|
| "What does this codebase look like?" | `ls --depth 2` → `find --size-greater` (largest) → `find --modified-within` (churn) |
| "Does pattern X exist?" | `ast` with a pattern from [context_ast_pattern_cookbook.md](./context_ast_pattern_cookbook.md) |
| "Where is X defined?" | `grep X` → `lsp definition` |
| "Who calls function X?" | `grep X` → `lsp callers` |
| "All usages of type/var X?" | `grep X` → `lsp references` |
| "Is export X dead?" | `search target:"research"` candidate row → `lsp references` (exclude decl) + `ast` import search proof |
| "Are there unused files/deps?" | `search target:"research"` candidate rows → knip or exact manifest/import proof before deletion |
| "Read this function" | `cat --match-string "X" --mode none` |
| "Trace flow A → B" | `grep` → `lsp definition` → `lsp callers/callees` → hop |
| "Architecture hotspots?" | fan-in (`lsp references` count) ∩ large span (`symbols`) ∩ churn — *approximation, flag it* |
| "Structural smells?" | batch `ast` patterns |
| "Did my fix work?" | re-run the relevant `ast`/`lsp` checks + project lint/test/build |

---

## Core workflows

> **LSP relational reliability — read before Workflows 3, 4, 11, 12.** `references`/`callers`/`implementation` are bounded by open files, so an **empty or low result ≠ unused/safe**. Three habits: (1) **Load the consumers first** — batch a `documentSymbols`/`definition` query on likely callers before trusting a zero. (2) **Prefer `callers` for cross-package blast radius**; `references` is same-package only; Python/C++ → use `references`. (3) **Reuse the anchor** — carry the `grep`/`ast` match line as both `matchString` in `cat` and `lineHint` in `lsp`; batch up to 5 independent lookups per call. When a relational result can't be verified, **lower confidence — don't assert "dead" or "safe".**

### 1 — New codebase orientation

1. `ls --depth 2` → top-level shape.
2. `find --size-greater 10k --sort size` → largest files (hotspot candidates).
3. `find --modified-within 30d` → recent churn.
4. `grep` for entry points (`main`, `index`, server bootstrap, `process.argv`).
5. `symbols` on the 2–3 biggest/most-central files → API surface.
6. `ast` `class_declaration` / default-export patterns → conventions.

### 2 — Symbol deep dive

1. `grep <symbol>` → file + a real `line`.
2. `cat --match-string` → read the body.
3. `lsp definition` → canonical declaration.
4. `lsp callers` → who depends on it.
5. `lsp callees` → what it depends on.

### 3 — Impact analysis (quick blast radius)

1. `grep <symbol>` → `line`.
2. `lsp references` (exclude declaration) → split test vs production. For cross-package reach use `lsp callers`; `references` only covers the same package (see the reliability note above — load consumers before trusting a low count).
3. Few prod refs + tests present = safe. Many = plan carefully → Workflow 11.

### 4 — Dead export / package-drift validation

1. Run Smart OQL first for the candidate universe:
   `octocode search --query '{"target":"research","from":{"kind":"local","path":"."},"params":{"goal":"find unused exports, transitive dead code, unused files, and package drift","mode":"analyze"}}' --json`.
2. Use `data.symbols` (`symbol`, `kind`, `file`, `line`, `directRefs`, `externalRefs`, `retainedBy`, `verdict`) to choose high-value rows. Treat the envelope as candidate evidence, especially when counts greatly exceed knip.
3. For each deletion candidate, run `lsp references` excluding declaration — **but a zero may just be open-file scope**, so load likely consumers first (batch `documentSymbols`/`definition` on them, or `symbols`/`cat`) and re-query before treating it as evidence.
4. `ast` import-statement search for the name → none = no static import.
5. `grep` the name broadly → catch dynamic/computed/re-exported usage.
6. 0 across Smart OQL + LSP + AST + broad grep = confirmed dead. Any reachable/static hit = dismiss or lower confidence.
7. For unused files/dependencies, confirm with knip or exact manifest/import evidence before deleting; Smart OQL's entrypoint graph is useful but not framework-complete.

### 5 — Code-smell sweep

1. Batch `ast` patterns (empty-catch, any/assertion, console, nested-ternary, await-in-loop) from [context_ast_pattern_cookbook.md](./context_ast_pattern_cookbook.md).
2. Read context around each `file:line`.
3. `lsp callers` on the enclosing function to gauge blast radius before recommending a fix.

### 6 — Dependency cycle tracing (native, provable)

1. Suspect A ↔ B: `grep`/`ast` import lines in A for B, and in B for A.
2. Both directions present = a real cycle (this one needs no metric tool).
3. `lsp definition` to hop through the chain and find the back-edge.
4. Fix by inverting one edge through a shared contract.
   *For SCC clusters across many modules, native search only finds them one pair at a time — flag and offer `dep-cruiser` ([context_external_measurement_tools.md](./context_external_measurement_tools.md)).*

### 7 — Security sink validation

1. `ast` for sinks (`eval`, `exec`, innerHTML assignment, command exec).
2. `grep` for guards (validate/sanitize/normalize) near the sink.
3. `cat` the sink context.
4. `lsp callers` → trace who feeds data in (the source).
5. `lsp references` on the sink → exposure breadth.

### 8 — Coupling hotspot (approximation — flag it)

1. Candidate module: `lsp references` count on its exports ≈ fan-in (`Ca`).
2. `grep` its import lines ≈ fan-out (`Ce`).
3. `find --size-greater` / `symbols` → module size.
4. High fan-in + large = decomposition candidate. **Say it's reasoned from counts, not a measured metric**; offer `dep-cruiser` for the real graph.

### 9 — Fix verification loop

1. Re-run the `ast` patterns over changed dirs → smell gone.
2. `cat` the fixed code → looks right.
3. `lsp references`/`callers` on moved/renamed symbols → still resolve.
4. Project toolchain: lint (`--fix`), tests, build/type-check.

---

## Change-safety workflows

### 10 — Smart coding (impact-aware change)

**Before** — define the behavior contract (current/desired/invariants/non-goals); `ls`/`cat`/`lsp definition` to understand the area; **blast radius** via `lsp references` (prod vs test) + `lsp callers`; check for cycle risk (Workflow 6); find an existing pattern to follow (`ast`/`grep`).

**Make the change.**

**After** — run tests; `ast` sweep for new smells; `lsp references`/`callers` intact; verify user-facing contracts (CLI `--help` / API checks); update docs if behavior changed; lint + build.

**Gates:** >5 prod consumers → blast-radius gate; touches a cycle member → extra caution; new smells → fix before commit.

### 11 — Refactoring plan (safe restructure)

1. `grep <symbol>` file-list mode → every file mentioning it.
2. `lsp references` (exclude decl) → consumer count, split test vs prod (0 test = coverage risk).
3. `ast` import search → static import map.
4. `lsp callers`/`callees` → call graph around the target.
5. Coupling/cycle risk via Workflow 6 + 8 (flag approximations).
   **Output:** file list + consumer split + import graph + (approximated) coupling risk = confidence level.

### 12 — Code review / change-impact

1. `cat` the changed regions.
2. Per changed symbol: `grep` → `lsp references` (prod/test) → `lsp callers`.
3. `ast` on changed dirs → new import patterns + new smells.
4. `lsp references` filtered to test dirs → is each change tested?
   **Output:** consumer impact + new quality issues + test coverage = review verdict.

### 13 — CLI / API contract safety

- **CLI:** `grep` for the arg parser (`process.argv`, commander, yargs) + `find` `cli/bin/command` files; `cat` command/option defs; `grep` flag names across tests + docs; run with `--help`, happy path, bad input → check stdout/stderr/exit codes. Checklist: names, aliases, defaults, positional args, exit codes, machine output, backward compat.
- **API:** `grep` router/handler/resolver + schema/contract files; `cat` route defs; `grep` response/DTO types → `lsp references` on shared types → `lsp callees` handler→service; run integration/contract tests. Checklist: request schema, response shape, status codes, error bodies, auth, pagination, idempotency, versioning, migration notes.

### 14 — Docs & rollout sync

1. `find` README / markdown / OpenAPI / `*.example` + `grep` changed names in docs.
2. Update docs/help/examples/migration notes; decide feature-flag/rollout/telemetry/rollback when behavior changed.
3. Run docs build/check if present.
   **Output:** updated docs list + compatibility note + rollout/rollback plan, or an explicit "no public docs/rollout work needed."

---

## History recipes (the "why")

For history and PR archaeology patterns, see [research_external.md](./research_external.md) §History and PR archaeology. Key rule: use history to recover the rationale behind a contract before you change it.
