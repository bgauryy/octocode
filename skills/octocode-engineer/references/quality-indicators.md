# Quality Indicators Reference

> Complete catalog of code quality signals detected by the octocode-engineer scanner.

## Overview

The scanner detects quality indicators across 5 pillars: **Architecture**, **Code Quality**, **Dead Code**, **Security**, and **Test Quality**. Each indicator produces a finding with severity (critical/high/medium/low/info), actionable fix guidance, and LSP validation hints.

---

## Metrics Engine

### Cyclomatic Complexity
- **What**: Number of independent paths through a function's source code.
- **How**: Increments for `if`, `while`, `for`, `switch/case`, `catch`, `||`, `&&`, `??`, ternary.
- **Threshold**: `criticalComplexityThreshold` (default: 30).
- **Engines**: TypeScript Compiler, Tree-sitter.

### Cognitive Complexity
- **What**: Mental load to understand a function — penalizes nesting more than branching.
- **How**: Increments for control-flow keywords; adds nesting depth bonus for each level.
- **Threshold**: `cognitiveComplexityThreshold` (default: 15).
- **Engines**: TypeScript Compiler, Tree-sitter (now at parity).

### Halstead Metrics
- **What**: Software science metrics based on operator/operand vocabulary.
- **Computed**: volume, difficulty, effort, time, estimated bugs.
- **Threshold**: `halsteadEffortThreshold` (default: 500,000).
- **Engine**: TypeScript Compiler only.

### Maintainability Index (MI)
- **What**: Composite score (0–171) indicating ease of maintenance.
- **Formula**: `171 - 5.2·ln(V) - 0.23·CC - 16.2·ln(LOC)` (clamped to [0, 100]).
- **Threshold**: `maintainabilityIndexThreshold` (default: 20).
- **Engine**: TypeScript Compiler only.

---

## Code Quality Detectors (34 total)

### Existing Detectors

| # | Category | Severity | Description |
|---|----------|----------|-------------|
| 1 | `duplicate-function-body` | low–high | Identical function body hash across locations |
| 2 | `duplicate-flow-structure` | low–medium | Repeated control-flow structure shapes |
| 3 | `function-optimization` | medium–high | High cyclomatic complexity or deep nesting |
| 4 | `cognitive-complexity` | medium–high | High cognitive complexity per function |
| 5 | `god-function` | high | Oversized functions by statement count + low MI |
| 6 | `excessive-parameters` | medium | Functions with too many parameters |
| 7 | `empty-catch` | medium | Empty catch blocks silently swallowing errors |
| 8 | `switch-no-default` | low | Switch statements without default case |
| 9 | `unsafe-any` | medium | Files with excessive `any` type usage |
| 10 | `halstead-effort` | medium–high | Functions with extreme Halstead effort |
| 11 | `low-maintainability` | medium–high | Functions below MI threshold |
| 12 | `type-assertion-escape` | medium | `as any`, double assertions, non-null assertions |
| 13 | `missing-error-boundary` | medium | Async functions with awaits but no try-catch |
| 14 | `promise-misuse` | low | Async functions without any await |
| 15 | `await-in-loop` | medium | Await calls inside loops (sequential where parallel possible) |
| 16 | `sync-io` | medium | Synchronous I/O calls (readFileSync, etc.) |
| 17 | `uncleared-timer` | medium | setInterval without corresponding clearInterval |
| 18 | `listener-leak-risk` | medium | Event listeners added without removal |
| 19 | `unbounded-collection` | medium | Potential unbounded collection growth |
| 20 | `similar-function-body` | low–medium | Near-duplicate functions by metric similarity |
| 21 | `message-chain` | medium–high | Deep property chains violating Law of Demeter |

### Semantic-Gated Detectors (require `--semantic`)

| # | Category | Severity | Description |
|---|----------|----------|-------------|
| 22 | `god-module` | medium–high | Oversized modules by statement count + export count |
| 23 | `unused-parameter` | low | Function parameters never referenced in body |
| 24 | `deep-override-chain` | medium | Method override chains exceeding depth threshold |
| 25 | `interface-compliance` | medium | Types claiming to implement an interface but missing members |
| 26 | `narrowable-type` | low | Union types that could be narrowed for better type safety |

### New Detectors (v2)

| # | Category | Severity | Description |
|---|----------|----------|-------------|
| 27 | `deep-nesting` | low–high | Functions with branch/loop nesting exceeding threshold |
| 28 | `multiple-return-paths` | low–high | Functions with too many return/throw exit points |
| 29 | `catch-rethrow` | low | Catch blocks containing a throw statement (simplification candidates) |
| 30 | `magic-string` | low–high | String literals repeated in comparisons across files |
| 31 | `boolean-parameter-cluster` | medium | Functions with 3+ boolean parameters |
| 32 | `promise-all-unhandled` | medium | Promise.all/race/any without error handling |
| 33 | `export-surface-density` | low–high | Modules where >50% of statements are exported |
| 34 | `change-risk` | medium–critical | Composite risk score from overlapping quality signals |

---

## New Detector Details

| Category | Threshold flag (default) | Signal | Severity | Fix |
|----------|--------------------------|--------|----------|-----|
| `deep-nesting` | `--deep-nesting-threshold N` (5) | `max(maxBranchDepth, maxLoopDepth)` per fn | `≥T+3`→high, `≥T+1`→med, else low | Guard clauses, early returns, extract helpers |
| `multiple-return-paths` | `--multiple-return-threshold N` (6) | return/throw count per fn | `≥T+4`→high, `≥T+2`→med, else low | Single result var; guard clauses for error paths |
| `catch-rethrow` | — | catch containing a throw (simplification candidates) | always low | Remove try-catch if only re-throwing, or add logging before re-throw |
| `magic-string` | `--magic-string-min-occurrences N` (3) | string literals in `===`/`!==`/`case` appearing ≥N times | `≥8`→high, `≥5`→med, else low | Extract to named constant or enum |
| `boolean-parameter-cluster` | `--boolean-param-threshold N` (3) | fns with ≥N `boolean` params | always medium | Options object or split into separate fns |
| `promise-all-unhandled` | — | `Promise.all/allSettled/race/any` without try-catch or `.catch()` | always medium | Wrap in try-catch or chain `.catch()` |
| `export-surface-density` | — | `exportCount / totalStatements ≥ 0.5` (files with ≥20 stmts) | `≥80%`→high, `≥60%`→med, else low | Make non-essential symbols private; split facade + impl |
| `change-risk` | — | weighted sum: complexity+2, cognitive+2, low-MI+count, empty-catch+1, unhandled-promise+1, exports>15+1; fires at ≥4 | `≥8`→critical, `≥6`→high, else med | Fix overlapping quality signals in the file |

---

## AST Search Presets (35 total: 22 JS/TS + 13 Python)

### JavaScript / TypeScript Presets (22)
`empty-catch`, `console-log`, `console-any`, `debugger`, `todo-fixme`, `any-type`, `type-assertion`, `non-null-assertion`, `fat-arrow-body`, `nested-ternary`, `throw-string`, `switch-no-default`, `class-declaration`, `async-function`, `export-default`, `import-star`, `catch-rethrow`, `promise-all`, `boolean-param`, `magic-number`, `deep-callback`, `unused-var`

### Python Presets (13)
All prefixed with `py-` to avoid collision with JS/TS presets.

| Preset | Description |
|--------|-------------|
| `py-bare-except` | `except:` clause with no exception type |
| `py-pass-except` | `except: pass` — silently swallowed exception |
| `py-broad-except` | Overly broad `except Exception` / `except BaseException` |
| `py-global-stmt` | `global` variable mutation |
| `py-exec-call` | `exec()` — dynamic code execution |
| `py-eval-call` | `eval()` — dynamic evaluation |
| `py-star-import` | `from X import *` — wildcard import |
| `py-assert` | `assert` statements (stripped with `-O` flag) |
| `py-mutable-default` | Mutable default arguments (list/dict/set literal) |
| `py-todo-fixme` | TODO, FIXME, HACK, XXX, BUG comments |
| `py-print-call` | `print()` calls in production code |
| `py-class` | All class definitions |
| `py-async-function` | Async function definitions |

---

## Python Scanner Coverage

The scanner supports Python files (`.py`) for tree-sitter-based analysis: function metrics (complexity, nesting, cognitive complexity), flow detection, duplicate detection, and AST tree snapshots. The following **do not apply to Python** and are automatically skipped:

| Category | Reason |
|----------|--------|
| `unsafe-any` | TypeScript-specific (`any` type) |
| `type-assertion-escape` | TypeScript `as` expressions |
| `non-null-assertion` (detector) | TypeScript `!` operator |
| `halstead-effort` | TypeScript Compiler API only |
| `low-maintainability` (MI) | TypeScript Compiler API only |
| All semantic categories (`--semantic`) | TypeScript LanguageService only |
| Dependency graph / import resolution | JS/TS module resolution only |
| `dead-export`, `dead-re-export`, `unused-import` | JS/TS import/export system |

**What works for Python**: cyclomatic complexity, cognitive complexity, god-function (by statement count), deep-nesting, multiple-return-paths, duplicate-function-body, duplicate-flow-structure, similar-function-body, all AST presets (`py-*`), AST tree snapshots.

---

## Thresholds Reference

All thresholds are configurable via CLI flags (e.g. `--critical-complexity-threshold 30`, `--deep-nesting-threshold 5`) or config file. See [CLI reference](./cli-reference.md) for exact flag names.

| Threshold | Default | Used By |
|-----------|---------|---------|
| `criticalComplexityThreshold` | 30 | function-optimization |
| `cognitiveComplexityThreshold` | 15 | cognitive-complexity |
| `halsteadEffortThreshold` | 500,000 | halstead-effort |
| `maintainabilityIndexThreshold` | 20 | low-maintainability |
| `parameterThreshold` | 5 | excessive-parameters |
| `anyThreshold` | 5 | unsafe-any |
| `flowDupThreshold` | 3 | duplicate-flow-structure |
| `similarityThreshold` | 0.85 | similar-function-body |
| `godFunctionStatements` | 100 | god-function |
| `godFunctionMiThreshold` | 10 | god-function |
| `deepNestingThreshold` | 5 | deep-nesting |
| `multipleReturnThreshold` | 6 | multiple-return-paths |
| `magicStringMinOccurrences` | 3 | magic-string |
| `booleanParamThreshold` | 3 | boolean-parameter-cluster |

---

## Algorithms

### Tarjan's SCC (Strongly Connected Components)
Used to detect dependency cycle clusters. O(V+E) complexity. Identifies groups of modules that form circular dependencies.

### Articulation Points & Bridge Edges
Graph theory algorithms to find critical nodes (removing one disconnects the graph) and critical edges. Identifies architectural chokepoints.

### Structural Hashing
Functions are hashed by their AST structure (ignoring identifiers/literals) to detect true duplicates vs. near-duplicates.

### Metric Similarity
Near-duplicate detection uses feature-vector similarity: `1 - |a-b|/max(a,b)` across 8 metrics (complexity, depth, returns, awaits, calls, loops, statements).

### Composite Risk Scoring
The `change-risk` detector aggregates multiple quality signals into a single score per file, identifying files most likely to cause regressions.
