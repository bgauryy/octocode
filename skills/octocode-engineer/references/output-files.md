# Output Files

Each scan writes to `.octocode/scan/<timestamp>/`:

| File | Contents | When to Read |
|------|----------|-------------|
| `summary.md` | Health scores, **per-feature scores**, **AI + Structure hybrid ratings**, tags, severity, per-pillar counts, top recs, change risk hotspots | **Always first** |
| `summary.json` | Machine-readable scan metadata, `agentOutput`, `analysisSummary`, `featureScores[]`, `qualityRating`, `investigationPrompts`, `parseErrors[]` | Programmatic access |
| `architecture.json` | Dep graph, arch findings, `hotFiles[]`, `graphSignals[]`, chokepoints, optional advanced graph overlays | Cycles, coupling, SDP, D metric, test gaps, side-effect risk |
| `code-quality.json` | Quality findings, severity/category breakdowns | Duplicates, complexity, perf |
| `dead-code.json` | Dead-code findings, severity/category breakdowns | Dead code cleanup |
| `security.json` | Security findings, severity/category breakdowns (emitted only when findings exist) | Secrets, sinks, unsafe eval |
| `test-quality.json` | Test quality findings, severity/category breakdowns (emitted only when findings exist, requires `--include-tests`) | Mock density, brittle tests |
| `file-inventory.json` | Per-file: functions, flows, metrics, `issueIds[]` | Deep-diving a specific file |
| `findings.json` | ALL findings sorted by severity with `ruleId`, `analysisLens`, `confidence`, `impact`, `correlatedSignals[]`, `recommendedValidation`, and optional `flowTrace[]` | Complete sorted list |
| `ast-trees.txt` | `Kind[startLine:endLine]` per file (on by default, disable with `--no-tree`) | Structural overview |
| `graph.md` | Mermaid dependency graph (only with `--graph`) | Visual architecture |

---

## JSON Key Reference

### `summary.json`

```
schemaVersion, generatedAt, repoRoot, options, parser,
summary { totalPackages, totalFiles, totalNodes, totalFunctions, totalFlows, totalDependencyFiles, byPackage },
agentOutput { totalFindings, highPriority, mediumPriority, lowPriority,
              topRecommendations[] { id, file, severity, category, title, reason, suggestedFix },
              filesWithIssues[] { file, issueCount, issueIds } },
analysisSummary { graphSignals[], astSignals[], strongestGraphSignal, strongestAstSignal, combinedSignals[], recommendedValidation },
strongestGraphSignal, strongestAstSignal, combinedSignals[], recommendedValidation, investigationPrompts[],
featureScores[] { category, pillar, findings, affectedFiles, hotspotHits, hotspotMaxRisk, contextPenalty, severityBreakdown { critical, high, medium, low, info }, score, grade },
qualityRating { model, overallScore, overallGrade, aspects[] { aspect, label, weight, score, grade, confidence, rationale, signals[] } },
parseErrors[] { file, message },
outputFiles { summary, architecture, codeQuality, deadCode, fileInventory, findings, ... }
```

Use `summary.json` to drive the first decision:

- Use `agentOutput.topRecommendations[]` and `filesWithIssues[]` to decide where to drill in first
- Use `featureScores[]` to rank worst categories across the currently active feature set
- Use `qualityRating.aspects[]` for non-rigid repo-shape scoring (architecture/folder structure health/naming/common/shared/maintainability/consistency)
- Use `summary.md` or `architecture.json` for graph-specific detail such as `cycles`, `criticalPaths`, and hotspots
- If top recommendations are mostly complexity, duplication, or side-effect findings, switch to AST-first investigation
- If graph-heavy recommendations and AST-heavy recommendations appear together, plan a combined investigation before proposing refactors

### Scoring model (current)

- Severity weights: `critical=25`, `high=10`, `medium=3`, `low=1`, `info=0`
- Score formula: `round(100 / (1 + (weightedFindingsPerFile / 10)))`
- Guardrails: non-info findings cannot score perfect `100`; critical/high findings are capped (`95`/`98`)
- Feature context penalties: hotspot overlap lowers category scores (`hotspotHits`, `hotspotMaxRisk`, `contextPenalty`)
- Hybrid quality model (`qualityRating`): weighted soft-signal scoring, not rigid checks:
  - Architecture & Structure (30%)
  - Folder Topology / Structure Health (15%): depth balance, source spread, vague buckets, and leaf-folder bloat
  - Naming Quality (15%)
  - Common/Shared Layer Health (15%)
  - Maintainability & Evolvability (15%)
  - Codebase Consistency (10%): file and folder naming style consistency, plus mixed TS/JS surface area

### `findings.json`

```
generatedAt,
optimizationFindings[] { id, ruleId, severity, category, analysisLens, confidence,
                         file, lineStart, lineEnd, title, reason,
                         files[], suggestedFix { strategy, steps[] }, impact, tags[],
                         correlatedSignals[], recommendedValidation, flowTrace[], lspHints[] },
totalFindings
```

Filter: `jq '.optimizationFindings[] | select(.tags | contains(["coupling"]))' findings.json`

Use `findings.json` to correlate categories:

- `feature-envy` + `low-cohesion` = likely boundary error
- `layer-violation` + `feature-envy` = likely dependency leak
- `import-side-effect-risk` + hotspot tags = likely startup risk
- `dependency-critical-path` + complexity tags = likely change chokepoint

### `architecture.json`

```
schemaVersion, generatedAt,
dependencyGraph { totalModules, totalEdges, criticalModules[], cycles[], criticalPaths[], ... },
dependencyFindings[], findings[], findingsCount,
severityBreakdown { critical, high, medium, low },
categoryBreakdown { "dependency-cycle": N, ... },
hotFiles[] { file, riskScore, fanIn, fanOut, complexityScore, exportCount, inCycle, onCriticalPath },
graphSignals[], chokepoints[], criticalHubCandidates[],
sccClusters[] (with `--graph-advanced`), packageGraphSummary (with `--graph-advanced`), packageHotspots[] (with `--graph-advanced`)
```

Use `architecture.json` as the graph lens:

- `criticalModules[]` = hub nodes already surfaced by the dependency summary
- `cycles[]` = immediate structural loops
- `criticalPaths[]` = long change propagation chains
- `hotFiles[]` = current approximation of graph chokepoints
- `graphSignals[]` = already-interpreted graph narratives for triage
- `chokepoints[]` = broker and articulation-style structural pressure points
- `categoryBreakdown` = whether the repo’s architecture risk is mostly cycles, layering, cohesion, or side effects

Good investigation prompts:

- "Do critical hub modules also appear in hotFiles or critical paths?"
- "Which files are both hot and on a critical path?"
- "Which layer violations cluster around the same folder?"
- "Do side-effectful modules also have high fan-in?"

### `code-quality.json`

```
generatedAt, duplicateFlows { duplicateFunctions[], redundantFlows[] },
optimizationOpportunities[] { type, message, file, lineStart, lineEnd, details },
findings[], findingsCount, severityBreakdown, categoryBreakdown
```

### `dead-code.json`

```
generatedAt, findings[], findingsCount, severityBreakdown, categoryBreakdown
```

### `security.json`

Emitted only when security findings exist. Same schema as `dead-code.json`:

```
generatedAt, findings[], findingsCount, severityBreakdown, categoryBreakdown
```

Categories: `hardcoded-secret`, `eval-usage`, `command-injection-risk`, `path-traversal-risk`, `sql-injection-risk`, `unsafe-html`, `unsafe-regex`, `prototype-pollution-risk`, `sensitive-data-logging`, `debug-log-leakage`, `input-passthrough-risk`, `unvalidated-input-sink`.

### `test-quality.json`

Emitted only when test quality findings exist (requires `--include-tests`). Same schema as `dead-code.json`:

```
generatedAt, findings[], findingsCount, severityBreakdown, categoryBreakdown
```

Categories: `test-no-assertion`, `low-assertion-density`, `excessive-mocking`, `shared-mutable-state`, `missing-test-cleanup`, `focused-test`, `fake-timer-no-restore`, `missing-mock-restoration`.

### `file-inventory.json`

```
generatedAt, fileCount,
fileInventory[] { package, file, parseEngine, nodeCount, kindCounts,
                  functions[] { name, lineStart, lineEnd, complexity, cognitiveComplexity, ... },
                  flows[], dependencyProfile { internalDependencies[], externalDependencies[],
                  declaredExports[], importedSymbols[], reExports[] },
                  emptyCatches[], switchesWithoutDefault[], anyCount, magicNumbers[],
                  topLevelEffects[], effectProfile, symbolUsageSummary, boundaryRoleHints[], cfgFlags,
                  prototypePollutionSites[], issueIds[] }
```

Use `file-inventory.json` as the AST lens:

- `functions[]` = shape and complexity of orchestration
- `flows[]` = repeated control structures
- `dependencyProfile` = exported/imported symbol detail for cohesion and feature-envy follow-up
- `topLevelEffects[]` = hidden initialization / import-time work
- `effectProfile` = summarized import-time risk
- `symbolUsageSummary` = compact symbol/import/export shape for boundary follow-up
- `boundaryRoleHints[]` = lightweight role inference for the file
- `cfgFlags` = lightweight flow clues for validation, cleanup, exit behavior, and async boundaries (with `--flow`)

If `architecture.json` names a hotspot, use `file-inventory.json` to explain why that hotspot is structurally hard to change.

---

## Reading `ast-trees.txt`

Flattened AST snapshot: `## <package> — <filepath>` section headers, then indented `Kind[startLine:endLine]` nodes (2 spaces = 1 depth level, `...` = truncated children). On by default (`--emit-tree`). Suppress with `--no-tree`. Tree depth: `--tree-depth N` (default: 4).

Query with `tree-search.js` (`-k`, `-p`, `--file`, `-C`). For format details and tool reference, see [ast-reference.md](./ast-reference.md).

---

## Legacy Single-File Mode (`--out path/to/file.json`)

Keys: `summary`, `fileInventory[]`, `duplicateFlows`, `dependencyGraph`, `dependencyFindings[]`, `optimizationFindings[]`, `agentOutput`, `parseErrors[]`.
