---
name: octocode-engineer
description: "System-engineering skill for codebase understanding, bug investigation, refactors, PR safety, architecture review, and RFC validation. Enforces Clean Architecture and Clean Code with AST, LSP, and scanner evidence. Produces a flows / boundaries / architecture-health artifact with file:line citations before recommending action."
---

# Octocode Engineer

Understand, change, and verify a codebase with system awareness. Single-file reading misses root causes — they live in boundaries, flow ownership, contracts, data paths, and runtime assumptions. This skill makes those visible before you act, and keeps them verified after.

## What you get (user view)

A structured **understanding artifact**, grounded in evidence, every claim cited `file:line`:

- **System summary** (what/who/invariants) · **Control flows** (numbered call paths) · **Data flows** (writers/readers/txn/cache per entity) · **Types & protocols** (DTOs, schemas, wire contracts, compat)
- **Boundaries & ownership** (owners, ports, contract tests) · **Structure health** (folder bloat, file/folder naming, project-fit) · **Duplication inventory** (near-clones, missing abstraction) · **Execution profile** (hot paths, async/sync, retries/timeouts/lifecycles)
- **Architecture health** (per-principle + per-dimension, `confirmed|likely|uncertain`) · **Clean-code hotspots** · **Next step** (1 sentence)

For a change: change flow, data-flow impact, contract impact, blast radius, risk vector. **Safety built in** — hard gates stop for your decision before public-contract changes, cross-layer edits, destructive actions, or large blast radius.

## How to invoke (user view)

Ask in natural language. The skill activates on phrases like: *"understand this codebase"*, *"deep-dive this feature"*, *"review the architecture of X"*, *"why is this slow / flaky / coupled?"*, *"is this PR safe?"*, *"what breaks if I change Y?"*, *"prepare to refactor Z"*, *"validate this RFC against the code"*.

## Quick decision cheatsheet (agent view)

Use this first to pick the cheapest proof path. Every LSP call needs a `lineHint` from `localSearchCode` — **never guess**.

| Question | Tool chain |
|---|---|
| Where is X defined? | `localSearchCode(X)` → `lspGotoDefinition(lineHint=N)` |
| Who calls function X? | `localSearchCode(X)` → `lspCallHierarchy(incoming, lineHint=N)` |
| What does X call? | `localSearchCode(X)` → `lspCallHierarchy(outgoing, lineHint=N)` |
| All usages of a type / var / non-function X? | `localSearchCode(X)` → `lspFindReferences(lineHint=N)` |
| Is this pattern duplicated? | `scripts/ast/search.js --pattern` → scanner `duplicate-*` findings |
| Is this shape an antipattern? | `scripts/ast/search.js --preset <name>` (list: `--list-presets`) |
| Is this module structurally unhealthy? | `scripts/run.js --graph --scope=<path>` → read `scan.json` |
| Is the project structure healthy? | `localViewStructure` + `localFindFiles` → `scripts/run.js --scope=<path> --graph` → inspect `qualityRating` folder/naming/consistency signals + `mega-folder` findings |
| Which layer/boundary does this cross? | Scanner layer output + `lspGotoDefinition` across packages |
| What breaks if I change Y? | `lspFindReferences(Y)` → label consumers by layer |
| Find files by name / churn / size | `localFindFiles` |
| Read implementation (last resort) | `localGetFileContent` with `matchString` |

For longer research recipes and end-to-end tool sequences, see [tool-workflows.md](./references/tool-workflows.md).

### References index

| Situation | Reference |
|---|---|
| Tool workflows, research recipes | [tool-workflows.md](./references/tool-workflows.md) |
| Scanner flags, thresholds, scope syntax, exit codes | [cli-reference.md](./references/cli-reference.md) |
| Reading scan artifacts | [output-files.md](./references/output-files.md) |
| AST presets, pattern syntax, Python kinds | [ast-reference.md](./references/ast-reference.md) |
| Confirming / dismissing a finding | [validation-playbooks.md](./references/validation-playbooks.md) |
| Detector catalog, metrics, severities | [quality-indicators.md](./references/quality-indicators.md) |
| How to present findings | [output-format.md](./references/output-format.md) |
| eslint, tsc, knip, ruff, mypy | [externals.md](./references/externals.md) |

## Operating contract (agent view)

Every **non-trivial** task MUST satisfy this contract:

1. **Scope** — restate the goal and constraints in one line before touching tools.
2. **Lenses** — apply both required lenses defined in §Clean Architecture & Clean Code: the five Clean-Architecture principles and the six analytic dimensions.
3. **Evidence** — prove every architectural or code-quality claim with at least one of: Octocode local tools, LSP, AST, scanner. Mark confidence (`confirmed|likely|uncertain`) with source.
4. **Artifact** — produce the understanding artifact (§Required output) before recommending action.
5. **Gates** — stop at every hard gate in §User-Ask Gates.
6. **Tool universe** — never fall back to native Claude Code search tools (`Grep`, `Glob`, `Read`) while Octocode MCP is registered. A warning inside a successful Octocode response is not a failure; see §Fallback Mode for the only legitimate fallback conditions.

## When To Use It

Use when the user asks to **understand** a codebase/feature end-to-end, **change** unclear/shared/cross-file code, **review** quality/architecture/tech-debt/dead-code/security/build issues, or **decide** architecture and validate RFCs against real behavior. Any language; strongest on Node/TypeScript and Python. For architecture options, trade-offs, or migration strategy that need a formal proposal before coding, pair with [octocode-rfc-generator](https://skills.sh/bgauryy/octocode-mcp/octocode-rfc-generator).

## Trivial vs. non-trivial — when the contract binds

The contract, lenses, and artifact apply to **non-trivial** tasks. A task is **trivial** only when ALL hold: single file; no public/exported symbol touched; 0 consumers (per `lspFindReferences`) or behavior-preserving for all; no contract/schema/protocol/config/migration touched; ≤ ~20 lines; no cross-layer/cross-package edit. Otherwise non-trivial (default on doubt). Trivial tasks: deliver the one-line next step + verification only.

## Clean Architecture & Clean Code (Required Lenses)

Non-trivial investigations MUST go through both lenses. Prove every claim with the listed tools — no unevidenced architectural or code-quality facts.

### Clean Architecture — what to enforce, how to verify

1. **Dependency rule** — source code dependencies point inward. Domain never imports infrastructure/UI; use cases never import frameworks.
2. **Layer boundaries** — entities → use cases → interface adapters → frameworks & drivers. Concerns stay in their layer.
3. **Stable abstractions** — volatile details depend on stable policy, never the reverse.
4. **Boundary ownership** — every cross-boundary call goes through an explicit port (interface/DTO). Implementation types do not leak.
5. **Single responsibility per module** — one reason to change; one axis of volatility.

| Principle | Tool | Evidence to collect |
|-----------|------|---------------------|
| Dependency rule | `scripts/run.js --graph` + `lspFindReferences` | layer-violation / SDP findings; inward-pointing edges only |
| Layer boundaries | `localSearchCode` on import lines + scanner layer output | UI→DB, domain→HTTP, adapter→framework leaks |
| Stable abstractions | scanner `distance-from-main-sequence` | concrete high-fan-in modules, unstable abstractions |
| Boundary ownership | `lspGotoDefinition` across package boundaries | types crossing boundaries without a port |
| Single responsibility | scanner + `scripts/ast/search.js` (`--preset class-declaration`, `god-function`) | god modules, multi-purpose classes, wide exports |

### Architect's analytic dimensions

Cover all six on a full review; on a scoped task, cover those the change touches and mark the rest `N/A` with a one-line reason (`N/A` is a claim, not silence). Mapping to artifact sections is encoded in §Required output. On a change, state which dimensions it stresses — that is the risk vector.

| # | Dimension | Verify | Anti-patterns |
|---|-----------|--------|---------------|
| 1 | **Flows** — entry → collaborators → side effects → return/emit | `localSearchCode`(entry,lineHint) → `lspCallHierarchy` incoming/outgoing → `scripts/run.js` flow/graph on hot paths | hidden event jumps; unenumerable middleware chains; untested error branches |
| 2 | **Duplication** — same logic in two places drifts | scanner (`duplicate-function-body`, `duplicate-flow-structure`, `similar-function-body`) → `scripts/ast/search.js --pattern` → `lspFindReferences` on canonical version | two sources of truth; drifting copies; per-caller reinvention |
| 3 | **Types** — in-process contracts | `lspGotoDefinition` on boundary params → `lspFindReferences` on type → `scripts/ast/search.js` presets (`any-type`, `type-assertion`, `non-null-assertion`) → scanner (`unsafe-any`, `type-assertion-escape`, `narrowable-type`) | `any`/`unknown` at public boundary; casts silencing compiler; always-populated "optional" fields |
| 4 | **Protocols & schemas** — wire contracts (HTTP/gRPC/GraphQL/SQL/events/config) | `localFindFiles` on `*.proto`, `*.graphql`, `*.sql`, `openapi*`, `schema*`, `migrations/*` → `localGetFileContent` → `lspFindReferences` on generated types → `githubSearchPullRequests` for external changes | schema drift; implicit required fields; defaults in code not schema; version bumps without compat windows; null/missing/empty ambiguity |
| 5 | **Data flows** — state, ownership, mutation | schema + repository/DAO → `lspFindReferences` on write fns (`save`, `update`, `insert`, `publish`) → `scripts/run.js` graph/flow on write paths → `scripts/ast/search.js --kind` on mutations | multi-writers on one field; read-your-writes across async; cache/write races; write paths bypassing validator; projections without consistency guarantees |
| 6 | **Execution** — runtime (sync/async, I/O, retries, timeouts, startup/shutdown, lifecycles) | `scripts/ast/search.js` presets (`async-function`, `await-in-loop`, `sync-io`, `promise-all`) → scanner (`await-in-loop`, `sync-io`, `uncleared-timer`, `unbounded-collection`, `startup-risk-hub`, `listener-leak-risk`) → `lspCallHierarchy` on hot path → tests/benchmarks | `await` in tight loops; sync I/O on request path; timers/listeners without lifecycle; startup assuming init order; retries without backoff/idempotency |

### Clean Code — what to enforce, how to verify

1. **Names reveal intent** — symbols describe what, not how.
2. **Small, single-purpose functions** — one level of abstraction; short; ≤ ~3 params.
3. **No dead or duplicated logic** — every branch reachable; each pattern lives in one place.
4. **Fail loudly, never silently** — no empty catches, no swallowed errors, no bare `except`.
5. **Types are precise at boundaries** — no `any` / no bare `except` / no unchecked casts at contracts.
6. **Comments explain *why*, not *what*** — if the comment restates the code, delete one.

| Rule | Tool | Preset / signal |
|------|------|-----------------|
| Small functions | `scripts/run.js` | `god-function`, `cognitive-complexity`, `halstead-effort`, `excessive-parameters` |
| Duplication | `scripts/run.js` | `duplicate-function-body`, `duplicate-flow-structure`, `similar-function-body` |
| Silent failures | `scripts/ast/search.js` | `--preset empty-catch`, `--preset py-bare-except`, `--preset catch-rethrow` |
| Loose types | `scripts/ast/search.js` | `--preset any-type`, `--preset type-assertion`, `--preset non-null-assertion` |
| Intent-revealing names | code read + `lspFindReferences` | widely-used cryptic symbols, abbreviations that spread |
| Dead / unreachable | scanner + `knip` | `dead-export`, `dead-file`, `unused-import`, `unused-npm-dependency` |

Full detector catalog, metric definitions, and severity rubric: [quality-indicators.md](./references/quality-indicators.md).

### Required output: understanding artifact

Produce before recommending action. **required** sections always appear (use `N/A` + reason if not applicable); **applicable** sections appear only when the task touches that surface. Keep each section ≤ 2 min read.

| # | Section | When | Source dimensions |
|---|---------|------|-------------------|
| 1 | **System summary** — what it does, who consumes it, invariants | required | — |
| 2 | **Control flows** — numbered call paths, each step cited `file:line` | required | Flows |
| 3 | **Data flows** — writers, readers, transaction boundaries, caches per entity | applicable (stateful tasks) | Data flows |
| 4 | **Types & protocols** — boundary DTOs/schemas/wire contracts, compatibility posture | applicable (contract tasks) | Types, Protocols & schemas |
| 5 | **Boundaries & ownership** — module ownership, ports, contract tests | required | — |
| 6 | **Duplication inventory** — top near-clones and the missing abstraction | applicable (refactor / quality) | Duplication |
| 7 | **Execution profile** — hot paths, async/sync posture, retry/timeout/lifecycle, runtime risks | applicable (perf / reliability) | Execution |
| 8 | **Architecture health** — one line per principle and per dimension, with `confirmed|likely|uncertain` + source | required | all |
| 9 | **Clean-code hotspots** — top AST/scanner findings worth fixing, cited `file:line` | applicable (quality work) | — |
| 10 | **Next step** — one sentence | required | — |

Trivial tasks: section 10 + verification only (see §Trivial vs. non-trivial). Section ordering, phrasing, examples: [output-format.md](./references/output-format.md).

If the task involves a change, also include:
- **Change flow** — the specific call path the change traverses. *(required for any change)*
- **Data-flow impact** — entities read/written and how transaction/cache semantics are preserved. *(required if section 3 applied)*
- **Contract impact** — types/schemas/protocols touched and compatibility posture (backwards-compatible / breaking-with-migration / additive-only). *(required if section 4 applied)*
- **Blast radius** — callers and consumers touched, from `lspFindReferences`, labeled by layer. *(required for any change with consumers)*
- **Risk vector** — which clean-architecture principles and which analytic dimensions the change stresses, and how each is preserved. *(required for any change)*

#### Artifact self-check — before closing

A good artifact answers all of: ownership/boundary; blast radius (consumers, layers); contract safety (types/schemas/protocols); local vs structural vs architectural; build/config involvement; reliability under failure/retry/concurrency; observability sufficiency; rollout/migration reversibility; folder bloat and file/folder naming fitness; modularity trajectory; documented assumptions; safest next move. If the answer only explains one file, it is usually incomplete.

## Tool Families And Their Jobs

### 1. Local Octocode tools

First tools for workspace mapping — not a fallback.

| Tool | Use it for |
|------|------------|
| `localViewStructure` | Package/module layout, folder depth, source spread |
| `localFindFiles` | Large files, recent churn, suspicious filenames, likely hotspots |
| `localSearchCode` | Fast discovery, symbol search, text patterns, and `lineHint` for LSP |
| `localGetFileContent` | Final code reading after you know what you are looking at |

Rules:
- Do not start with a full-file read when discovery tools can narrow the target first.
- When `localSearchCode` returns zero matches: (1) widen the pattern (drop regex meta-chars, try a substring), (2) fall back to `localFindFiles` on likely filename patterns, (3) retry with the literal symbol name. Only after that may you broaden to `localViewStructure` for layout reconnaissance.

### 2. LSP tools

Use LSP tools to understand real semantic relationships. `lineHint` rule stated in §Quick decision cheatsheet — applies to every entry below.

| Tool | Use it for |
|------|------------|
| `lspGotoDefinition` | What symbol is this really? |
| `lspFindReferences` | Blast radius, all usages, dead-code checks (types, vars, anything) |
| `lspCallHierarchy` | Function call flow only: incoming callers and outgoing callees |

### 3. AST tools — structural proof

Authoritative proof for code-shape, redundancy, and smell claims; use when text search is too weak.

| Script | Role | Example invocation |
|--------|------|--------------------|
| `scripts/ast/search.js` | Live ast-grep search on current source — authoritative | `node scripts/ast/search.js --preset empty-catch --root ./src` |
| `scripts/ast/search.js` | Project-specific structural claim | `node scripts/ast/search.js --pattern 'if ($C) { return $V }' --json` |
| `scripts/ast/tree-search.js` | Fast triage over cached AST trees from a prior scan | `node scripts/ast/tree-search.js -i .octocode/scan -k function_declaration --limit 25` |

Rules:
- Presets cover the common clean-code rules; list them with `node scripts/ast/search.js --list-presets`.
- Python presets are prefixed `py-` (e.g. `py-bare-except`, `py-mutable-default`).
- Use `tree-search.js` first to narrow, then `search.js` to confirm on live code.
- If a structural claim matters to a decision, confirm it with AST before presenting it as fact.
- Pair every match with its `file:line` in the summary.
- For preset catalog, pattern syntax, and Python node kinds, see [ast-reference.md](./references/ast-reference.md).

### 4. Scanner — architecture and flow

Use `scripts/run.js` when the question is bigger than one symbol or one file. It surfaces dependency cycles, chokepoints, coupling pressure, layer violations, dead-code clusters, security sinks, test gaps, and hot paths — the issues local reading misses.

| Script | Role | Example invocation |
|--------|------|--------------------|
| `scripts/run.js` | Default scoped scan | `node scripts/run.js --scope=packages/my-pkg` |
| `scripts/run.js --graph` | Architecture graph (cycles, SDP, coupling) | `node scripts/run.js --graph --out .octocode/scan/scan.json` |
| `scripts/run.js --json` | Machine-readable findings | `node scripts/run.js --json --out .octocode/scan/scan.json` |

Use scanner output to reason about: where change risk concentrates, whether a module is structurally unhealthy, whether a local fix ignores a broader architectural problem, which area to refactor first. Flags, thresholds, scope syntax, and exit codes: [cli-reference.md](./references/cli-reference.md). Reading the scan artifacts: [output-files.md](./references/output-files.md).

**First-run install.** Scripts auto-install native deps (`tree-search.js` needs none) using the detected package manager (pnpm-lock.yaml → pnpm, yarn.lock → yarn, else npm); on failure they exit non-zero with the manual command. Opt out with `OCTOCODE_NO_AUTO_INSTALL=1`.

**Invoking the scripts.** Skill is `private: true` with no `bin` — `npx octocode-engineer-*` is **invalid**. `npx` applies only to externals in [externals.md](./references/externals.md). Forms:

| Form | Example | When |
|---|---|---|
| Absolute path | `node <SKILL_DIR>/scripts/run.js --scope=packages/my-pkg` | From any cwd (default) |
| `yarn` alias | `cd <SKILL_DIR> && yarn analyze\|analyze:full\|analyze:graph\|analyze:json` | Idiomatic in-skill shortcut |
| `yarn` alias (AST) | `cd <SKILL_DIR> && yarn search\|search:json\|search:presets\|search:trees\|search:trees:json` | AST scripts |
| Raw node (cwd-local) | `cd <SKILL_DIR> && node scripts/run.js [flags]` | When you need flags not covered by an alias |

Full flag catalog + exit codes: [cli-reference.md](./references/cli-reference.md).

**Cost.** Prefer `--scope=<path>` over full-repo; reuse existing artifacts when they answer the question; on staleness re-run only the minimal scope.

### 5. Cross-cutting quality checks

The Clean-Architecture principles and the six analytic dimensions already cover naming, cohesion, duplication, layering, contracts, types, data flow, and execution. Use this section only for concerns **not** directly named there:

| Check | Focus |
|------|------|
| Reliability & resilience | retry policy, timeout handling, failure isolation, idempotency, fallback behavior |
| Observability & operability | logging quality, metric/tracing coverage, diagnosability, alert/runbook readiness |
| Rollout & migration | feature flags, backward-compatibility windows, rollback path, migration sequencing |
| Build & config | ESM/CJS mismatch, module resolution, script wiring, runtime assumptions |
| Structure health | leaf-folder bloat, vague shared/helper buckets, depth balance, source spread, file and folder naming consistency for the project |
| Docs | whether critical assumptions, contracts, flows, setup, migrations, and risks are documented |
| CSS hygiene | selector scope, token reuse, naming clarity, dead styles (when frontend styling is touched) |
| `knip` | unused exports, files, dependencies, dead integration edges (run on refactors) |

Skip items that do not apply. For concrete `npx` commands for `eslint`, `tsc`, `knip`, `stylelint`, `type-coverage`, `dep-cruiser`, `ruff`, `mypy`, and related externals, see [externals.md](./references/externals.md) — **ask before running**.

### 6. Execution discipline

- **Per-step**: declare the next tool and why it is the cheapest proof; separate facts from inference; carry forward concrete identifiers (`lineHint`, paths, symbols); verify explicitly after edits.
- **Status updates**: say what was checked and what remains — no vague progress, no "looks fine" without evidence, no switching to edits without a short flow summary.
- **Depth control**: mark `N/A` on irrelevant checks; go deeper only where risk/uncertainty is meaningful; pick the lightest evidence path.
- **Token efficiency**: one investigation thread at a time; reference prior checkpoints instead of restating evidence; stop research when confidence is sufficient; summarize findings as **issue → evidence → impact → action**.
- **Task tracking**: use todos when the work spans research → plan → implement → verify → docs. Track investigation, decision, implementation, verification, docs follow-up.

## Default Working Order

Non-trivial tasks follow this arc (recommended, not mandatory): clarify the question → create todos if multi-step → map layout with local tools → trace symbols with LSP → identify critical and failure paths → validate structure with AST → check architecture/contracts/reliability/build/docs with the scanner → read the code in context → validate design docs or RFCs against current flows and contracts → apply both lenses (`confirmed|likely|uncertain` with evidence) → produce the artifact → pause at any hard gate → decide to explain, plan, or edit.

## Task shapes

Same working order; emphasis differs. **Code understanding**: steps 3–8 (layout → LSP → AST → scanner → read), deliverable is the artifact. **Bug fixing**: Flows + Execution from failing behavior inward; fix the smallest responsible layer; escalate at the Smallest-fix gate if systemic. **Refactor**: blast radius first (`lspFindReferences`), then scoped scan + duplication inventory; prefer extracting modules and clarifying contracts over cosmetic reshuffling; verify per batch. **Architecture review**: scanner `--graph` first, then LSP on candidates; report local and system-level causes. **RFC/design validation**: map each claim to code ownership; verify flow, contract, and architecture alignment; mark `confirmed|likely|uncertain`.

## Before / During / After A Change

**Before**: produce the understanding artifact; map RFC/design-doc claims to code ownership; look for an existing pattern before inventing one.

**During**: stay in the smallest responsible layer; preserve contract/protocol compatibility unless migration is in scope; if root cause is structural mid-task, stop at the Smallest-fix vs. safest-fix gate.

**After**: run tests, lint, build/type-check; re-check changed symbols with LSP; run scoped scanner pass for non-trivial changes; run `knip` if dead artifacts are likely; sync docs/RFC sections the change touches.

## Confidence Rules

| Level | Meaning | Example |
|-------|---------|---------|
| `confirmed` | ≥2 approaches agree, or one authoritative source | AST proves empty catch + LSP shows function widely used |
| `likely` | good evidence, one angle still missing | scanner hotspot agrees with code shape, blast radius unverified |
| `uncertain` | conflicting / incomplete / single weak source | text search suggests dead code, LSP unavailable |

### Evidence conflict resolution

When sources disagree on a claim that affects a decision, prefer the source whose domain it is, then re-verify the weaker source:

| Claim type | Authoritative source | Corroborator |
|-----------|----------------------|--------------|
| Symbol identity, references, callers/callees | LSP (`lspGotoDefinition`, `lspFindReferences`, `lspCallHierarchy`) | AST + code read |
| Structural shape (empty catch, `any` usage, nested ternary, preset match) | AST (`scripts/ast/search.js`) | scanner + code read |
| Runtime behavior and side effects | targeted code read + tests | AST + scanner |
| Architecture pressure (coupling, cycles, SDP, hot paths) | scanner (`scripts/run.js`) | LSP references + code read |
| Contract/schema shape at a boundary | the schema/IDL file itself + `lspGotoDefinition` | references to generated types |

If the authoritative source contradicts a weaker one, mark the weaker one as "re-verify" in the artifact and note the resolution. Never present conflicting evidence as resolved without a recorded tiebreak.

For step-by-step playbooks that confirm or dismiss a finding (dead code, duplicate, unsafe `any`, layer violation, cycle, etc.), see [validation-playbooks.md](./references/validation-playbooks.md).

## User-Ask Gates

A gate is a **hard stop** — do not proceed without the user's explicit decision.

### Hard gates (always stop and ask)

State situation in ≤3 lines, list options, name tradeoff, recommend one.

1. **Ambiguous scope** — the task has more than one reasonable interpretation and the right one changes the plan.
   _Ambiguous:_ "fix the login bug". _Unambiguous:_ "fix 500 on /api/login when password field is empty".
2. **Public contract change** — a public API, exported symbol, event schema, DB schema, CLI flag, or wire protocol would change.
   _Fires:_ renaming an exported function with external consumers. _Does not fire:_ renaming a private helper with no references.
3. **Cross-layer/cross-package change** — the fix requires editing more than one layer or crosses a workspace boundary.
   _Fires:_ bug fix needs changes in `packages/domain` AND `packages/http-adapter`. _Does not fire:_ edit contained in one package.
4. **Dependency-rule violation required** — the cleanest fix would break the dependency rule, break a boundary, or introduce a new cycle.
   _Fires:_ domain module would need to import the HTTP adapter. _Does not fire:_ adapter importing domain (correct direction).
5. **Destructive or irreversible action** — delete/rename shared files, drop tables, reset branches, force-push, publish packages, send messages/PRs on the user's behalf.
   _Fires:_ `git reset --hard`, `rm -rf`, publishing an npm version. _Does not fire:_ local file edits on a feature branch.
6. **Blast radius > ~5 consumers** — `lspFindReferences` returns many callers and the change alters their behavior.
   _Fires:_ changing a utility called by 20 files. _Does not fire:_ changing a helper with 2 callers, both of which are co-edited.
7. **Two refinement attempts failed** — same approach tried twice and the evidence still doesn't line up.
   _Fires:_ two different search patterns both return empty for a symbol you expected. _Does not fire:_ one failed attempt with a clear next angle.
8. **Missing gate prerequisite** — no tests exist for the area, no owner documented, no schema available, and the change needs one.
   _Fires:_ user asks for a refactor of untested legacy code. _Does not fire:_ tests exist and cover the change surface.
9. **Conflicting evidence** — authoritative and corroborating sources (see §Evidence conflict resolution) disagree on a claim that matters to the decision.
   _Fires:_ LSP says 0 references, AST shows an import of the symbol. _Does not fire:_ scanner is noisy but LSP is clear.
10. **Smallest-fix vs. safest-fix conflict** — a narrow patch would work but the root cause is structural.
    _Fires:_ bug can be fixed by adding a null check but the real cause is a missing contract between two layers. _Does not fire:_ the narrow patch IS the right layer.

### Soft gates (ask if material)

Ask when the decision materially changes the outcome; otherwise proceed and note the assumption.

- Multiple reasonable architectures exist for a greenfield area.
- Framework / library choice where the project has no established pattern.
- Rollout strategy (feature flag vs direct deploy) for a behavior change.
- Migration sequencing when old and new consumers coexist.
- Whether to fix adjacent smells discovered mid-task or log them as follow-ups.

### Ask template

> **Gate:** <what triggered it, 1 line>
> **Options:**
> 1. <option A> — tradeoff
> 2. <option B> — tradeoff
> **Recommendation:** <A or B, 1 line why>
> **Blocking:** <what I will not do until you decide>

Keep it short. The user should be able to respond in one sentence. If the user picks an option you did not recommend, record the decision and the stated reason in the **Architecture health** / **Risk vector** sections and proceed without re-asking. Do not argue against a decided option — raise residual risks once, then execute.

### Gate discipline

- Do not ask when the answer is obvious from the code, CLAUDE.md, or prior context.
- Do not silently continue past a hard gate because "it seemed fine" — that is the failure gates prevent.
- If a gate fires mid-implementation, stop at a clean checkpoint (commit if appropriate, revert if not) and ask.
- After a decision, record it in the artifact so future steps carry it forward.
- **Gates bind regardless of fallback state.** If an Octocode tool is unavailable and you are in §Fallback Mode, gates still fire on the same conditions; lower confidence does not weaken the rule.

## Hard Rules (recap)

Non-negotiable guardrails beyond the §Operating contract (which already binds gates and tool universe):

- Do not present raw detector output as unquestioned fact.
- Do not use `lspCallHierarchy` on non-function symbols — use `lspFindReferences` instead.
- Do not judge shared modules from one file read alone.
- Do not claim design/RFC compliance without claim-by-claim evidence.
- Do not ignore build/config evidence when runtime behavior may depend on it.
- Do not apply a quick patch when the real issue is contracts, boundaries, duplication, or architecture — hit the Smallest-fix vs. safest-fix gate.
- Check blast radius before changing shared symbols.
- Re-sync docs/RFCs when implementation changes architecture, contracts, rollout assumptions, or constraints.

## Fallback Mode

Fallback applies only when an Octocode tool is truly **unavailable** — not registered, unreachable, or returning hard errors. A warning inside a successful response is **not** a failure.

**If unavailable:** continue with AST tools and the scanner; rely more on local search and direct code reading within this skill's tool universe; reduce confidence on semantic claims; label proven vs. inferred.

**If degraded but completed:** treat the response as valid; on empty/wrong results, retry with a simpler input (drop regex meta-characters, switch to literal search, narrow the path). **Do not** switch to native Claude Code tools — that leaves the skill's evidence model.

