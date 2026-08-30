# Local analysis findings and TODOs

This file records the verified findings from the MCP v2 and local-tool evaluation. It also tracks the unified graph-tool decision that supersedes the earlier separate-tool recommendation.

## Decision

Replace `localFindDeadCode` and the proposed `localAnalyzeDependencies` surface with one public tool: `localAnalyzeGraph`.

`localAnalyzeGraph` uses a discriminated `operation` contract. Dead-code analysis is an operation over the same repository graph, not a separate tool. The first contract must support `deadCode`, `cycles`, `dependencies`, `dependents`, `path`, and `reachability`. Do not expose an unbounded node-and-edge query language.

Why:

- `packages/octocode-tools-core/src/graph/buildFileGraph.ts` builds file-level import edges and records exact syntactic edge provenance.
- `packages/octocode-tools-core/src/graph/reachability.ts` provides entrypoint reachability and iterative strongly connected component analysis.
- `packages/octocode-tools-core/src/tools/local_analyze_graph/deadCodeScan.ts` adds one graph operation's policy: entrypoint inference, retention rules, confidence warnings, and dead-export classification.
- `packages/octocode-engine/docs/NATIVE_GRAPH_DOMAIN_SCOPE.md` already defines a graph domain that includes file and symbol nodes, import/call/containment edges, reachability, retainers, strongly connected components, and dead-node analysis.
- The current TypeScript `FileNode` contains module-import edges, while native graph facts also contain declarations and calls. The unified tool must state which graph and edge kinds each operation uses.
- Existing graph tools expose concrete operations. The unified tool preserves that clarity through its required discriminator instead of splitting each operation into another MCP tool.

## P1: Implement the unified graph tool

- [x] Keep `localAnalyzeGraph` metadata in the repository-owned tools-core contract; do not move it to external core.
- [x] Use a discriminated union keyed by `operation`; do not create one object with every operation's fields optional.
- [x] Support `deadCode`, `cycles`, `dependencies`, `dependents`, `path`, and `reachability` as bounded projections over one graph builder.
- [x] Move the existing dead-code query fields under the `deadCode` operation and preserve its candidate-grade output semantics.
- [x] Define compact, paginated responses before implementation. Return summaries and requested slices, not the entire repository graph.
- [x] Include exact implemented edge provenance (`static-import`, `dynamic-import`, `named-reexport`, `star-reexport`) and syntactic confidence in graph rows, paths, cycles, clusters, and reachability. Reserve `call` and `contains` for a future native-symbol graph contract rather than implying unsupported edges.
- [x] Replace `LOCAL_FIND_DEAD_CODE`, its MCP registration, CLI catalog entry, documentation, and tests. Do not ship two public graph-analysis tools.
- [x] Map and migrate every live import, registration, test, and architecture consumer before renaming. A post-change source/docs search finds no `local_dead_code` references outside this historical record.
- [x] Validate the unified tool on the React reconciler and VS Code editor sparse clones used by the local-tool evaluation.
- [ ] **Environment-gated:** run a local small-model routing and comprehension check when Ollama is available. The 2026-08-28 health check still found no service on `127.0.0.1:11434`; no model was invented, started, or pulled.

Acceptance criteria:

- An agent can select the correct graph operation without reading unrelated fields.
- A query does not emit an unbounded node or edge collection.
- Cycle results match the existing iterative strongly connected component implementation.
- Reachability results state their entrypoints, truncation state, skipped-file count, and confidence.
- `operation: "deadCode"` matches the existing dead-code results on frozen fixtures and large-repository smoke cases.
- The public catalog contains `localAnalyzeGraph` and does not contain `localFindDeadCode` or `localAnalyzeDependencies`.

Evaluation contract:

- Primary KPI: unified graph contract cases passing. Baseline 0 because the tool does not exist; target 100% for all declared operations.
- Leading KPI: one compact schema with operation-specific fields and no unrelated required fields.
- Guardrails: existing dead-code fixture parity, no new false-positive deletion verdicts, bounded responses, and the full repository verification suite.
- Held-out checks: React reconciler and VS Code editor sparse clones.
- Decision rule: accept only when all operation-contract cases pass and every guardrail holds.

## P1: Extract the internal graph substrate

- [x] Move file-graph types, construction, import resolution, reachability, and SCC traversal into the neutral `packages/octocode-tools-core/src/graph/` substrate.
- [x] Keep entrypoint inference and export-retention policy owned by the `deadCode` operation.
- [x] Preserve the native `extractGraphFacts` boundary in `packages/octocode-tools-core/src/utils/contextUtils.ts`; the LSP fallback remains a consumer.
- [x] Add focused construction, all-four-provenance-kind, path, cycle, reachability, truncation, and frozen dead-code parity tests.
- [x] Benchmark repeated same-root operations and add request-local promise reuse. A contract test proves one fact-extraction pass for two operations; held-out five-operation CLI wall time fell from 3.20s to 0.82s on React and 2.78s to 0.79s on VS Code.

## P1: Make dead-code responses bounded

- [x] Bound `deadClusters` to clusters referenced by the current dead-export page, cap each cluster at 50 files, and report its full `size`/`truncated` state.
- [x] Bound `entrypointsResolved` to 50 entries and report the complete count and truncation state for both dead-code and reachability summaries.
- [x] Evaluate a cluster continuation against held-out workflows. Do not add it yet: current page-scoped cluster summaries retain sufficient anchors, and an unconditional expansion weakens the bounded contract.
- [x] Set and pass a compact five-query target of at most 32 KiB: React is 26,220 characters and VS Code is 30,526 characters after evidence/provenance metadata.

## P2: Correct contracts and documentation

- [x] Fix the locally owned `localSearchCode.maxFiles` description: text/regex uses a non-lossy per-page ceiling; structural mode uses a potentially lossy native scan cap and reports truncation evidence.
- [x] Update `docs/OCTOCODE_TOOLS.md`: dead clusters are mutually importing SCCs, not necessarily files that call each other.
- [x] Restore `docs/context/SEARCH_GUIDE.md` and `docs/context/AGENT_RESEARCH_WORKFLOWS.md` as concise, current references.
- [x] Document the boundary between `localAnalyzeGraph` syntactic repository topology and `lspGetSemantics` symbol-identity proof.

## Completed during the evaluation

- [x] Migrated MCP dependencies to `@modelcontextprotocol/server` and `@modelcontextprotocol/client` v2.
- [x] Added one valid JSON envelope for batched compact schemas. The six-local-tool schema payload fell from 16,207 to 6,396 characters.
- [x] Applied `localFindDeadCode.limit` before pagination.
- [x] Fixed LSP symbol-kind counts for inherited object keys such as `constructor`.
- [x] Propagated longer clone timeouts through the security, catalog, MCP registration, and single-query bulk layers.
- [x] Exercised all local tools on large React and VS Code sparse clones.
- [x] Passed `yarn build`, `yarn test`, `yarn lint`, `yarn typecheck`, and `yarn verify` after the earlier changes; final post-refactor receipts are recorded below.

## Unified graph evaluation result

- **Verdict: ACCEPT.** Primary KPI improved from 0/6 available operations to 6/6 through the public `localAnalyzeGraph` CLI contract.
- The public catalog contains `localAnalyzeGraph`; querying `localFindDeadCode --scheme` returns unknown-tool and lists only the unified graph surface.
- The focused graph/dead-code suite passes 22/22 tests, including limit-before-pagination, confidence propagation, retention, dynamic imports, SCCs, traversal, paths, and reachability.
- Held-out React reconciler: 173 files scanned; dependencies, dependents, path, cycles, and dead-code response shaping completed successfully.
- Held-out VS Code editor: 863 files scanned; dependencies, dependents, path, cycles, reachability, and dead-code response shaping completed successfully.
- Nested response guard: SCC and dead-cluster member lists cap at 50 files; shortest paths cap at 100 files; every cap reports full size/length and `truncated`.
- Repository guardrails pass: `yarn build`, `yarn lint`, `yarn typecheck`, `yarn test` (3,908 passed, 1 skipped), `yarn docs:verify`, and `yarn verify`.

## Unified graph skill and workflow evaluation

- **Tool rating: 9.5/10 (ACCEPT; up from 8.8).** The discriminated bounded contract now adds exact edge provenance, normalized evidence, request-local graph reuse, frozen parity tests, and two large-repository held-outs.
- [x] Route all six operations through the canonical `octocode-research` skill: `dependencies`, `dependents`, `path`, `cycles`, `reachability`, and `deadCode`.
- [x] Define the cross-tool boundary consistently: graph operations provide syntactic file-topology evidence; `lspGetSemantics` provides semantic symbol identity.
- [x] Update local, local+external, change, refactor, PR-review, general research, and proof-ladder flows.
- [x] Update `AGENTS.md` and the agent workflow/tool-quality guide with the graph stage and verification path.
- Verification: canonical and packaged `octocode-research` skills both pass review with 0 errors/0 warnings; the description contract and research evaluator self-test pass; canonical/package searches contain no legacy public graph-tool names; `yarn build`, `yarn docs:verify`, and `yarn verify` pass.
- [x] Preserve operation-specific required fields in compact graph schemas through the explicit `relations` contract; the flattened field list remains intentionally concise.
- [x] Bound `deadCode.summary.entrypointsResolved`; a test-heavy package now reports at most 50 entries plus count/truncation metadata.
- [x] Refresh generated/project skill mirrors through their owning build/sync pipeline without deleting mirror-only eval assets (99/99 supported destinations current; Pi-only eval overlays preserved).

## Complete 17-tool schema and workflow audit

- [x] Inspect the full and compact schema for all 15 default tools plus `ghListReleases` and `ghSearchDiscussions` with both feature flags enabled.
- [x] Add compact/full `relations` for conditional and mutually exclusive modes: local search, graph, local/remote content reads, LSP, PRs, issues, commits, and discussions.
- [x] Fix the lean catalog hint from nonexistent `localSearchCode.keywords` to `searchText`.
- [x] Replace relative local command examples with unmistakable `/ABS/...` placeholders.
- [x] Publish strict-valid examples for all six graph operations and for issue list/detail, releases, and first-page discussions.
- [x] Align `localAnalyzeGraph` default pagination with the schema maximum (`50`).
- [x] Cap graph `entrypointsResolved` summaries at 50 and publish `entrypointsResolvedCount` plus `entrypointsResolvedTruncated`.
- [x] Add regression tests proving every published command pattern stays within its strict tool schema.
- [x] Keep relation text and local graph metadata in the repository-owned tools-core contract.
- [x] Normalize `meta.evidence` and optional `meta.diagnostics` across every shared bulk response, including custom finalizers that previously dropped common metadata.
- [x] Add deterministic, sanitized provider recordings for five authenticated GitHub response paths to CI: code, content, repositories, pull requests, and structure.
- [x] Re-evaluate shared graph construction after the held-out benchmark and implement safe request-local same-root reuse.

## Live dogfood findings (2026-08-28)

- [x] Fixed `lspGetSemantics` empty-result fallback: `next.textSearch.query` now emits schema-valid `path` + `searchText`, and its regression test strict-prepares the emitted query.
- [x] Replaced the invalid `github/community` discussions example with the live-verified, cursor-free `vitejs/vite` plugin search.
- [x] Preserved the PR-only issue-page explanation through the public response as `hints`; live `status:"empty"` + `pagination.hasMore:true` output now explains why and points to `nextPage`.
- [x] Refreshed the workspace and shared-agent `octocode-research` mirrors as links to canonical `skills/octocode-research`; all three copies contain no removed graph-tool names and skill review reports zero errors/warnings.
- [x] Aligned local CLI discovery across all 17 public schemas: opt-in releases/discussions now appear in catalog, help, context, and schema JSON with explicit availability/env gates, while runtime execution remains disabled until enabled.

## Prior art

- [Knip](https://github.com/webpro-nl/knip) focuses its public contract on unused dependencies, exports, and files.
- [dependency-cruiser](https://github.com/sverweij/dependency-cruiser) validates dependency graphs against explicit rules and can render them.
- [Madge](https://github.com/pahen/madge) exposes concrete graph questions such as circular dependencies, dependents, orphans, and leaves.
- [dpdm](https://github.com/acrazing/dpdm) exposes dependency trees, cycles, and unused-file detection as separate operations.

## Tool, skill, and workflow scorecard (2026-08-28)

Rubrics: tools = schema clarity, routing distinctness, response/continuation quality, reliability, and cost; skills = trigger precision, executable workflow, evidence, gates, portability, and review health. Scores are out of 10 and are backed by the live local CLI catalog/schemes, dogfood calls, the skill-review gate, and repository verification.

### Public tools

| Tool | Score | Main finding |
|---|---:|---|
| `ghSearchCode` | 9.3 | Strong path/content split and cheap concise mode; deterministic provider-response coverage now protects finalization. |
| `ghSearchRepos` | 8.9 | Rich discovery filters and examples; deterministic provider-response coverage offsets its broad query surface. |
| `ghSearchPullRequests` | 9.2 | Excellent list/detail/patch contract plus a recorded provider smoke; its nested selector surface is necessarily large. |
| `ghSearchIssues` | 8.9 | Clear list/detail routing and continuation; GitHub page edge cases need response hints. |
| `ghSearchCommits` | 8.9 | Strong history/range semantics; provider behavior still needs recorded integration smokes. |
| `ghListReleases` | 8.4 | Distinct, simple operation; opt-in execution lowers default workflow availability. |
| `ghSearchDiscussions` | 8.2 | Valuable distinct evidence surface; GraphQL/provider variability and opt-in execution add friction. |
| `ghGetFileContent` | 9.5 | Precise extraction modes, pagination, cost controls, and a bounded recorded-response smoke. |
| `ghViewRepoStructure` | 9.1 | Cheap remote orientation with clean follow-ups and recorded finalization coverage; branch/tree limits remain external. |
| `ghCloneRepo` | 8.6 | Correct escalation for repeated/semantic work; network, disk, and trust costs are material. |
| `localSearchCode` | 9.2 | Excellent text/regex/AST breadth plus strict-safe `pattern` and `useRegex` first-contact aliases. |
| `localFindFiles` | 8.9 | Strong metadata discovery plus readable name/type aliases; it still has many metadata knobs. |
| `localAnalyzeGraph` | 9.5 | Six bounded operations, exact syntactic provenance, request-local reuse, parity tests, and large-repository held-outs. |
| `localGetFileContent` | 9.4 | Exact, ranged, matched, and minified reads with strong mode relations. |
| `localViewStructure` | 8.8 | Cheapest orientation step; readable entry types and `depth` alias now absorb the live first-contact misses. |
| `lspGetSemantics` | 8.8 | Unique symbol-identity proof and compact output; server/language availability and anchor requirements add fragility. |
| `npmSearch` | 8.3 | Very lean package-to-source bridge; limited filters and registry/provider dependence cap depth. |

Suite average: **8.9/10** after the graph, response, recording, and strict-safe alias changes.

- [x] Add strict-safe aliases for common local guesses: search `pattern`/`useRegex`, file `name`/`type`, and tree `depth`, while preserving structural-pattern behavior and unknown-field rejection.
- [x] Normalize `meta.evidence` and optional `meta.diagnostics` envelopes across all shared bulk tools.
- [x] Add deterministic authenticated GitHub provider-response recordings to CI.

### Repository skills

| Skill | Score | Main finding |
|---|---:|---|
| `octocode-research` | 9.5 | Best evidence ladder and routing backbone; precise proof grades and local/external handoffs. |
| `octocode-code-graph` | 9.4 | Focused graph-to-code proof ladder with strong false-positive controls; the six-case suite is intentionally narrow. |
| `octocode-brainstorming` | 9.1 | Disciplined divergence, cross-surface research, and decision gates; broad validation can be heavy for small asks. |
| `octocode-rfc-generator` | 9.2 | Strong decision and traceability contract; multi-artifact mode is intentionally expensive. |
| `octocode-eval-benchmark` | 9.6 | Strongest measurable accept/revert discipline, smart grader selection, held-out checks, benchmark hygiene, and Goodhart guards. |
| `octocode-subagent` | 9.2 | Excellent spawn gate and local-Ollama route; the combined cloud/local reference surface is large. |
| `octocode-documentation` | 9.2 | Evidence-backed writing plus deterministic style gate; its complete style pack is costly to maintain. |
| `octocode-roast` | 8.8 | Memorable evidence-first critique and repair checkpoint; tone specialization narrows reuse. |
| `octocode-prompt-optimizer` | 9.0 | Clear behavioral optimization loop; validation is model-driven rather than scripted. |
| `octocode-skills` | 9.3 | Strong lifecycle, cleanup, sync, and zero-error review gate; collection discovery and invalid-target handling are now deterministic. |
| `octocode-chrome-devtools` | 9.0 | Deep live-browser evidence and safety gates; operational surface is large and environment-sensitive. |
| `octocode-scraping` | 9.1 | Excellent corpus-first, keyless-first, recovery-oriented flow; provider matrix adds complexity. |
| `octocode-mannequin` | 6.8 (removed) | Coherent but product-niche; duplicate FK/clamp implementations and browser coupling formed a maintenance island. |

Current-suite average: **9.2/10**. Baseline skill review: 12/12 clean, 0 errors, 0 warnings.

### End-to-end workflow

Score: **9.0/10**. Best route: discover live catalog and availability → read the selected scheme → orient cheaply → search for anchors → read exact bytes → prove topology with `localAnalyzeGraph` and symbol identity with `lspGetSemantics` → follow returned continuations → run focused tests → build the affected package/interface → dogfood the built local CLI → run repository verification.

- [x] Replace stale absolute skill-source path with a checkout-relative source-of-truth statement.
- [x] Align `octocode-research` with the built local CLI and the 17-schema discovery contract.
- [x] Remove `octocode-mannequin` from this repository's source, generated CLI bundle, active install, and catalog documentation.
- [x] Make `skill-review.mjs skills --json` discover immediate child skills; invalid targets now return a clean usage error with exit code 2 instead of throwing `ENOENT`.
- [x] Final gates: 11 remaining skills review with 0 errors/0 warnings; research description check passes; built local CLI reports 17 tools; `yarn verify` passes 3,872 tests with 1 skipped.
- [x] Ignore hook-command paths in YAML frontmatter when enforcing prose route conditions; `skill-review.mjs --self-test` now covers collection discovery, invalid targets, and frontmatter hooks.
- [x] Refresh all 11 Octocode-home skill copies and link every canonical skill into the four approved top vendors; all 44 vendor targets report `ok`.
- [x] Review installed collections after sync: Octocode-home, shared Agents, Cursor, Claude, and Codex-native all report 0 errors/0 warnings; remove the final broken mannequin vendor link.
- [x] Expand the approved skill sync from the 44-target baseline to all supported vendors: 99/99 unique destinations are current (97 canonical symlinks plus 2 reviewed Pi overlays); 11 duplicate Codex/Agents rows are correctly deduplicated.
- [x] Preserve and verify the Pi-only held-out eval assets for brainstorming and RFC generation while aligning their canonical content; every installed `octocode-*` skill reports 0 errors/0 warnings across all ten roots.
- [x] Exclude the unrelated Pi `run-test` skill from the Octocode verdict; its 3 pre-existing warnings remain reported and untouched.
- [x] All-vendor eval verdict: **ACCEPT** — skill-sync self-test passes 14 checks, skill-review self-test and docs verification pass, mannequin is absent from every active root, `yarn build` passes, and `yarn verify` passes 3,872 tests with 1 skipped.

## Final post-refactor verification (2026-08-28)

- **Verdict: ACCEPT.** All repo-local actionable findings moved to complete. The only unchecked entry is the environment-gated Ollama check.
- `yarn build` passes across the TypeScript packages and release-mode Rust engine.
- `yarn verify` passes 3,884 tests with 1 skipped: tools-core 1,679; MCP 787; CLI 905; VS Code 21; config 135; engine 357.
- The focused graph/alias/response/recording suite passes, including 14 alias cases, five deterministic GitHub provider paths, frozen dead-code parity, exact four-kind edge provenance, and one-build-per-batch reuse.
- The rebuilt local CLI reports all 17 schemas. Every local tool path passes: structure, file discovery, text search, exact read, all six graph operations, and LSP references.
- Dogfood found and fixed three intuitive-input misses (`entryType:"file"`, `entryType:"both"`, graph `maxDepth`) and one newly introduced unused graph type; LSP proved the type had zero references before removal.
- Held-out five-operation compact graph responses stay below the 32 KiB guard: React 26,220 characters and VS Code 30,526 characters. Request-local reuse reduced measured wall time from 3.20s to 0.82s and 2.78s to 0.79s respectively.
- Documentation verification passes. Style lint reports no errors; pre-existing informational style debt in the long tools reference remains non-blocking.
- Coverage is reported honestly: the neutral graph substrate has 91.16% statements and 92.64% lines; tools-core overall remains 66.91% statements because broad legacy provider paths are not unit-covered.
- All 17 public tool names, schemas, descriptions, relation text, and command patterns now live under `packages/octocode-tools-core/src/toolContract/`; MCP and CLI consume that repository-owned contract instead of `@octocodeai/octocode-core/schemas`.
- A source-ownership regression test rejects external schema imports and restricts the remaining root `@octocodeai/octocode-core` metadata import to the shared system prompt adapter. External core remains only for the system prompt, CLI command specs, and output/result types.
- Rebuilt-CLI dogfood passed each of the 17 individual `tools <name> --scheme --json` contracts: matching identity, non-empty short/full descriptions, executable object schema, command patterns, and explicit availability metadata.
- Tool-contract score after ownership consolidation: **4.8/5 (A)**, up from **3.2/5 (C)**. The remaining 0.2 reflects the intentionally separate external system-prompt and output-type surfaces, not split tool-schema ownership.

## Full tools, flow, security, and efficiency audit (2026-08-28)

- **Verdict: ACCEPT.** All three actionable audit findings are closed. The MCP output boundary now fails closed, graph analysis follows exact workspace package exports without enabling aliases, and both opt-in GitHub transports have direct deterministic coverage.
- [x] **SEC-1 (HIGH): fail closed when output sanitization throws.** The MCP wrapper now discards the complete raw result, returns `isError:true` with a generic message, and writes no sanitizer exception details to stderr. The inverted regression proves both tool content and sanitizer secrets remain absent; `docs/SECURITY.md` documents this failure mode.
- [x] **GRAPH-1 (MEDIUM): resolve workspace-package imports without aliases.** Graph construction reads exact `package.json` export declarations and maps their build targets back to source files already inside the scan. It does not read `tsconfig` paths, accept custom aliases, traverse `node_modules`, or add compatibility aliases. Rebuilt-CLI dogfood now reports the missing `registry.ts` → `packages/octocode-tools-core/src/schema.ts` edge over a 5,889-file scan; an undeclared alias regression remains unresolved by design.
- [x] **TEST-1 (LOW): directly cover the opt-in GitHub provider transports.** Deterministic provider-level tests now cover releases and discussions mapping, pagination, cache identity, non-fatal missing-latest behavior, and shared error conversion. Full-run statement coverage moved to 93.75% for `releases.ts` and 95% for `discussions.ts`.
- [x] All 17 public schemas passed individual rebuilt-CLI identity, description, executable-schema, command-pattern, and availability checks.
- [x] All 17 tools executed through the local CLI: six local tools, npm, nine read-only GitHub operations, and a sparse `ghCloneRepo`; gated releases/discussions passed with their feature flags enabled.
- [x] Focused contract/security suites passed 344 tests: MCP 118, engine security 141, tools-core contract/security 15, and CLI flow/security 70.
- [x] Full verification passed 3,884 tests with 1 skipped; repository lint, typecheck, and documentation verification pass.
- [x] Efficiency sensors: compact 17-tool catalog 4,245 characters; minimal context 593; compact context 2,745. Three-trial local medians were ≤0.20s except graph analysis. A single graph operation averaged 5.18s over 5,883 files; a five-operation same-root batch averaged 5.03s total and 17,935 response characters, confirming request-local graph reuse and the 32 KiB response guard.
- [x] Post-fix verification passed 3,901 tests with 1 skipped; root build, lint, typecheck, documentation verification, release-mode native build, focused package suites, and rebuilt local-CLI graph dogfood all pass.
- [x] Research skill flow now states the graph boundary explicitly: declared workspace `package.json` exports resolve, inferred aliases do not. Post-edit skill review reports 0 ERROR / 0 WARN, the description contract passes, and documentation verification passes.
- Ratings after fixes: tool contracts **9.5/10**; local graph tool **9.3/10**; execution/response flow **9.3/10**; security **9.5/10**; efficiency **9.0/10**. Overall **9.3/10**. Remaining graph limitations are intentional: exact repository-declared exports are supported, while custom aliases and bundler-only resolution are not inferred.
