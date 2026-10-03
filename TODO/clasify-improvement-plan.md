# Improve clasify reliability and usefulness

Status: phases 0–4 and 6 implemented and verified on 2026-10-02 (phase 4 limited to the identifier fix; the policy part is blocked on phase 5); phase 5 is blocked. Audit date: 2026-10-01.

Make `clasify` enforce its capture limits, return evidence the host can verify, and provide continuations that advance. Then measure which classification workflows reduce complete-task work before expanding automatic suggestions.

The audit passed 203 focused tests and reproduced three failures: search candidates bypassed `maxChars`, a high sufficiency verdict omitted the answer needed by the host, and an emitted continuation immediately exceeded its resource budget. These are observations from the audited build. Reproduce them on matching contracts before implementation because concurrent sessions can change the source and generated artifacts.

## Delivery order

| Phase | Change | Reason | Acceptance criterion |
| --- | --- | --- | --- |
| 0 | Freeze a reproducible baseline | Separate an implementation change from stale builds or contract drift | Matching contracts, retained probe receipts, and reproducible failures |
| 1 | Enforce one capture budget across resource paths | Prevent unexpected evidence submission and provider work | Over-budget evidence never reaches classification; remaining candidates stay reachable |
| 2 | Correct sufficiency guidance and verification reads | Distinguish provider knowledge from evidence available to the host | The host retrieves the deciding fact before answering from unread Scout evidence |
| 3 | Prepare continuations that fit their budget | Avoid predictable failed replays and repair turns | Replay advances without skipped evidence or repeating continuations |
| 4 | Narrow automatic classification suggestions | Avoid unnecessary round trips and premature defaults | Exact lookups avoid classification; semantic cases retain useful verification routes |
| 5 | Compare complete workflows | Establish task-level benefit rather than response-size savings | Meet a frozen quality and efficiency gate on held-out tasks |
| 6 | Extract the responsibilities implicated in the bugs | Give budget and coverage invariants clear owners | Preserve behavior under regression checks and remove duplicate enforcement paths |

## Phase 0: freeze the baseline

- [x] Record the core and native fingerprints, source revisions, local changes, build versions, configuration, and provider model identity without recording credentials.
- [x] Resolve fingerprint drift through the established contract pipeline.
- [x] Reproduce the three failures and turn them into deterministic regression cases using a local provider fixture.
- [x] Preserve requests, responses, coverage, provider-call counts, and failure reasons.
- [x] Record which existing tests pass before changing implementation.

Why this helps: an earlier debug-continuation observation came from a stale binary; rebuilding showed that the source already preserved `debug`. Matching contracts and retained receipts prevent that observation from becoming an unnecessary fix.

Evidence: [audit receipts](../.octocode/tmp/octocode-roast/clasify-20261001/) and [gotchas](../.octocode/GOTCHAS.md).


### Results (2026-10-02)

- Identity: monorepo `81530d26c` + uncommitted concurrent work, core `a1f2c4d` (dirty, other agents), CLI `octocode 19.2.0 (native 20.0.0)`, config 20.1.0, contract fingerprint `8a00aee8…a9c2e` (core 19.1.6), provider `jev-latest` default (no model override in `config --json` env keys). `scheme clasify` passed (no drift) before and after the rebuild; no regen was needed for this plan.
- Reproduced on the pre-change build (receipts in the session scratchpad, mirrored by the regressions below): `maxChars:1` Scout over `localSearch` (`runtime/`, `maxChars`, `pageSize:2`) made 2 provider calls / 1,685 input tokens; the 2,000-char/8-line walk replay failed `classificationContextTooLarge` on a 2,233-char page (17–24); `sufficient` over `clasify_handoff.rs:16-19` returned 0.95 with lines only. Two extra failures found while reproducing: a replay after the source changed silently followed `next.restart` and judged the new version from line 1, and hydrated `fileChunks` evidence reached 2,464 chars under `maxChars:900` (per-candidate split ignored per-window reads).
- Deterministic wiremock regressions (`crates/runtime/tests/runtime_clasify.rs`): `search_candidates_above_max_chars_never_reach_the_provider`, `search_candidates_past_the_remaining_budget_resume_without_skips`, `list_items_above_max_chars_never_reach_the_provider`, `hydrated_candidates_share_one_max_chars_budget`, `sufficient_unread_file_evidence_returns_a_bounded_verification_read`, `confident_negative_file_pages_add_no_read`, `an_oversized_next_page_shrinks_and_the_replay_advances`, `a_line_larger_than_the_whole_budget_is_terminal_not_a_repeating_continuation`, `a_changed_source_is_rejected_on_replay_instead_of_mixing_versions`. Red before the fix: 8/9 (the terminal-line case first passed vacuously; tightened to a 1-line walk, then red).
- Pre-change green: 150 clasify unit tests (`--lib clasify`), 38 `runtime_clasify`, 5 `runtime_clasify_routing`.

## Phase 1: enforce the capture budget

- [x] Check sanitized evidence before search-candidate and list-splitting branches return captured pages.
- [x] Account for evidence across all candidates and pages belonging to one resource.
- [x] Use the same definition of characters across resource paths; keep evidence accounting separate from provider request overhead.
- [x] Preserve unread candidates through executable continuations, or return an explicit size error when no supported capture fits.
- [x] Keep source scopes accurate when selecting a smaller capture.

Why this helps: `maxChars` becomes a dependable limit on evidence submitted for classification. The audited search branch bypasses the generic budget check, so a caller's cap does not constrain that path.

Acceptance checks:

- The `maxChars: 1` search probe makes zero provider calls. Its audited response reported one call, 549 input tokens, and 20 output tokens.
- Exercise ordinary file pages, search snippets, hydrated candidates, AST/LSP lists, and supported supplied-state forms.
- Cover Unicode, many small candidates, and oversized individual items.
- Confirm that budget exhaustion never silently drops candidates.

Owner: [capture orchestration](../packages/octocode-native/crates/runtime/src/runtime/clasify_batch.rs). Reproduction: [capture-cap probe](../.octocode/tmp/octocode-roast/clasify-20261001/octocode-clasify-capture-cap-probe.json).


### Results (2026-10-02)

- New `runtime/clasify_budget.rs` owns the evidence-character definition and the budget rules. `capture_pages` applies them on every path: snippet candidates, hydrated chunks, list items (`clasify_items::split`), and file pages.
- In page order, a candidate larger than the whole cap fails `classificationContextTooLarge` with its `next.read`. The first candidate that only overflows the call's remainder is deferred with all later ones. For `localSearch`/`ghSearchCode` without a pending match page, the deferred candidates resume through `next.clasify` at that file row (an aligned smaller `pageSize`). Otherwise they fail `classificationBudgetSpent` with their reads, so nothing is silently dropped. Per-candidate size failures no longer collapse into one read-less error.
- Hydration splits the budget over the planned reads (windows), not the candidate count, and checks the sum afterward. Supplied values now count characters the same way as tool evidence (keys and scalars, no JSON punctuation).
- Live (rebuilt CLI): the `maxChars:1` probe made 0 calls (audit: 1 call/549 tokens) and returned both candidates as `classificationContextTooLarge` with reads plus `next.clasify` page 2. `maxChars:2500`, `pageSize:3` judged 2 files (2 calls) and resumed the third at `page:2,pageSize:2`; the replay judged it (1 call), so each file was judged once. MCP: a `maxChars:1` search resource beside a file resource errored with its read in the same matrix.

## Phase 2: correct sufficiency semantics

- [x] Define a high `sufficient` score as an assessment that the captured evidence states an answer.
- [x] Allow skipping another read only when the host already holds the deciding evidence.
- [x] For unread Scout resources, return a bounded verification read even when sufficiency is high.
- [x] Keep Judge compatible with caller-supplied evidence.
- [x] Update authored tool guidance, documentation, research references, and affected harness expectations together.
- [x] Reuse source scopes and `next.read` before considering a new public field.

Why this helps: the provider can assess evidence that the host has never received. A score cannot substitute for the actual fact needed to answer or cite a source.

Acceptance checks:

- The threshold probe retrieves the actual answer, eight files, before answering. The audited result returned `sufficient: 0.95` and source coordinates without that value.
- A high score alone cannot satisfy an answer-quality check for unread evidence.
- A Judge call over evidence already held does not force redundant retrieval.

Owners: [clasify documentation](../docs/OCTOCODE_CLASIFY.md), [research reference](../skills/octocode-research/references/clasify.md), and [result projection](../packages/octocode-native/crates/runtime/src/runtime/clasify_output.rs). Reproduction: [sufficiency probe](../.octocode/tmp/octocode-roast/clasify-20261001/octocode-clasify-sufficient-probe.json).


### Results (2026-10-02)

- `clasify_output::verification_read`: a page judged from a delegated file read that has no published read gets `next.read` for exactly its judged lines (the private `fileRead` template narrowed to `scope`, with its snapshot and ref kept). This happens unless every verdict is a confident "no" (P(yes) < 0.36). Pages with a located window keep the locate route. Supplied `value` resources get no read.
- Provider template wording ("content alone states the answer") is unchanged. No new public field and no core change. Docs (`docs/OCTOCODE_CLASIFY.md` sufficiency rules, limits, pipeline table) and `skills/octocode-research/references/clasify.md` now say: read the bounded `next.read` before answering from unread evidence, and skip it only for evidence you already hold. The compact-output expectation in `unified_and_nested_matrices_reach_the_provider_identically` now includes the read.
- Live: the threshold probe returned `sufficient:0.95` + `next.read` for `clasify_handoff.rs:16-19`. Running that read returned `const WIDE_RESULT_FILES: usize = 8;`. MCP returned the same (0.94).
- Harness: no clasify-suite expectation encodes the "no read on sufficient" rule (suite is locate-only).

## Phase 3: make bounded continuations advance

- [x] Distinguish a page that exceeds this call's remaining budget from a page that exceeds the resource's entire budget.
- [x] Defer a page only when it fits a fresh call under the same resource cap.
- [x] For a page that exceeds the entire cap, reduce the delegated page size through bounded attempts while preserving source coordinates.
- [x] If the smallest supported capture cannot fit, return an actionable terminal error without a repeating continuation.
- [x] Preserve source snapshots, GitHub references, questions, `debug`, and ranking state across replays.

Why this helps: copying an emitted continuation advances assessment instead of requiring the caller to diagnose a size conflict that capture already encountered.

Acceptance checks:

- Replay the 2,000-character/eight-line probe unchanged. The audited continuation failed on a 2,233-character page.
- Verify progress and accurate coverage across variable-length lines and pages.
- Verify that no required source interval disappears or repeats indefinitely.
- Reject changed source snapshots instead of mixing versions.

Owners: [capture and continuation orchestration](../packages/octocode-native/crates/runtime/src/runtime/clasify_batch.rs) and [source context](../packages/octocode-native/crates/runtime/src/runtime/clasify_context.rs). Reproduction: [first call](../.octocode/tmp/octocode-roast/clasify-20261001/octocode-clasify-debug-probe.json) and [failed replay](../.octocode/tmp/octocode-roast/clasify-20261001/octocode-clasify-debug-probe-next.json).


### Results (2026-10-02)

- `CaptureBudget {left, cap, defer}` separates what this call has left from the resource's whole cap (prefilter windows pass the shared budget as `cap`). A page over `cap` is re-read at the same `pagination.offset`/unit/snapshot with a proportionally smaller `chunkSize`, up to 4 attempts. It is then judged, or deferred when it fits `cap` but not `left`. The following page returns to the original page size. A 1-unit page still over `cap` fails `classificationContextTooLarge` with no continuation.
- `clasify_context::exact_continuation` no longer follows a tool's `restart`. A replay after the source changed fails `staleSnapshot` instead of mixing versions. `debug`, questions, and snapshots survive replays (regression asserts `debug:true`).
- Live: the audited walk (`docs/OCTOCODE_CLASIFY.md` 1–60, 8 lines, `maxChars:2000`) completed in 6 calls / 6 provider requests with contiguous scopes 1–16, 17–22, 23–27, 28–32, 33–57, 58–60 and no errors (audit: replay failed on the 2,233-char page). Remaining: a replay that starts on a shrunk page keeps the shrunk size, because the continuation does not carry the original size. Coverage is still contiguous.

## Phase 4: narrow automatic suggestions

- [x] Suppress large-file classification suggestions for recognizable exact identifiers.
- [x] Keep direct search and bounded reads as the default for exact targets.
- [ ] Keep generic hydrated Scout available explicitly while evaluating whether automatic suggestions improve complete tasks.
- [x] Preserve optional semantic locate for unread files when it can select a useful verification window.
- [x] Align runtime suggestions with authored guidance and documented admission rules.

Why this helps: the audited large-file handoff offered classification for `MAX_CALL_CAPTURES`, which classification immediately redirected to literal search without a read or provider call. Generic hydrated Scout also lacks a demonstrated broad task-level benefit in the recorded evaluations.

Acceptance checks:

- Exact-lookup controls make no unnecessary classification calls.
- Semantic cases still return useful verification windows.
- Compare relevant candidate coverage before and after changing suggestions.

Owner: [handoff policy](../packages/octocode-native/crates/runtime/src/runtime/clasify_handoff.rs). Evidence: [literal redirect](../.octocode/tmp/octocode-roast/clasify-20261001/octocode-clasify-literal-route-probe.json) and [evaluation history](../.octocode/JEV.md).


### Results (2026-10-02)

- `clasify_handoff::large_read_handoff` offers no `next.clasify` when the read's goal is a bare identifier (`clasify_locate::bare_identifier`, the same test clasify uses to route to `localSearch`). Unit cases: `MAX_CALL_CAPTURES` (localFetch) and `` `newElementWith()` `` (ghGetFileContent).
- Live: `localFetch` of `clasify_batch.rs` (3,541 lines) with goal `MAX_CALL_CAPTURES` → `next:{continue}` only; with a descriptive goal → `next:{clasify, continue}`. The same holds over MCP. Semantic locate/search handoffs are unchanged, and candidate coverage for semantic cases is unchanged by construction (only identifier goals are filtered).
- Unchecked: generic hydrated Scout stays the automatic ≥8-file semantic-search handoff. Whether to narrow or remove it requires the phase 5 task-level comparison.

## Phase 5: measure complete workflows

- [ ] Freeze tasks, baseline identity, candidate versions, metrics, guardrails, trial budget, and acceptance criteria before comparing policies.
- [ ] Compare direct search/read, optional semantic locate, and optional hydrated Scout under equivalent conditions.
- [ ] Separate development tasks from held-out tasks by repository where practical.
- [ ] Keep answer keys and evaluation feedback outside solver access; use fresh solver sessions.
- [ ] Grade observable task outcomes with deterministic checks and source verification where possible.
- [ ] Record retries, failed attempts, and unknown provider usage; missing receipts do not mean zero cost.

Why this helps: a shorter response or more efficient provider batch does not establish lower host-token use, better answers, or faster task completion.

Measure task success, missed critical facts, actual host tokens, provider usage, verification reads, provider-call counts, and total elapsed time. Report latency and provider consumption separately from host tokens.

Proposed acceptance target: at least 15% lower host-token use within the intended task category. Require no task-success regression and no additional missed critical facts on the held-out cases. This is a proposed target, not a measured saving. Freeze the latency tolerance before running the comparison. Insufficient or noisy evidence produces an inconclusive result.

Owners: [unified benchmark harness](../packages/octocode-benchmark/compare/unified/README.md) and [evaluation skill](../skills/octocode-eval-benchmark/SKILL.md).


### Results (2026-10-02)

- Blocked. `node packages/octocode-benchmark/compare/unified/run.mjs --help` runs (it requires `--run-id`), but the harness is under external rewrite, and a frozen held-out comparison needs fresh solver sessions plus provider spend beyond this pass's budget of 40 live calls. No policy was promoted to a default.

## Phase 6: extract clear internal responsibilities

- [x] Protect the corrected behaviors before restructuring orchestration.
- [ ] Extract capture and budget planning, provider execution, and result/continuation handling.
- [ ] Use generated query types at contract boundaries and small typed internal structures where they clarify invariants.
- [x] Give each budget and coverage rule one owner.
- [x] Preserve public behavior and existing security checks, cancellation handling, output order, and snapshot validation.

Why this helps: the budget failure crosses branches with different accounting. Clear internal responsibilities reduce the chance that another tool integration bypasses a shared rule. Moving code into more files alone does not establish improvement.

Owner: [native architecture](../packages/octocode-native/ARCHITECTURE.md) and [clasify orchestration](../packages/octocode-native/crates/runtime/src/runtime/clasify_batch.rs).


### Results (2026-10-02)

- Extracted `runtime/clasify_budget.rs` (child module of `clasify_batch`): character accounting, `CaptureBudget`, `budget_candidates`/`budget_spent`, `resume_search`, `shrunk_page`/`restore_chunk`, and `too_large` (one error constructor in place of inline copies), plus their unit tests. `ARCHITECTURE.md` and the clasify docs name the new owner. Behavior is protected by the phase 1–3 regressions.
- Unchecked: provider execution and result/continuation handling remain in `clasify_batch.rs` (about 3.5k lines). A further split was not started in this pass, and generated query types at the internal boundaries were not adopted.

## Validation (2026-10-02)

- `cargo test -p octocode-native --no-fail-fast`: all clasify targets green (lib 1114 passed, `runtime_clasify` 47, routing 5). Two timing tests outside clasify (`runtime_github_scope::pull_request_commit_details_load_concurrently_in_order`, `auth::discovery::discovery_uses_explicit_path…`) failed under concurrent build load and passed when rerun alone.
- `cargo test -p octocode-cli`: green. `cargo clippy --workspace --all-targets -- -D warnings`: clean. rustfmt was applied to the touched files. `dev.mjs docs:verify` passed.
- Live provider calls this pass: about 14 (budget 40). Harness (`OCTOCODE_BETA=true run-all.mjs large-files,workflows`): 49/0 and 27/0. The clasify harness suite was not run (dozens of paid locate walks).

## Verification and rollout

Ship phases 1–3 as small correctness changes. Evaluate phase 4 independently. Start phase 6 after regression checks protect the intended behavior.

Author public schemas, instructions, and limits in the sibling core package. Build core, regenerate contracts, immediately rebuild native, and rebuild affected consumers. Do not edit generated contracts or wire types by hand. The [development skill](../skills-dev/octocode-dev/SKILL.md) owns the commands and verification gates.

Run the focused regressions and existing classifier/runtime/provider checks. Exercise actual CLI and MCP calls against matching contracts, including a cross-tool handoff, a verification read, and a continuation replay. Check documentation links and guidance after wording changes. Preserve concurrent work and leave commits to the human or checkpoint bot.

Completion means bounded evidence submission, deciding evidence available to the host, advancing continuations, and measured benefit for any policy promoted to a default.
