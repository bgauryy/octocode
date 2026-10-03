# Improve GitHub tool correctness and efficiency

Status: implemented 2026-10-02 (phases 0–4 done; phase 5 tree waste removed and commit-detail concurrency kept on measurement; history caching evaluated, not implemented). Audit date: 2026-10-01.

Make GitHub tools preserve the requested source scope, apply filters consistently, and return executable follow-up actions. Then make incomplete evidence explicit and remove measured request waste. Keep the shared contract, native runtime, provider, and adapter boundaries.

The [GitHub audit](../.octocode/octocode-dev/github-tools-2026-10-01.md) passed 83 focused tests and identified ten findings: eight reproduced through live calls or realistic CLI fixtures, plus two established from source. Passing tests did not cover outgoing GraphQL validation, endpoint filtering equivalence, or all generated continuations. Reproduce the findings on matching contracts before implementation because concurrent sessions can change the source and built artifacts.

## Delivery order

| Phase | Change | Reason | Acceptance criterion |
| --- | --- | --- | --- |
| 0 | Freeze a reproducible baseline | Separate actual regressions from stale builds and contract drift | Matching fingerprints, retained probes, and reproducible failures |
| 1 | Preserve source scope and filtering | Prevent wrong-branch evidence and inconsistent result sets | Requested refs, archive filters, ignored paths, counts, and downloads agree |
| 2 | Make accepted requests and follow-ups executable | Remove predictable failures and recovery turns | Supported qualifiers execute; emitted next actions pass unchanged |
| 3 | Repair GraphQL batching and fallback disclosure | Restore useful batching and reveal broken provider paths | Valid outgoing query, distinct review IDs, equivalent REST fallback, and an observable fallback reason |
| 4 | Disclose incomplete evidence | Prevent partial sets from supporting exhaustive claims | Coverage survives normal output and subsequent pages; bounded collections disclose their limits |
| 5 | Remove waste and measure further optimization | Reduce requests and latency without adding unproven complexity | No discarded tree fetch; further changes improve a comparable baseline while preserving behavior |

Recommended first implementation batch: phases 0 through 2. These establish reliable evidence and working workflows before optimizing them.

## Phase 0: freeze the baseline

- [x] Record source revisions, local changes, contract fingerprints, build versions, and relevant configuration without credentials.
- [x] Resolve fingerprint drift through the repository contract pipeline.
- [x] Reproduce the eight runtime findings and convert them into deterministic regressions.
- [x] Add explicit fixtures for the source-only closing-reference cap and latent review-ID defect.
- [x] Retain requests, responses, source scopes, continuation results, provider-call counts, and failure reasons.
- [ ] Record which existing tests pass before editing. (Not done as a separate pre-edit run: the shared tree did not compile mid-session because of another lane's in-progress edits. Baseline = the audit's 83 passing tests plus the live reproductions below.)

Why this helps: the audit began with a stale native fingerprint. Matching source, contracts, and binaries prevents an already-fixed behavior from becoming an unnecessary change.

Acceptance checks: the deciding regression cases fail for the audited reasons before implementation. Record any case that no longer reproduces and inspect its updated source before choosing a fix.

### Results (2026-10-02)

- Baseline: branch `codex/preproduction-hardening` @ `81530d26c` plus uncommitted lane edits. Contract fingerprint `a86125b5…` → `8a00aee8…` after this change's core regen. `yarn workspace @octocodeai/config check:tool-contract` exits 0. The CLI `scheme`/`config --json` both exit 0.
- Re-verified on the current source and a rebuilt CLI (probes are in the session scratchpad under `gh-p0/`):
  - F1 GraphQL `isMerged`: **open → fixed**. A live `gh api graphql` call returned `undefinedField` for PullRequest.isMerged.
  - F2 archived routing: **open → fixed**. Live: `archived:true` returned open PR #11371 through `/pulls`.
  - F3 ignored ancestors: **open → fixed**. In `tree.rs`, `traversal_from_git_tree` checked only the leaf name.
  - F4 sparse-file handoff: **open → fixed**. Live replay of `exploreClone` failed with `structure.execution.failed`.
  - F5 tree prefetch: **open → fixed**. The architect's "disproved" claim does not hold: `TreeResponse.sha` is the tree-object SHA and was compared against the commit SHA from `commits/{ref}` (`vnd.github.sha`). The old test hid this by using one SHA for both.
  - F6 negation: **open → fixed**. Live: `-label:bug` was schema-valid, then native rejected it with exit 2.
  - F7 non-empty incomplete page: **open → fixed**. `apply_partial` received `incomplete && items.is_empty()`.
  - F8 closing references: **open → fixed**. The query used `first:10` with no `pageInfo`.
  - F9 review IDs: **open → fixed**. No `id`/`databaseId` was selected, and `pr_sections` mapped a missing ID to `"null"`.
  - F10 requested-ref read: **open → fixed**. The fragment fallback had no branch, and the line read carried the mutable ref.
- Fixtures: `crates/runtime/tests/fixtures/github/graphql-schema.json` holds the versioned field and argument names of the 20 GraphQL types the documents use, extracted from `docs.github.com/public/fpt/schema.docs.graphql`. The closing-reference cap and review-ID fixtures live in `tests/runtime_github_scope.rs`.

## Phase 1: preserve source scope and filters

- [x] Build verified code-search reads using the resolved commit SHA.
- [x] Suppress an unqualified default-branch fragment fallback when verification at the requested ref fails.
- [x] If an indexed default-branch alternative is useful, label its scope explicitly instead of presenting it as a read of the requested branch.
- [x] Route scoped PR requests with `archived` through an endpoint that enforces the filter, or validate repository archive state before listing.
- [x] Remove paths beneath ignored ancestors before tree pagination, size reporting, and materialization.
- [x] Apply the same scope rules to recursive Git Trees and Contents traversal.

Why this helps: a successful request can otherwise deliver evidence from another branch or a result set that ignores the caller's filters. Early tree filtering also prevents phantom pages and unexpected downloads.

Acceptance checks:

- A file missing at `dev` never produces an unqualified default-branch read.
- A generated line read retains the resolved SHA even if the branch moves before replay.
- Adding a keyword can change the PR matching set, but cannot disable or enable archive-filter enforcement.
- A fixture containing `app.rs` and `vendor/hidden.rs` counts, displays, sizes, and materializes only permitted entries.
- Recursive and Contents traversal produce equivalent permitted paths for the same fixture.

Owners: [code-search output](../packages/octocode-native/crates/runtime/src/tools/gh_search/code_output.rs), [search routing](../packages/octocode-native/crates/runtime/src/tools/gh_search/mod.rs), [history search](../packages/octocode-native/crates/runtime/src/tools/gh_search_history/mod.rs), and [tree traversal](../packages/octocode-native/crates/runtime/src/tools/gh_search/tree.rs).

### Results (2026-10-02)

- `gh_search/code_output.rs`: the `readTopMatch` line read now sets `branch` to the resolved commit (`data.commitSha`), not the mutable ref. `scoped_fragment_read` pins the fragment read to that commit, drops it when the top file is `Missing` at the ref, and names the requested ref when nothing was resolved. It never emits an unqualified default-branch read.
- `gh_search_history/mod.rs`: `archived` (true or false) always routes PR queries to `/search/issues`.
- `gh_search/tree.rs`: entries under an ignored ancestor are dropped inside `traversal_from_git_tree`, before paging, sizing, and materializing. The now-redundant display-time `filter_structure` was removed. Recursive and Contents traversal now produce the same permitted set.
- Tests: `a_file_missing_at_the_requested_ref_offers_no_default_branch_read`, `branch_hits_are_verified_at_the_ref_and_labeled` (now expects a SHA-pinned read), `ignored_directories_hide_their_descendants_from_every_consumer` (Git Trees vs Contents: structure, no phantom page, `fileSizes`, materialized files), `archived_pull_request_filter_always_routes_to_search`, and integration test `archived_pull_request_listing_enforces_the_filter`.
- Live: `archived:true` on octocat/Hello-World now returns `status:"empty"` through search. The cli/cli `ghSearchCode` `readTopMatch.query.branch` equals `commitSha` `fc4b137c…`.

## Phase 2: make requests and follow-ups executable

- [x] Choose a sparse-clone follow-up by the checked-out path kind: `localFetch` for a file and `structureSearch` for a directory.
- [x] Preserve repository identity, source scope, and caller intent in generated follow-ups.
- [x] Align negative-qualifier syntax with native execution. Implement the supported negative filters or narrow the authored schema to the exact supported subset.
- [x] Reject unsupported qualifier combinations during preparation with an actionable explanation.
- [x] Replay emitted next actions unchanged in integration tests.

Why this helps: accepted inputs and suggested next actions must execute without asking an agent to repair predictable tool errors. This reduces wasted calls and keeps the workflow understandable.

Acceptance checks:

- A sparse clone of `README` emits a file read that succeeds.
- A sparse directory clone emits a directory query that succeeds.
- Multi-path sparse clones retain the appropriate exploration scope.
- Every advertised negative qualifier has an execution test; unsupported forms fail before provider work.
- Tests cover both CLI-only cloning and the read tools available through MCP.

Owners: [clone follow-ups](../packages/octocode-native/crates/runtime/src/tools/gh_clone_repo/mod.rs), [native qualifier normalization](../packages/octocode-native/crates/runtime/src/tools/gh_search_history/mod.rs), and authored history query schemas in `../octocode-mcp-host/packages/octocode-core`.

### Results (2026-10-02)

- `gh_clone_repo/mod.rs` `explore_next`: a single sparse path that is a file emits `localFetch {path}`. A directory still emits `structureSearch` tree, and multiple paths still explore the checkout root. Test: `clone_rows_continue_into_local_tools_and_name_the_cache_age` (`gh_clone_repo/tests.rs`) adds a `src/lib.rs` case that is validated against the output contract.
- Core `_toolkit.ts` `qualifiersField`: the `negated` list replaces `negation:true`. PR search admits only `-is:draft`, the one negation native executes. Issue search admits none. Core regen followed. Core test: `accepts only the negated qualifiers native executes`. The `directToolSchemaArtifacts` and `agentContextBudgets` snapshots were updated (ghSearchHistory schema +10 chars). The `tools/list` 32k budget test passes.
- `contracts/validate/schema.rs` `pattern_message`: a failing `qualifiers` pattern now names the bad term. A negation reports `"-label:bug": negation is not supported for this filter; drop the leading -.`, and other bad terms report "is not an allowed key:value filter". Native test: `schema_negation_matches_native_execution`.
- Live replay: the sparse `README` clone emits `localFetch`, and its query executes unchanged (exit 0, content `1\tHello World!`). CLI `-label:bug` returns exit 2 with the actionable message, and `-is:draft` executes. MCP `debug-call` rejects at the Zod layer with `negate only -is:draft`. A directory sparse clone was not run live (covered by the fixture test).

## Phase 3: repair GraphQL and disclose fallback

- [x] Remove the unused invalid `isMerged` selection or replace it with the correct field if required by mapping.
- [x] Request and map a stable review identifier compatible with the public output.
- [x] Validate the actual outgoing GraphQL document against a versioned GitHub schema fixture.
- [x] Preserve a useful fallback reason while allowing supported REST recovery.
- [x] Compare GraphQL and REST results for the same selected surfaces.
- [x] Preserve deadline, cancellation, and permission-error behavior during fallback.

Why this helps: the audited fast path always fails schema validation on real GitHub, then hides the failure. Repairing the document restores batching; repairing review IDs in the same phase avoids activating a latent identity bug.

Acceptance checks:

- The outgoing PR document passes schema validation and a bounded live read.
- Two reviews return two distinct stable identifiers.
- A forced GraphQL failure produces equivalent REST evidence and a useful fallback reason.
- Request traces demonstrate that successful GraphQL reads avoid the REST collections they replace.

Owners: [GraphQL document and mapping](../packages/octocode-native/crates/runtime/src/tools/gh_get_history_item/graphql.rs), [PR fallback](../packages/octocode-native/crates/runtime/src/tools/gh_get_history_item/pull_request.rs), and [review projection](../packages/octocode-native/crates/runtime/src/tools/gh_get_history_item/pr_sections.rs).

### Results (2026-10-02)

- `graphql.rs`: `pull_request_document` was extracted and `isMerged` removed (unused; state comes from `mergedAt`). Reviews now select `databaseId` and `commit{oid}`, mapped to the REST `id`/`commit_id`. `GraphqlOutcome::{Unavailable, Failed(reason), Served}` replaces the silent `Option`.
- `pull_request.rs`: Cancelled and Timeout errors from GraphQL propagate. Any other GraphQL failure falls back to REST, and `debug:true` adds `graphqlFallback: "<code>: <message>"`. Permission errors still surface from REST. `pr_sections.rs` omits a missing review ID instead of writing `"null"`.
- Tests: `outgoing_documents_validate_against_the_github_schema` walks the full PR document and the closing-reference document against the schema fixture, and rejects `isMerged` and unknown arguments. Also `graphql_reviews_carry_distinct_rest_ids`, plus integration test `pull_request_graphql_fallback_is_observable_and_equivalent`: the forced `undefinedField` path gives REST evidence with its reason. The served path returns identical `reviews` and makes exactly one request (`/api/graphql`), with no REST metadata or reviews calls.
- Live: cli/cli PR #14553 (`content.reviews`, `debug`) came back with no `graphqlFallback` and two distinct review IDs (`5355584357`, `5356269136`) carrying `commitId`. The fresh-MCP `debug-call` result matched.

## Phase 4: make coverage explicit

- [x] Expose provider incompleteness in ordinary code-search output, including non-empty pages, without requiring `debug`.
- [x] Keep provider-index completeness separate from whether another accessible result page exists.
- [x] Request closing-PR `pageInfo` and provide continuation, or disclose an explicit terminal collection limit.
- [x] Preserve relevant coverage and reasons in continuation results.
- [x] Treat the selected closing PR as a candidate when the supporting set is incomplete.
- [x] Reuse existing coverage fields before introducing another public output shape.

Why this helps: callers need to distinguish a complete set from a bounded sample. A retry action or process exit code alone is a weak explanation for consumers reading structured results.

Acceptance checks:

- A non-empty provider-incomplete code-search page explicitly reports partial coverage.
- An issue fixture with more than ten closing references exposes remaining evidence or an explicit bounded-set warning.
- Later pages retain unresolved provider limits.
- CLI and an MCP server restarted after rebuilding expose equivalent coverage semantics.

Owners: [code-search coverage](../packages/octocode-native/crates/runtime/src/tools/gh_search/mod.rs), [response metadata](../packages/octocode-native/crates/runtime/src/runtime/response.rs), and [closing-PR lookup](../packages/octocode-native/crates/runtime/src/tools/gh_get_history_item/issue.rs). Public contract changes belong in authored core.

### Results (2026-10-02)

- `gh_search/mod.rs`: `apply_partial` receives `incomplete_results` on every page. A non-empty incomplete page sets `isPartial`, `partialReasons:["providerIncompleteResults"]`, `incompleteResults:true`, and an exact `next.retry` with its page. The duplicate retry builder was removed, and `hasMore` stays independent. Tests: `a_non_empty_incomplete_page_reports_partial_coverage` (unit) and `incomplete_code_search_page_is_partial_without_debug` (runtime, `debug:false`).
- `issue.rs`: the closing-reference lookup now reads `first:$first` (25) with `totalCount pageInfo{hasNextPage}`. No public cursor exists, so a bounded set uses the existing coverage fields: `isPartial`, `terminalLimit`, `partialReasons:["closingReferenceLimit"]`, and a `warnings` entry ("closedBy lists N of M…"). `readFixPr` drops to `confidence:"medium"`. No new output field. Test: `issue_closing_references_past_the_read_limit_are_disclosed` (25 of 31). No live issue with >25 references was exercised.
- Docs: `docs/OCTOCODE_TOOLS.md` and `docs/TOOL_DATA_CONTRACT.md` cover the pinned reads, incomplete pages, closedBy bound, negation subset, archived routing, and sparse-file handoff.

## Phase 5: reduce request waste and measure optimization

- [x] Replace the broken tree prefetch with one fetch at the resolved SHA, or validate prefetch against the actual tree identity from the resolved commit.
- [x] Use distinct commit and tree SHAs in fixtures.
- [ ] Measure cold and warm request counts, downloaded bytes, latency, and fallback frequency before further changes.
- [ ] Evaluate caching for immutable SHA-pinned history evidence while keeping mutable discussions and PR state fresh.
- [x] Evaluate bounded concurrency for commit-detail enrichment through existing provider admission and cancellation controls.
- [x] Keep an optimization only when it improves comparable measurements and preserves evidence quality.

Why this helps: the tree prefetch demonstrably discards useful work because it compares different Git object identities. Removing that waste has a concrete acceptance check. History caching and concurrency might help, but their benefit and complexity require measurement.

Acceptance checks:

- A cold deep tree read with distinct object SHAs makes no discarded speculative tree request.
- A ref-change fixture cannot mix a speculative tree with a different resolved commit.
- Cache hits cannot cross credential or endpoint partitions.
- Concurrent enrichment preserves ordering, cancellation, provider budgets, and error disclosure.
- Set performance targets from the recorded baseline; do not claim an unmeasured percentage improvement.

Owners: [tree retrieval](../packages/octocode-native/crates/runtime/src/tools/gh_search/tree.rs), [GitHub provider](../packages/octocode-native/crates/github/src/content.rs), [runtime cache](../packages/octocode-native/crates/runtime/src/runtime/github_cache.rs), and [commit enrichment](../packages/octocode-native/crates/runtime/src/tools/gh_get_history_item/pr_sections.rs).

### Results (2026-10-02)

- Tree prefetch was removed, not repaired. Validating it would need the commit's tree SHA, which costs a request. The recursive tree is fetched once at the resolved commit SHA. `GitHubProvider::memoized_reference`, the prefetch's only user, was deleted. Test: `deep_tree_is_fetched_once_at_the_resolved_commit` uses distinct commit and tree SHAs and asserts the exact request trace `commits/main` → `git/trees/<commit>`.
  - Cold deep listing request counts: by ref, 3 → 2. Default branch, 4 → 3 (metadata + HEAD + tree).
  - Latency stays at two sequential round trips.
  - Warm counts are unchanged (memoized ref plus cached tree).
  - A moved ref can no longer pair a speculative tree with another commit.
- Commit-detail concurrency (`content.commits.includeFiles`) is kept. Measured with mock 400 ms detail reads: sequential arrival spread was 1.21 s for 4 commits; at `COMMIT_DETAIL_CONCURRENCY = 4` (ordered `buffered`, shared transport admission and deadline, first error aborts) all 4 are in flight before the first answer. Test: `pull_request_commit_details_load_concurrently_in_order` asserts a spread under 300 ms and page order.
- Left unchecked:
  - Live cold/warm latency, downloaded bytes, and fallback frequency were not measured; only mock request counts above.
  - Immutable-history caching was evaluated and not implemented. There is no comparable live baseline, and the history path deliberately bypasses `ConditionalCache`. Adding a SHA-keyed cache needs a credential/endpoint-partitioned design and a measured repeat-read workload first.

## Implementation and verification rules

- [x] Preserve concurrent edits; never commit or stash.
- [x] Keep execution, filtering, and output shaping in native; keep CLI and MCP adapters thin.
- [x] Author schemas, descriptions, limits, and public output contracts in core. Build core, regenerate contracts, immediately rebuild native, and rebuild affected consumers.
- [x] Never edit generated contracts or hand-write duplicate wire types.
- [x] Run focused regressions after each slice, then exercise the rebuilt CLI and an MCP server restarted after rebuilding.
- [x] Check contract discovery and configuration through the real CLI.
- [ ] Run repository verification and documentation checks before completing the combined change.
- [x] Record exact results, remaining limits, and any failed or skipped checks.

Use the [development workflow](../skills-dev/octocode-dev/SKILL.md) for build and verification commands. The audit's 83 passing tests are a baseline, not evidence that these planned fixes are complete.

### Validation (2026-10-02)

- Core: `yarn typecheck` and `yarn test` (471 passed after the two snapshot updates), `yarn build`, and eslint on the edited files all pass.
- Monorepo: `yarn contracts:regen`, then `check:tool-contract` exits 0.
- `cargo test -p octocode-native --no-fail-fast`: 1279 passed, 0 failed (lib 1114; `runtime_github` 35, `runtime_github_scope` 5, `runtime_github_large_pr` 18, `runtime_github_cache` 2, `runtime_tool_flow_handoffs` 3, among others).
  - Two load-sensitive tests outside this lane flaked once and passed on rerun: `system_git_cancellation_kills_and_reaps_process_group` and `discovery_uses_explicit_path_and_removes_environment_credentials`.
- `cargo test -p octocode-cli` and `cargo clippy --workspace --all-targets -- -D warnings` pass. rustfmt was applied to the touched files.
- `node skills-dev/octocode-dev/scripts/dev.mjs build:dev` exits 0, then the live CLI and fresh-MCP `debug-call` checks above ran.
- `OCTOCODE_BETA=true node octocode-local-testing/harness/run-all.mjs github,remote`: github 89/0, remote 22/0.
- Not run: full repository verify and a docs link or lint check.

## Completion criteria

- [x] Requested source scope survives all relevant reads and continuations.
- [x] Supported filters have consistent meaning across endpoint choices.
- [x] Displayed paths, counts, sizes, and materialized files agree.
- [x] Published request syntax agrees with native execution.
- [x] Follow-up actions execute unchanged.
- [x] GraphQL queries validate, review identities remain stable, and fallbacks explain their cause.
- [x] Partial evidence remains explicit across transports and pages.
- [ ] Measurements demonstrate request reductions and support further performance claims.
