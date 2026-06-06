# Tool Workflows — Research Methodology & Patterns

Step-focused workflows for code analysis. For the Clean Architecture principles, the six analytic dimensions, and Tool Families, see [SKILL.md](../SKILL.md). For tool params and flags, see [CLI reference](./cli-reference.md).

---

## Quick Decision Table

| Question | Approach |
|----------|----------|
| "What does this codebase look like?" | Explore directory structure → find large files → spot hotspots |
| "Does pattern X exist?" | AST structural search with preset or pattern |
| "Where is X defined?" | Text search to locate → jump to definition |
| "Who calls function X?" | Text search for lineHint → call hierarchy (incoming) |
| "All usages of type/var X?" | Text search for lineHint → find references |
| "Is export X dead?" | Find references (exclude declaration) → cross-check with AST import search |
| "What's the AST shape of file X?" | AST tree-search on scan artifact |
| "Read this function" | Read file content with match targeting |
| "Trace flow A → B" | Search → definition jump → call hierarchy → chain hops |
| "Architecture hotspots?" | Scan with graph analysis → validate hotFiles with reference counts |
| "Structural smells?" | AST preset sweep (batch multiple presets in parallel) |
| "Did my fix work?" | Re-scan scoped to changed files + AST preset check + project toolchain |

---

## Workflows

### 1 — Full Scan → Triage → Validate

Start here for any new codebase or broad audit.

1. **Run full scan** with graph + flow flags → generates hypotheses
2. **Read summary.md** → health scores, pillar grades, top recommendations
3. **Triage top findings** from findings.json → check severity, lspHints, correlated signals
4. **Quick structural triage** — AST tree-search for function declarations to spot large/complex shapes
5. **Explore project layout** — directory structure + largest files for hotspot candidates
6. **Validate top findings** — use lspHints from findings to confirm or dismiss via reference counts and call hierarchy

See [output files](./output-files.md) for scan artifact schemas and read order. See [validation playbooks](./validation-playbooks.md) for per-category validation tactics.

### 2 — Symbol Deep Dive

Trace a function: definition → callers → callees.

1. **Check AST shape** — tree-search for the function to see its span, nesting depth, and structure
2. **Locate the symbol** — text search to get file + lineHint
3. **Read the function body** — targeted file content around the function
4. **Jump to definition** — follow the symbol to its canonical definition
5. **Find callers** — incoming call hierarchy → who depends on this?
6. **Find callees** — outgoing call hierarchy → what does this depend on?

### 3 — Impact Analysis (Quick Blast Radius)

Fast pre-change check. For a full refactoring plan with coupling and cycle analysis, use [Workflow 12](#12--refactoring-plan-safe-restructure).

1. **Locate the symbol** — text search to get file + lineHint
2. **Count all consumers** — find references, excluding declaration; split test vs production
3. **Assess safety** — few production refs + high test coverage = safe to change. Many refs = plan carefully → Workflow 12.

### 4 — Dead Export Validation

Fastest path from finding to verdict.

1. **Check for consumers** — find references excluding declaration. 0 refs = likely dead, >0 = alive.
2. **Cross-check structurally** — AST search for import statements of the export name
3. **Check for dynamic usage** — text search for the export name to catch computed/dynamic references
4. **Verdict** — 0 consumers across all checks = confirmed dead. Any usage found = dismiss.

### 5 — Code Smell Sweep (AST Presets)

Structural code smell detection — zero false positives.

1. **Run AST presets in parallel** — batch presets: empty-catch, any-type, console-log, switch-no-default, nested-ternary, non-null-assertion
2. **Add custom pattern checks** if needed (e.g. eval usage, specific anti-patterns)
3. **Read context** around flagged locations to understand severity
4. **Assess impact** — check callers of flagged functions to gauge blast radius

See [AST reference](./ast-reference.md) for all presets and pattern syntax.

### 6 — Dependency Cycle Tracing

Validate cycles from `architecture.json`.

1. **Scan for cycles** — run scan with dependency-cycle feature + graph
2. **Read cycle paths** from architecture.json
3. **Find back-edge imports** — AST search for import patterns in the cycle directory
4. **Identify importing files** — text search for imports from the cycle modules
5. **Read the import blocks** — targeted content reading at import statements
6. **Trace through the chain** — jump to definitions and follow call hierarchy until the cycle closes

### 7 — Security Sink Validation

Trace data flow from source to sink.

1. **Find sink patterns** — AST search for eval, innerHTML assignment, exec, command injection patterns
2. **Find secret patterns** — AST rule search for strings matching password/secret/token patterns
3. **Find guards** — text search for validation, sanitization, or normalization functions
4. **Read sink context** — targeted content reading around the sink function
5. **Locate the sink** — jump to definition for cross-file resolution
6. **Trace data sources** — incoming call hierarchy to trace who feeds data to the sink
7. **Check all call sites** — find references for the sink function to assess exposure breadth

See [validation playbooks](./validation-playbooks.md) for taint-tracing and false-positive dismissal.

### 8 — Scoped Deep-Dive (File or Function)

Drill into a specific flagged file or function.

1. **Re-scan scoped** — scan with scope narrowed to the target file, with flow + semantic flags
2. **Function-level scope** — if drilling into a specific function, use `file:symbol` scope syntax
3. **Check AST shape** — tree-search for the file to see function spans and nesting
4. **Read public surface** — targeted content at export declarations
5. **Read imports** — content at the top of the file for dependency context
6. **Read target section** — content around the specific area of interest
7. **Count consumers per export** — find references for each exported symbol
8. **Map function dependencies** — outgoing call hierarchy per function

### 9 — Coupling Hotspot Analysis

Quantify coupling for architecture findings.

1. **Scan for coupling signals** — run scan with coupling + god-module features + advanced graph
2. **Read top hotFiles** from architecture.json
3. **Map import density** — AST search for import patterns in the hotspot directory
4. **Count consumer files** — text search for imports from the hotspot
5. **Assess module size** — structure view of the hotspot directory + find largest files
6. **Quantify per-export coupling** — find references for each export, call hierarchy for each function

**Decision**: high fan-in + large files = decomposition candidate. Low fan-in = less urgent.

### 10 — Fix Verification Loop

Confirm fixes reduced finding count. Run after every fix batch.

1. **Re-scan changed files** — scoped scan with relevant feature flags
2. **AST smell check** — run presets against changed directories to verify smells are gone
3. **Spot-check fixes** — read fixed code to confirm the change looks right
4. **Verify references** — find references to confirm moved/renamed symbols still resolve
5. **Verify callers** — incoming call hierarchy to confirm callers are still connected
6. **Run project toolchain** — lint (with auto-fix), tests, build → all must pass

---

## Extended Workflows — Architecture, Planning, Exploration

### 11 — Pre-Implementation Check ("Where should new code live?")

Before writing new code, understand the existing landscape to pick the right location.

1. **Explore project layout** — directory structure at depth 2
2. **Map dependency graph** — scan with graph + advanced flags → identify hotspots
3. **Avoid hotspots** — read top hotFiles from architecture.json, don't add to them
4. **Find analogous patterns** — text search for similar features to see how the codebase does it
5. **Check existing API shape** — AST search for export patterns in candidate directories
6. **Check candidate module coupling** — find references for candidate module's exports
7. **Read the public surface** — targeted content at exports of the target module

**Decision**: low fan-in module with related exports = good home. High fan-in hotspot = add to a new module instead.

### 12 — Refactoring Plan (Safe Restructure)

Full blast-radius refactoring plan. Use when Workflow 3 (quick check) reveals high consumer count or cross-package scope.

1. **Map all files containing the symbol** — text search with file-list mode
2. **Count all consumers** — find references excluding declaration
3. **Split test vs production consumers** — find references filtered to test dirs; 0 test refs = coverage risk
4. **Map structural imports** — AST search for import patterns involving the symbol
5. **Map callers and dependencies** — incoming and outgoing call hierarchy
6. **Check coupling/cycle risk** — scoped scan with architecture + graph features
7. **Assess test quality around target** — scoped scan with test-quality feature

**Output**: file list + consumer count (test/prod split) + import graph + coupling risk = refactoring confidence level.

### 13 — Codebase Exploration (New Repo Orientation)

Quickly understand an unfamiliar codebase.

1. **Layout** — top-level directory structure → source root shape with file sizes
2. **Scale and hotspots** — find largest files, recently modified files, barrel/index files
3. **API surface** — text search for exports (file-list mode), AST search for class declarations and default exports
4. **Architecture shape** — full scan with graph + flow → read summary.md for health scores
5. **Conventions** — AST search for import patterns, text search for test patterns, find test file locations

### 14 — Test Strategy Analysis

Map test coverage gaps and test quality issues.

1. **Test landscape** — find all test files, explore test directory structure, count test density
2. **Coverage gaps** — text search for exported functions, then find references filtered to test dirs. 0 test refs = coverage gap.
3. **Test quality** — scan with test-quality feature + include-tests. AST search for empty catches, mock density, assertion density in test source
4. **Critical untested code** — scan for architecture to find critical paths, check test coverage per hotFile

**Output**: untested exports list + test quality findings + critical untested hotspots = test priority plan.

### 15 — Code Review Support (Change Impact Analysis)

Assess the architectural impact of changed files.

1. **Read the changes** — targeted content reading around changed functions
2. **Blast radius per symbol** — text search for changed symbols → find references → split test vs production → incoming call hierarchy for direct callers
3. **Architecture effect** — scoped scan of changed files with architecture + graph → AST search for new import patterns
4. **Quality check** — scoped scan of changed files with code-quality + security features → AST preset sweep on changed dirs for new smells
5. **Test coverage** — find references for changed symbols filtered to test directories → are all changes tested?

**Output**: consumer impact count + architecture delta + new quality issues + test coverage = review verdict.

### 16 — Code Quality Review (Module or File)

Focused quality review of a specific target.

1. **Scoped scan** — scan target with code-quality + dead-code features, flow + semantic flags → read summary + top findings
2. **AST smell sweep** — batch presets: empty-catch, any-type, type-assertion, non-null-assertion, console-log, nested-ternary, switch-no-default
3. **Complexity check** — AST tree-search for function spans + nesting, scoped scan for cognitive-complexity + god-module + god-function
4. **Dead code check** — find references per export (0 refs = dead), AST cross-check on import patterns
5. **Maintainability assessment** — read public surface size, map outgoing call hierarchy (dependency count), map incoming call hierarchy (fan-in per export)

**Output**: smell count + complexity scores + dead exports + fan-in/fan-out + maintainability = quality verdict with evidence.

### 17 — Full Architecture Analysis

Complete architecture health assessment.

1. **Full architecture scan** — graph + graph-advanced + flow + architecture features
2. **Read architecture outputs** — summary.md health score, cycles, hotFiles (ranked), SCC clusters, chokepoints, critical paths, Mermaid graph
3. **Validate top hotspots** — text search for hotspot files → find references for fan-in, call hierarchy for fan-out and direct callers
4. **Module boundary analysis** — AST search for cross-module imports, text search for barrel re-exports, scan for boundary/layer violations
5. **Cycle deep-dive** (per cycle) — AST search for imports in cycle dirs, jump to definitions to hop through, read import blocks to confirm back-edges
6. **Critical path analysis** — read criticalPaths from architecture.json, check incoming call hierarchy for each hub

**Output**: cycle list + SCC clusters + chokepoints + hotfiles (ranked) + boundary violations + critical paths + fan-in/fan-out = full architecture health report.

### 18 — Smart Coding (Impact-Aware Changes)

Before and after making code changes, check blast radius and verify safety.

**=== BEFORE CODING ===**

1. **Define behavior contract** — current behavior, desired behavior, invariants, non-goals, user-facing contract
2. **Understand the target area** — explore module layout, read current code, jump to definitions
3. **Check blast radius** — text search for target symbol → find references (total, production-only, test-only) → incoming call hierarchy for direct callers
4. **Check architecture safety** — scoped scan with architecture + graph → read cycles to check if the change would create new ones
5. **Follow existing patterns** — AST search for similar patterns nearby, text search for analogous implementations

**=== MAKE THE CHANGE ===**

6. **Implement** — apply edits

**=== AFTER CODING ===**

7. **Verify behavior** — run project tests
8. **Verify no new issues** — scoped scan of changed files with code-quality + architecture features, AST preset sweep for any-type + empty-catch
9. **Verify references intact** — find references for moved/renamed symbols, incoming call hierarchy for callers
10. **Verify user-facing contracts** — run CLI/API/integration checks when relevant, update docs when behavior changed
11. **Run project toolchain** — lint (with auto-fix), build

**Decision gates**:
- Step 3: >20 production consumers = high-risk, consider feature flag or incremental migration
- Step 4: change touches cycle member or hotfile = extra caution, verify with re-scan after
- Step 8: new findings = fix before committing
- Step 10: docs or contract drift = fix before committing
- Step 11: any failure = investigate before proceeding

### 19 — CLI Change Safety

Use when changing commands, flags, help text, output, or exit behavior.

1. **Find CLI entry points** — text search for CLI frameworks (process.argv, commander, yargs, etc.) + find files named cli/bin/command
2. **Read commands and options** — targeted content reading around command definitions, options, defaults
3. **Find affected tests and docs** — text search for flag/command names across tests, scripts, and docs
4. **Verify behavior** — run entry with --help, happy-path input, bad input; check stdout/stderr/exit codes
5. **Run CLI and e2e tests** if the project has them

**Checklist**: names, aliases, defaults, positional args, stdout/stderr, exit codes, env/config inputs, machine-readable output, backward compatibility.

### 20 — API Contract Safety

Use when changing handlers, endpoints, schemas, DTOs, or serialized responses.

1. **Find the public surface** — text search for router/handler/endpoint/resolver patterns + schema/contract files
2. **Read request/response code** — targeted content around route definitions
3. **Trace affected internals** — text search for response/DTO types → find references for shared types → outgoing call hierarchy for handler→service flow
4. **Verify the contract** — run integration, contract, and/or project test scripts

**Checklist**: request schema, response shape, status codes, error bodies, auth, pagination, idempotency, versioning, deprecation, migration notes.

### 21 — Docs and Rollout Sync

Use when public behavior changed or a risky change needs an operational plan.

1. **Find docs and examples** — find README, markdown, OpenAPI, env.example files + text search for changed names in docs
2. **Update completion criteria** — docs/help/examples/migration notes updated; feature flag/rollout/telemetry/rollback decided when needed
3. **Verify docs tooling** — run docs build/check scripts if the project has them

**Output**: updated docs list + compatibility note + rollout/rollback plan, or explicit statement that no public docs or rollout work was needed.

