# Leftovers

Open work from graph quality, skill validation, and the research-performance review. Reconciled on 2026-11-12. Existing items are retained below; duplicate stitch-lint entries are consolidated.

**Status:** Confirmed = source or a focused check establishes the scoped finding; Candidate = measurement must justify the change; Pending = acceptance check has not run; Blocked = the environment prevents validation; Decision = behavior or scope remains unsettled. Prior failures are historical until reproduced.

**Order:** close correctness gates and freeze a baseline; repair startup; select measured response/reuse improvements; evaluate model workflows; then select broader coverage work. A blocked provider or model does not block local startup work.

## Validation snapshot

| Check | Result in this reconciliation | Interpretation |
|---|---|---|
| `node skills-dev/octocode-dev/scripts/dev.mjs docs:verify` | Passed before the edit; rerun after the final edit | Documentation validation is separate from Rust admission tests. |
| Targeted oxlint on `stitch.mjs` and `skill-review.mjs` | Passed, exit 0 | The old five-error stitch-lint claim is no longer current. |
| `node --test skills/octocode-graph-view/scripts/stitch.test.mjs` | 9 passed, 0 failed | This focused suite passed; it does not replace the full skill gate. |
| `node skills/octocode-skills/scripts/skill-review.mjs --self-test` | Passed | The optional missing-finding assertion below is still absent. |
| `node skills-dev/octocode-dev/scripts/dev.mjs skills:check` | Incomplete: 300-second deadline, exit 124; a rejected-key test printed a failure | Do not describe the full gate as passed or as failing only lint. The timeout's full cause remains unestablished. |
| Focused rejected-key test in `research-scripts.test.mjs` | Failed, exit 1: `listen EPERM` on loopback | The test cannot open its local fixture server here. This does not establish a product defect in key-rejection handling. |
| Haiku runner authentication | `loggedIn:false`, `authMethod:none` | Model trials and actual Haiku usage remain blocked. |
| Shell GitHub request to `vitejs/vite` | Failed connecting to `api.github.com` | No fair live shell-versus-Octocode remote timing comparison was completed. |
| Fresh native compilation, focused Rust failures, and full language battery | Not run in this reconciliation | Keep VAL-01–VAL-03 pending; source inspection does not establish a passing build. |

## P1 — correctness and validation gates

| ID / status | Item and owning source | Why | Next action and acceptance |
|---|---|---|---|
| **VAL-01 — Pending** | Full grammar battery after graph changes. Owner: graph engine and [battery runner](skills-dev/octocode-graph-dev/references/battery.md). | Focused graph tests do not establish all-language precision or workflow completeness. Do not assume precision is unchanged merely because a particular patch did not change a linker. | Run every registered battery case on pinned corpora, compare a compatible baseline, and inspect missed/extra sites with source/LSP. Follow all pages; failed, missing or partial rows cannot pass. Use the runner's timing guidance when comparing speed. |
| **VAL-02 — Pending; prior failures unconfirmed** | Native warnings and local-search lint: [pager re-export](packages/octocode-native/crates/runtime/src/response/pager.rs), [page model](packages/octocode-native/crates/runtime/src/response/page_model.rs), and [search page tests](packages/octocode-native/crates/runtime/src/tools/local_search/tests/pages.rs). | The old build-blocker diagnosis is stale: `returned_chars` is test-gated, `holds_value_over` remains re-exported, and related references exist. These observations do not prove either unused code or a clean build. | One build owner reruns native compilation/lint and reproduces any diagnostic at its current location. Fix the demonstrated cause without deleting evidence or lowering coverage. |
| **VAL-03 — Passing on the 2026-11-12 tree (focused); full rerun at the next build** | Focused Rust checks: LSP `no_operation_shows_a_secret_the_server_returns`; construction `sections_runtime_refuses_changed_sources_and_a_one_shot_memory_run_before_any_provider_call`; `doc_examples`; CLI `cli_skill`. | The secret-output check protects a security boundary; the others protect retained-work policy, authored examples and packaged skill behavior. Their prior failures are not freshly reproduced here. | Run the focused commands below on a current host build, inspect payloads/skips, and then run the owning gates. Preserve example/coverage floors. Documentation verification alone cannot close these checks. |
| **VAL-04 — Blocked** | Full `skills:check`, including [research-scripts.test.mjs](skills/octocode-brainstorming/scripts/tests/research-scripts.test.mjs). | Focused lint/tests passed, but the full gate timed out and the isolated rejected-key test fails because loopback listening is denied. | Rerun in an environment that permits fixture servers. Require the complete gate to exit 0; diagnose a remaining timeout independently. Do not suppress a genuine failure or label unsupported execution as success. |

Focused Rust commands, from `packages/octocode-native`, when the shared build lane is available:

```bash
cargo test -p octocode-cli --test integration cli_skill
cargo test -p octocode-native --lib no_operation_shows_a_secret_the_server_returns
cargo test -p octocode-native --lib sections_runtime_refuses_changed_sources_and_a_one_shot_memory_run_before_any_provider_call
cargo test -p octocode-native --lib doc_examples
```

Run the repository's `lint:rust` task from the root for VAL-02. Stop processes only by their owned PID/task; do not clear shared targets or caches to obtain a clean-looking run.

## P1 — performance and agent efficiency

Design owner: research-performance RFC (`.octocode/octocode-rfc-generator/2026-11-12-research-performance-and-efficiency/RFC.md`, local), status **Proposed**. These findings authorize backlog tracking; implementation still follows the repository's change rules.

| ID / status | Finding / owner | Why | Next action and acceptance |
|---|---|---|---|
| **PERF-01 — Confirmed source; earlier runtime diagnostic** | Silent daemon-start failure. [Startup client](packages/octocode-native/crates/cli/src/cli/serve/unix/client.rs) polls connectivity, discards the child handle and suppresses child output. | Earlier restricted-host diagnostics measured 2,833 ms versus 125 ms when startup was bypassed. This is scoped diagnostic evidence, not a release speedup claim. | Implement the RFC's explicit startup outcome/child-lifecycle design. Detect failure/exit without the old polling window; preserve slow valid startup, cancellation, existing server ownership and no duplicate dispatch. Memory mode is not the production fix. |
| **PERF-02 — Confirmed measurement gap** | Release/profile and executable provenance. Owner: [profiling workflow](skills-dev/octocode-dev/docs/DEVELOPMENT.md) and benchmark package. | The earlier binary's release profile was not established, and the staged binary changed afterward. Those timings cannot establish a release ranking. | Freeze digest, source/patch state, toolchain/profile, corpus, configuration, interface and cache state. Separate one-shot, fresh MCP, healthy warm server, provider replay, continuation and failed-start measurements. Reuse existing performance guards. |
| **PERF-03 — Candidate optimization** | Redundant response metadata and optional leads. Owner: core contract/runtime response production; CLI formatting follows that contract. | One text lookup emitted 678 characters versus rg's 145, but Octocode supplied extra symbol context and ready calls. Smaller output is useful only if the task still receives its evidence. | Profile and remove duplication or unused enrichment through existing views. Preserve selected source, requested fields, warnings, coverage and executable continuations. Compare the entire task and all required pages on CLI and MCP. |
| **PERF-04 — Candidate optimization** | Repeated reads/parses and cache misses. Owner: existing engine/runtime/provider reuse boundaries. | Repeated work can dominate a session; stale reuse can invalidate the answer. A missing cache has not been demonstrated. | Measure hit/miss and recomputation cost first. Reuse existing mechanisms and validate content/revision, parser/context and policy invalidation. Do not add another cache by default or memoize fresh classifier evaluations as free requests. |
| **PERF-05 — Candidate workflow improvement** | Cheapest sufficient tool routing and independent batching. Owner: core `toolContract/instructions.ts` and canonical `skills/common/references/octocode-usage.md`. | Unneeded AST/LSP/classification work and extra model turns can cost more than the underlying lookup. Existing routing is substantial; a rewrite needs evidence. | Use exact reads for known spans; text for spelling; AST for shape; LSP for identity; graphs for repeated structural questions; classification for meaning. Batch independent queries; use returned identities for dependent reads. Change guidance only where trials show misrouting. |
| **PERF-06 — Candidate host integration** | Schema and prompt-context cost. Owner: model host/benchmark adapter, with stable core descriptions. | Output characters omit schema, prompt, history and cache costs. Prompt caching reduces repeated input cost, not occupied context. | Discover needed schemas on demand and keep definitions cacheable where supported. Measure actual provider input/output/cache usage and complete transcripts; do not turn a character estimate into a Haiku token or dollar claim. |
| **PERF-07 — Blocked evaluation** | Matched Haiku comparison: Octocode CLI, MCP and expert shell/registry tools. Owner: [benchmark accounting](packages/octocode-benchmark/docs/outputs.md). | Haiku is unauthenticated and shell network access failed. Connected MCP timings do not establish the performance of the locally pinned CLI. | Establish access, freeze exact model/solver/judge and corpus identities, then compare correctness first and cost second. Include discovery, startup, retries, failed attempts and verification reads. Exercise local tasks, GitHub search/history and locked package-to-source workflows. |
| **PERF-08 — Pending evaluation** | Classification benefit and total cost. Owner: shared judgment/selection runtime and benchmark package. | No paid classification comparison was completed in this review. Reduced visible context can hide recall loss or additional classifier cost. | Compare classification on/off on suitable broad evidence. Count classifier requests/tokens, original reads, excluded/unknown units and solver usage separately. Preserve unfiltered recovery and settled-cursor replay accounting. |

The earlier package checks resolved and source-verified npm, PyPI and crates.io releases; they did not benchmark every supported registry. The GitHub checks covered multiple public repositories, but local shell requests failed. Repeat under matched access before making a comparative claim.

## P2 — graph decisions and extraction coverage

Design/evidence owner: graph-quality RFC (`.octocode/octocode-rfc-generator/2026-11-12-graph-quality/RFC.md`, local). Its outcome records prior focused validation; that record is not a fresh language-battery result.

| ID / status | Open item | Why | Next action and acceptance |
|---|---|---|---|
| **GRAPH-01 — Decision; current live reproduction pending** | F1: TS cycle through a value import used only as a type can remain `runtime:true`. | The prior RFC records this residual case. Declaration value-reference counts already exist in [js_oxc_references.rs](packages/octocode-native/crates/engine/src/signatures/js_oxc_references.rs); they must not be confused with an import binding's runtime-use fact. Demoting every unlinked value import can also demote valid JSX cycles. | Reproduce the type-only-use and JSX counterexample on the current build. Identify the missing import-specific fact before changing confidence/runtime labels. Accept only a rule that distinguishes both cases. |
| **GRAPH-02 — Decision; coverage inventory pending** | F4: distinguish calls of parameters, locals and closures with a `local-binding` unresolved reason. | The prior RFC reports insufficient per-language facts and a Scala-specific distinction. Exclusive parser coverage was not re-established here; a name-based special case would misclassify calls. | Inventory extraction facts by grammar and add invariant tests before choosing a shared reason. Keep unsupported cases explicit; do not claim all languages have the same evidence. |
| **GRAPH-03 — Decision** | Change the default `hubs` ordering from degree to PageRank? | Both are supported; the [documented default](docs/OCTOCODE_GRAPH.md) remains degree. A more sophisticated ranking is not automatically more useful. | Held-out hub-task A/B with identical scope and evidence; change the default only if task quality improves without hiding qualifying rows. |
| **GRAPH-04 — Decision** | Follow file renames in `cochange`? | [cochange.rs](packages/octocode-native/crates/runtime/src/tools/ast_graph/store/query/cochange.rs) explicitly uses `--no-renames`. Following one file's history changes cost and potentially the interpretation of coupling. | Define identity and commit-count semantics, measure representative rename histories, and test path variants before choosing behavior. |
| **GRAPH-05 — Deferred scope** | Runtime-frequency weighting. | Syntax sites, binding confidence, PageRank and Git co-change are not observed runtime frequency. | Select a supported profile format, provenance and owner before a separate ingestion design; never relabel syntax weights as execution counts. |
| **GRAPH-06 — Candidate follow-on** | Broader language/document/community navigation and a Graphify comparison. | Graphify's reviewed source/docs offer broader architecture-context features. Its runtime was not installed or compared here; its six-question code study and naive full-corpus token benchmark do not establish a win over Octocode/Haiku. | Rank real unsupported tasks before adding grammars or a document graph. If adding Graphify as an arm, pin source/version and count graph build/refresh plus equivalent task completion. Reviewed source: [Graphify 06aa51b](https://github.com/Graphify-Labs/graphify/tree/06aa51be050170675912781ad8a2be2b20439d39). |

## P2/P3 — skill follow-ups

| ID / status | Item and source | Why | Next action and acceptance |
|---|---|---|---|
| **SKILL-01 — Decision** | Should `octocode-test-plan` join `github_skill` in [environment.rs](packages/octocode-native/crates/cli/src/cli/skill/environment.rs)? | The static list does not include it. GitHub is used for PR input, while local test planning need not require GitHub readiness. | Decide conditional capability semantics before changing the list. Verify local planning without credentials and actionable authentication guidance for PR-driven input; avoid globally marking local planning partial. |
| **SKILL-02 — Pending measurement** | Repeat test-plan skill activation trials. | The old note reports one Sonnet run as 10/10; the raw activation receipts, metric and repeated-trial denominator were not revalidated here. It cannot justify small comparative differences. | Retain the original evidence, define what the score counts, pin the host/model and repeat on unseen matched prompts before claiming reliability or improvement. |
| **SKILL-03 — Confirmed maintenance constraint** | [skill-review.mjs](skills/octocode-skills/scripts/skill-review.mjs) has 400 lines, matching the configured max-lines limit. | Additional logic can break lint; lowering the limit's protection or coverage floor would hide the constraint. | When extending it, reduce duplication or extract a cohesive module with its callers and checks. Targeted lint and the existing self-test pass now. |
| **SKILL-04 — Optional coverage improvement** | The `broken-package` self-test checks collection membership, not the emitted `skill-missing` finding. | Discovery can pass while the intended diagnostic regresses. The current self-test's success does not cover that assertion. | Assert the expected finding on the discovered broken package. Demonstrate that removing the diagnostic fails the test; keep valid collection behavior. |

Native skill delivery remains part of **VAL-03**; it is not a second duplicate task here.

2026-11-12 orchestrator note: the four VAL-03 checks were reproduced and fixed on the current tree — `doc_examples` (extractor now reads `octocode.js <tool> '{…}'`), `sections_runtime_refuses_changed_sources…` (expects `staleSnapshot` after the stale-replay fix), the LSP secret test (security lane), and `cli_skill` (octocode-engineer links to octocode-workflows references restored); each passes on its own. Agents now on: PERF-01; VAL-01 + GRAPH-01…04; VAL-04 + SKILL-01/03/04. PERF-02…08 run after the next pinned build.

## Cleared and superseded diagnoses

- **Old stitch lint blocker:** targeted lint passes, nine stitch tests pass, and the current resolver/stitch functions use option/context objects. The duplicate graph-quality/test-plan entries are consolidated into this result. Full skill-gate completion remains **VAL-04**.
- **Old pagination build diagnosis:** do not automatically remove `holds_value_over` or `returned_chars`. Current source differs from the original report; fresh diagnostics decide **VAL-02**.
- **Original graph-ranking/text fixes:** the graph RFC records fixes for unreachable critical-file candidates, unequal tied scores, stale function-value text and impact-as-breakage wording. They are not reopened as confirmed defects here. **VAL-01** remains the all-grammar validation obligation.

## Completion rule

Close an item only with evidence at its claimed boundary. A source review, one passing fixture, a skipped test, a cached answer or a shorter first page cannot establish full runtime correctness, completeness or model savings. Preserve failures and unknowns, keep one owner per fact, and update this file when a deciding check changes its status.
