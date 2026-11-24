---
status: Implemented
question: Where do the tokens and the time of an Octocode answer go, and which changes cut them without losing evidence?
---

# Context efficiency: what an Octocode answer costs, and what to cut

The tool definitions an agent re-reads on every request are 77% of all input tokens of a research answer, so
the published schema is the first thing to shrink and the number of requests the second. Reply size and
engine time are small by comparison. On the owner's instruction every proposal except P7 is now landed: the
prefix is 1,087 tokens per request smaller, two chains lost a step, and all 27 answers of the rerun are
correct. P7 does not fit the instruction ceiling. A 69-question round then measured 10% fewer input tokens per answer (95% CI 5–15%) with tool calls and output
unchanged; answer correctness in that round was not judged.

## Summary

| | Measured on 9 tasks, 3 passes each, Claude Sonnet 5.5, default 12 tools |
|---|---|
| Input tokens per answer (median task) | 72k, of which 89% is the fixed prefix re-read on each request |
| Octocode's part of the prefix | 20,050 of 23,100 tokens (3,052 without any MCP server) |
| What the surface's tokens are | about 60% JSON-schema structure, 40% prose; 2.3 bytes per token |
| Engine time | 7.9 s of 120 s wall time; the rest is the model |
| Reply characters | 0.5–12 KB per task, except a paged walk (24 KB median) |

- **F1 (P1, landed):** published schemas drop two more bounds an agent cannot hit.
- **F2 (P2, landed):** astSearch lists `rule` as an open object with a one-line grammar note.
- **F3 (P3, landed):** the localSearch `view` note says what `matchOnly` returns.
- **F4 (P4, landed):** `lspSearch` anchors by name when no `line` is given.
- **F5 (P5, landed):** a declaring search hit points at its own `declarations[]` row, which states its end.
- **F6 (P6, landed):** an empty glob result shows the globs as read.
- **F7 (P7, not landed):** the clause does not fit the 4,096-character instruction ceiling (Q2).
- **F8 (P8, landed):** `responseChars` says "Page size in characters."

A first round built P1 and P3 alone, measured them on nine tasks, saw calls rise from 22 to 30, and reverted
them. The same tasks on the binary with everything landed took 22 calls. Both results sit inside the noise
described next; the history is kept in the proposals because it is the evidence for Q0.

One caveat decides how to read every per-task number below. Two runs of the same nine tasks on the same
surface differ by 15% in input tokens (676k and 780k), and one task alone ranged from 3 to 12 calls. No
saving proposed here is larger than that. Per-request prefix tokens are deterministic and were measured
directly; per-task totals show direction at best.

## Glossary

| Term | Meaning here | Where it lives |
|---|---|---|
| Prefix | Everything the model re-reads before the conversation on each request: the host's system prompt, the server instructions and every tool definition | host + `initialize.instructions` + `tools/list` |
| Request | One model call. A task with two sequential tool calls is three requests | provider usage, one per assistant message |
| Published schema | The input schema hosts list. It is looser than what native admits | core `publishedJson.ts`, `publishedCompaction.ts` |
| Admission | Native validation against the enforcement contract; refuses with the field named | `packages/octocode-config/contract/tool-contract.json` |
| Bare row | A call that sends row fields beside or instead of `queries` | repaired by native, disclosed in `warnings` |

## Problem and context

**Method.** Nine research tasks (T1–T9, below), each answered by a fresh headless agent with only the Octocode
MCP tools, three passes per binary, against a frozen copy of the build. Tokens are the provider's usage, split
into new input, cached re-reads and output. Answers were checked against keys computed with `rg`, `gh` and
`ast-grep`. The prefix was measured by serving a tool list from a stub MCP server and reading the first
request's input tokens; a pass-through proxy served candidate surfaces over the real binary.

**Baseline (median of three passes).**

| Task | Chain | Calls per pass | Requests | New input | Cached | Output | Wall | Engine | Reply chars |
|---|---|---|---:|---:|---:|---:|---:|---:|---:|
| T1 local search → fetch | Tokio `MAX_PERMITS` | 1/1/1 | 2 | 4 | 48,230 | 1,026 | 9.4 s | 33 ms | 3,512 |
| T2 symbol trace | Prometheus `Head` callers | 3/2/2 | 2 | 2,795 | 49,215 | 1,560 | 12.3 s | 384 ms | 4,901 |
| T3 GitHub search → read | Express `trust proxy` | 2/2/2 | 3 | 5,393 | 65,717 | 1,076 | 11.8 s | 2,004 ms | 3,144 |
| T4 history | Flask PR for a config | 2/2/2 | 3 | 6,638 | 65,784 | 1,143 | 12.6 s | 1,625 ms | 5,662 |
| T5 package lookup | zod 3.22.4 → `ZodError` | 3/3/3 | 3 | 4,378 | 65,670 | 683 | 10.1 s | 2,581 ms | 536 |
| T6 large file | Redis `checkChildrenDone` | 2/2/2 | 3 | 6 | 72,660 | 1,492 | 12.5 s | 132 ms | 4,783 |
| T7 many-hit search | Django `gettext_lazy` ranking | 2/3/3 | 3 | 2,007 | 70,518 | 1,281 | 10.9 s | 515 ms | 1,822 |
| T8 paginated walk | Redis config registrations | 3/7/5 | 4 | 14,015 | 121,522 | 3,539 | 24.6 s | 186 ms | 23,675 |
| T9 code shape | Tokio `async fn` with `unsafe` | 3/2/2 | 3 | 6,698 | 69,939 | 1,805 | 16.1 s | 390 ms | 11,515 |
| **Sum of medians** | | 22 | 26 | 41,934 | 629,255 | 13,605 | 120.3 s | 7.9 s | 59,550 |

All 27 answers matched their keys. New input depends on whether an earlier run left the prefix in the
provider cache, so new + cached is the comparable number.

**Where the tokens go.** 26 requests × 23,150 prefix tokens = 89% of the 675,797 input tokens. Octocode's
20,050 of each prefix is 77% of the total. Per tool, alone on a stub server:

| Tool | Tokens | Tool | Tokens |
|---|---:|---|---:|
| astSearch | 2,938 | ghGetHistoryItem | 1,339 |
| ghSearchHistory | 1,989 | lspSearch | 1,327 |
| localSearch | 1,754 | ghGetFileContent | 1,280 |
| ghSearchRepo | 1,547 | ghSearchCode | 1,101 |
| structureSearch | 1,505 | ghStructure | 1,077 |
| instructions | 1,203 | localFetch | 1,013 |
| | | artifactSearch | 984 |

By kind of content, removing each from the whole surface:

| Removed | Tokens saved | Share of Octocode's 20,024 |
|---|---:|---:|
| `title`, `annotations` | 0 | 0% (the host does not show them to the model) |
| Every field description | 5,826 | 29% |
| Field descriptions and tool descriptions past the first line | 7,990 | 40% |
| Every `default` | 509 | 2.5% |
| `additionalProperties: false` (envelope and row) | 240 | 1.2% |
| Bounds: `minimum`, `maximum`, `maxItems`, `minItems`, `maxLength`, `minProperties`, `format` | 1,058 | 5.3% |
| The astSearch rule grammar (`$defs` of `rule`) | 788 | 3.9% |
| The `responseChars` note on 12 tools | 324 | 1.6% |

`surfaceBudget.ts:8-14` budgets the surface in bytes. Bytes under-weigh structure: a bound such as
`"maxItems":100` is 14 bytes and 9 tokens.

**Where the requests go.** A request costs a prefix. The chains that took more requests than the tools allow:

| Task | Extra call | Passes | Why |
|---|---|---|---|
| T6 | search, then a guessed range | 3 of 3 | One `localFetch` with `matchString` + `block:true` answers. Agents use `ranges` 237 times and `block` 7 times in 147 stored `localFetch` calls |
| T2 | `lspSearch workspaceSymbol` to find a line | 3 of 3 | `callers` needs a `line` the question does not give; the agent then answered from text search |
| T7 | a second search after `limit:5` | 3 of 3 | Twice to see ties past row 5, once following the `next` its own `limit` produced |
| T8 | 3–7 calls | 3 of 3 | Pass 1 paged 44.6 KB; passes 2 and 3 found `matchOnly` but sent one bare row per call |
| T5 | a read of a line the search had shown | 3 of 3 | Sent in the same request as the search, so it cost 140 characters, not a prefix |

Stored transcripts of the last three days (134 runs, 800 calls) add: 239 calls are bare rows (99 of 212
`localSearch`, 66 of 147 `localFetch`, 25 of 42 `astSearch`); 9 calls sent a comma-joined string for an array
field; `responseChars` was never set on a local tool.

**Cost of doing nothing.** Each request keeps re-reading 20k tokens of tool definitions. A ten-call session
re-reads 220k tokens of schema before its first reply byte counts.

## Goals and scope

- Cut prefix tokens per request without removing a field, an operation or a fact an agent acts on.
- Cut requests per answer in the chains above.
- Keep every answer correct and cited; never shrink evidence.
- Out of scope: removing tools or fields (Q3 lists the unused ones for the owner), host-side tool deferral,
  model choice.

## Features

| ID | Consumer | Before → after | Example | Accepted by |
|---|---|---|---|---|
| F1 | Any MCP agent | Published row schemas state `minimum: 0` and a `maxItems` equal to the list's own enum → they do not | `contextLines.before` has no `minimum`; `sections` has no `maxItems: 8` | V1, V2 |
| F2 | Agent writing an astSearch rule | 5 `$defs`, 6,461 bytes → `rule: {type: object}` with a grammar note, 4,566 bytes | same request, same admission | V3 |
| F3 | Agent extracting values | Pages a content view (44.6 KB, 3 calls) → extracts in one call (3.7 KB) more often | `{"view":"matchOnly","unique":"count"}` | V4 (the note tried did not achieve it) |
| F4 | Agent tracing a symbol | `lspSearch` refuses without `line` → resolves `symbolName` to its one declaration in `path` | `{"operation":"callers","path":"tsdb/head.go","symbolName":"truncateMemory"}` | V5 |
| F5 | Agent reading a found declaration | A declaring hit has no end line → it does | `{"line":1434,"declaration":true}` plus its `1434\tfunction checkChildrenDone (end 1483)` row | V6 |
| F6 | Agent whose globs matched nothing | Hint blames the slash → hint shows the globs as read | `include read as 1 glob: "a.go,b.go"` | V7 |
| F7 | Agent that set `limit` | Top-level `next` reads as unfinished → the reply says the list was cut by the caller's `limit` | see P7 | V8 |
| F8 | Any MCP agent | The cursor's page-size rule is in 12 field notes → in the one reply that needs it | `responseChars`: "Page size in characters." | V9 |

## Proposed design

Ranked by saving per risk. Each proposal: breakdown (layer, step, owner), measurement, change, expected saving,
risk, check.

### P1. Publish no bound an agent cannot hit (landed)

- **Breakdown:** context, step 3. Owner: core `packages/octocode-core/src/toolContract/publishedCompaction.ts:270`
  (`dropAdmissionOnlyBounds`).
- **Measurement:** 15 `minimum: 0` nodes cost 120 tokens; `maxItems` nodes cost 9 tokens each.
- **Change:** below the envelope and the row, drop `minimum: 0` (a negative count is not a value an agent
  sends in good faith) and a list's `maxItems` when it equals the size of its own inline enum (a longer list
  repeats a value). Admission is unchanged: native refuses both with the field named.
- **Not dropped, with the reason:** the array cap at the native bound (100): the graph viewer reads
  the published cap of the `graphQuery` target list as its batch size (`localWorkflowGuidance.test.ts:36`), so it has a consumer.
  `minimum: 1` (an agent can send `line: 0`), `maxLength` (astSearch `content` at 20,000 can be hit),
  `minProperties`, `maximum` on `limit` and `contextLines`.
- **Saving:** 23,811 → 23,520 prefix tokens, −291 per request (measured on the rebuilt binary's surface);
  `tools/list` 42,690 → 42,369 bytes.
- **Result:** the prefix fell as predicted, and total cost rose: 30 calls and 859k input tokens against 22 and
  676k (baseline) and 24 and 780k (the same baseline surface rerun as a control). One shift repeats: on T1 the
  first call carried `contextLines` in 6 of 6 baseline-surface passes and 1 of 9 passes on surfaces without
  `minimum: 0` (final binary, the pass with P3, the proxy candidate), which added a read each time. Nothing in
  the change explains that, so it is recorded as an effect of perturbing the prefix, not of this bound.
- **Decision:** reverted (core edit and test reversed, contract regenerated, native rebuilt; the served
  surface is byte-identical to baseline). The change is kept as a patch for a round that can measure it (Q0).
- **Risk if re-landed:** a request with a negative offset passes the host's schema check and is refused by
  native one step later.
- **Check:** V1, V2, and V10 before it lands again.

- **State:** re-landed on the owner's instruction after the first round's revert. On the binary with everything
  landed the nine tasks took 22 calls, as the baseline did.

### P2. astSearch publishes `rule` as an open object (landed)

- **Breakdown:** context, step 3. Owner: core `tools/_astRule.ts:186` and the published view in
  `publishedJson.ts:204`.
- **Measurement:** the recursive rule grammar is 788 of astSearch's 2,938 tokens. With the grammar replaced by
  the note below, T9 was answered correctly in 3 of 3 passes with 1 refused rule in 4 `astSearch` calls;
  baseline was 3 of 3 with 1 refused in 3.
- **Change:** publish `rule` as `{"type":"object"}` with: "ast-grep rule: pattern (one node; a fragment:
  {context, selector}), kind, regex, has/inside/precedes/follows (stopBy inside them: omit for the immediate
  neighbor, "end" for any depth, or a rule; field only in has/inside), all/any/not, nthChild, range. Nested
  nodes match too." The full grammar stays in the enforcement contract and in the
  `octocode://contract/input/astSearch/{operation}` resource the description already names. The general
  form: a definition that refers to itself is published as an open object whose note lists its keys.
- **Saving:** −788 tokens per request, 3.9% of the surface, on every task whether or not it uses astSearch.
- **Risk:** medium. An agent that does not know ast-grep has only the note. One task and three passes is thin.
- **Check:** V3. Reverses if refused rules per astSearch call rise on a wider set.

- **State:** landed in core as the general rule: in the listed views a definition that refers to itself is an
  open object and its field note carries the grammar (`publishedCompaction.ts` `openRecursiveDefinitions`).
  The exact view, the selected rows (`schema astSearch --select`) and admission keep the grammar. astSearch's
  listed schema went from about 5,750 to 4,555 bytes. T9 on the landed binary: 3 of 3 correct, 2 calls each.

### P3. Say what `matchOnly` returns (landed)

- **Breakdown:** context, step 3. Owner: core `tools/localSearch.ts:40` (`VIEW_TEXT`).
- **Measurement:** the note described five of six `view` values. `{"view":"matchOnly","unique":"count"}` plus
  a second `matchOnly` row answers T8 in one call and 3,652 characters; paging the content view took three
  calls and 44,602. Two of three baseline passes found `matchOnly` through the `unique` note.
- **Change tried:** append "matchOnly: only the matched text (a regex extracts)." (+51 bytes).
- **Result:** with the note, `matchOnly` was used in 1 of 6 T8 passes (0 of 3 on the rebuilt binary, 1 of 3 on
  the proxy candidate); without it, in 2 of 3. Six passes cannot show the note hurt, and they show no gain, so
  it was reverted. The gap stays: five of six `view` values have a meaning stated.
- **Next:** state the use, not the value: a worked second example in the description
  (`{"view":"matchOnly","unique":"count"}` for "how many of each") measured on at least ten passes.
- **Check:** V4.

- **State:** the note is landed on the owner's instruction; its effect on `matchOnly` use is still unmeasured.

### P4. `lspSearch` resolves a symbol by name when no line is given (landed)

- **Breakdown:** both, steps 3 and 5. Owners: core `tools/lspSearch.ts:30` (`line` required); native
  `crates/tool-lsp/src/lsp_search/anchor.rs`.
- **Measurement:** T2: 3 of 3 passes called `workspaceSymbol` only to learn a line (1,326 characters, 5.7 s
  on a cold server) and never asked for callers. A stored run of the same kind took 7 calls.
- **Change:** `line` becomes optional. Without it native takes the declarations named `symbolName` in `path`
  from the outline localSearch already builds: exactly one → anchor there; none or several → `correctInput`
  listing each candidate's line. Core first (schema, note "Omit: the one declaration of symbolName in path"),
  regenerate, then native.
- **Saving:** one request (about 23k tokens, 3–6 s) per symbol question. Assumption: agents then call
  `callers` directly; not measured.
- **Risk:** medium. A wrong anchor would be silent, so several candidates must refuse, never pick.
- **Check:** V5.

- **State:** landed. One declaration answers byte-identically to the same call with its line; several refuse
  with each candidate ("declares `config` 2 times: line 12 (property in Target), line 72 (method in
  Fallback)"); none refuses and names `line` and localSearch. Matching is exact on the declared name; `impl`
  blocks are not counted. `occurrence` of 1 or more without `line` is refused. T2 on the landed binary: 3 of 3
  passes called `callers` by name and none called `workspaceSymbol`.

### P5. A declaring hit states where its declaration ends (landed)

- **Breakdown:** code, step 7. Owner: native `crates/tool-local/src/local_search/enclosing.rs:15-18` and
  `types.rs:57-60`; core `outputEvidence.ts:284-302`.
- **Measurement:** T6: 3 of 3 passes read `ranges 1425-1500` for a function at 1434–1483, 26 lines too many.
  The extent is already computed (`Outline::declaration`, `enclosing.rs:182`). Agents read by `ranges`
  (237 of 280 stored `localFetch` rows), so an exact range serves the habit they have.
- **Change:** list a declaring hit's own declaration in the file's `declarations[]` and point the hit at it,
  replacing the boolean `declaration: true` with that index in one step (core, regenerate, native, the CLI
  renderer `cli/lines/hits.rs`, clasify `units/hits.rs`).
- **Saving:** over-read lines per search → read chain, and a third call when a guess is too short.
  Assumption: agents use the stated end; not measured.
- **Risk:** medium: an output-contract change with three consumers.
- **Check:** V6.

- **State:** landed with the outline as the one source of `declaration` (an index, or one index per declaring
  hit of a merged window). Methods and C functions the old keyword walk missed gain it; local bindings, files
  with no extractor and files over 1 MiB no longer carry it, and the 1 MiB note says so. A clasify unit no
  longer names a hit's own declaration as its enclosing one. T6 on the landed binary: 3 of 3 passes read to
  line 1485 for a function ending at 1483 (1500 or 1520 before).
- **Follow-ups:** a declarations row is now priced at the string that is sent, through the function that forms
  it (a 60-file page went back from 100 to 120 rows); the boolean `declaration` core still declared on
  astSearch and lspSearch rows is removed after a caller check found no sender or reader. Still open: the
  engine's lexical `rank` and `is_declaration()` now feed only tests, and `rank` is written into the stored
  scan format (`local_search/packed/write.rs:136`), so removing it is its own decision.

### P6. An empty glob result shows the globs as read (landed)

- **Breakdown:** code, step 8. Owner: native `crates/runtime-base/src/policy/include.rs:23`, with callers
  `tool-local/src/local_search/coverage.rs:248` and `tool-ast/src/ast_search/matches/render.rs:710`.
- **Measurement:** `{"include":"tsdb/db.go,tsdb/head.go"}` is read as one glob, matches nothing, and the hint
  says to start the glob with `**/`. A stored run followed that advice and failed again before sending an
  array. 9 of 800 stored calls send a comma-joined string.
- **Change:** the hint names what was read ("include read as 1 glob: `tsdb/db.go,tsdb/head.go`"), for every
  empty glob result. No comma rule: a comma is legal in a file name.
- **Saving:** one or two retries on about 1% of calls.
- **Risk:** low.
- **Check:** V7.

- **State:** landed. `Nothing searched: … Globs as read: 1 include (`tsdb/db.go,tsdb/head.go`).`

### P7. A caller's own `limit` is not "unfinished" (not landed)

- **Breakdown:** both, step 8. Owners: core `instructions.ts:87`; native paging in
  `crates/runtime-base/src/response/paging.rs`.
- **Measurement:** T7 pass 2 followed the top-level `next` after its own `limit:5`. Agents set `limit` on
  38 of 42 stored `ghSearchHistory` rows and 31 of 238 `localSearch` rows.
- **Change (one of two, Q2):** (a) one clause in the Page line: after a limit the caller set, `next` is the
  rest, to follow only when the claim needs more rows; or (b) a reply cut only by the caller's `limit` carries
  its continuation as a row `next`, and the top-level `next` stays for cuts the server made.
- **Saving:** one request on some limited listings. Not measured.
- **Risk:** (a) low but uncertain effect; an earlier round strengthened this wording because agents stopped
  paging too early. (b) changes the continuation contract.
- **Check:** V8.

- **State:** the instruction clause is not landed: variant (a) adds 47 characters, the instructions are at 4,078
  of 4,096, and the completion rule it would attach to is pinned by a test (`coldReaderGuidance.test.ts:66`).
  Variant (b) is rejected: MCP reads a missing cursor as the end of results ("Treat a missing nextCursor as
  the end of results"), so moving the continuation off the top level would invite false absence claims. What
  is landed: `docs/TOOL_DATA_CONTRACT.md` now says a `limit` you set leaves only the rest behind `next`.
  Measured since: agents followed the cursor after their own `limit` in 20 of 95 stored replies (21%), and in
  26 of 48 without one.

### P8. State the cursor's page size once, where it applies (landed)

- **Breakdown:** context, step 3. Owner: core `tools/_fields.ts:234-242`.
- **Measurement:** the `responseChars` note on all 12 tools costs 324 tokens per request, half of it the clause
  "a cursor keeps the size it was issued with". Native already reads `{"cursor":…,"responseChars":N}` as the
  cursor alone and says so in the reply's `warnings` (`contracts/normalize.rs:449-472`), at the moment an agent
  does it. An earlier draft of this RFC called that repair silent; it is not.
- **Change:** the note becomes "Page size in characters." No native change.
- **Saving:** about 150 tokens per request.
- **Risk:** low: the fact moves from every request to the one reply that needs it.
- **Check:** V9.

- **State:** landed.

## Decisions and reasoning

**D1: budget the surface in tokens, cut structure first.** Evidence: 2.3 bytes per token; structure is 60% of
the tokens; `title` and `annotations` cost none. Reasoning: byte ceilings reward trimming prose, which earlier
rounds already did sentence by sentence, and miss keywords that cost 9 tokens for 14 bytes. Rejected: cutting
more prose (each remaining sentence has a dated reason in `MCP_SURFACE_BUDGET.md`). Reverses if a host renders
schemas in a cheaper form.

**D2: prototype surface changes on a proxy before core.** Evidence: the proxy served candidate `tools/list`
over the frozen binary and ran the real tasks, with no regeneration. Reasoning: a core edit needs build,
regeneration and a native rebuild per try. Rejected: editing core per variant.

**D3: keep the native array cap published.** Evidence: `localWorkflowGuidance.test.ts:36` failed when it went;
the graph viewer sizes its batches from it. Reasoning: a bound with a reader is not admission-only. Rejected:
dropping it for 207 tokens.

**D6: revert what the round cannot show.** Evidence: the after table. Reasoning: a deterministic −291 tokens
per request is 1.3% of a request; an extra request costs 23,000. A change that might add one request in 80
cancels itself, and this round cannot see one in 80. Rejected: keeping P1 on the strength of the prefix
number alone. Reverses with V10.

**D4: do not change the Prove section.** Evidence: T5 and T2 re-read lines a hit had shown; a prove A/B was run
on 2026-11-23 and its wording kept. T5's extra read shared a request and cost 140 characters. Reasoning: the
cost is small and the wording guards correctness. Reverses if a re-read costs a request in a wider sample.

**D5: no change for bare rows.** Evidence: 30% of calls; the repair is lossless and the agents that sent
separate bare calls mostly sent them in one request. Reasoning: no measured cost beyond a 75-character warning.

## Risks and mitigations

| Risk | Mitigation |
|---|---|
| Three passes cannot show a 1–5% saving in per-task totals | Report the deterministic prefix number; keep per-task medians as direction; a size claim needs more passes |
| A looser published schema lets a bad request reach native | Native refuses with the field named; V2 checks admission is unchanged |
| P2 hurts agents that need the grammar | Wider AST question set before landing; the resource keeps the grammar |
| Tasks written by the author of the changes | Keys come from `rg`, `gh`, `ast-grep`; confirm on questions written after the change |

## Tasks and rollout

| ID | What | Where | Serves | Depends on | Done when | State |
|---|---|---|---|---|---|---|
| T-0 | A measurement round that can see a 2% saving | `packages/octocode-benchmark` | all | Q0 | V10 | todo |
| T-1 | Drop `minimum: 0` and enum-sized `maxItems` from the published row | core `publishedCompaction.ts`, `__tests__/publishedFlat.test.ts` | F1 | T-0 | V1, V2, V10 | landed |
| T-2 | `matchOnly` in `VIEW_TEXT` | core `tools/localSearch.ts` | F3 | | V4 | landed |
| T-3 | Regenerate, rebuild native, frozen-binary rerun | `packages/octocode-config/contract/` | F1 | T-1 | after table below | done |
| T-4 | Publish self-referring definitions as an open object with a key note | core `publishedJson.ts`, `_astRule.ts` | F2 | Q1 | V3 | landed |
| T-5 | Shorten the `responseChars` note | core `_fields.ts` | F8 | | V9 | landed |
| T-6 | Glob-miss hint shows the globs as read | native `policy/include.rs` and its two callers | F6 | | V7 | landed |
| T-7 | `lspSearch` anchor by name | core `lspSearch.ts`; native `tool-lsp` | F4 | | V5 | landed |
| T-8 | Declaring hit points at its own declaration | core `outputEvidence.ts`; native `tool-local`, `cli`, `tool-clasify` | F5 | | V6 | landed |
| T-9 | Limit and `next` | core `instructions.ts` or native paging | F7 | Q2 | V8 | blocked: Q2, instruction ceiling |
| T-10 | Re-measure the three surface ceilings and record them | core `surfaceBudget.ts`, `docs/MCP_SURFACE_BUDGET.md` | | Q4 | ceilings equal measured, rounded up | done: 44,100 / 53,400 / 67,200 |

Rollback for T-1, as done here: reverse the edit to `publishedCompaction.ts` and its test, regenerate, rebuild.

## Validation and success

- **V1 (F1):** given every direct tool's published input, when its nodes below the envelope and the batch are
  walked, then none has `minimum: 0` and no list has `maxItems` equal to its enum size.
  `publishedFlat.test.ts`, "bounds an agent cannot hit are admission-only": failed on 16 tools before the
  change, passes after.
- **V2 (F1):** given a synthetic form, when the published schema is compiled, then `line: 0`, three of a
  two-item list and eight of a seven-item list are still refused, and `offset: -1` is admitted. Same test file.
- **V3 (F2):** given the open-rule surface, when the AST question set runs, then answers stay correct and
  refused rules per `astSearch` call do not rise. Measured so far: T9, 3 of 3 correct, 1 refused of 4.
- **V4 (F3):** given T8 on at least ten passes per arm, when the surface names the `matchOnly` use, then the
  share of passes that extract instead of paging rises. Failed for the note tried here (1 of 6 against 2 of 3).
- **V5 (F4):** given a file with one and with two declarations of a name, when `callers` runs without `line`,
  then the first answers as with the line and the second refuses listing both lines.
- **V6 (F5):** given a search whose only hit in a file is a declaration, then the row names a `declarations[]`
  entry whose end equals astSearch `symbols`' end for it.
- **V7 (F6):** given `include: "a.go,b.go"`, then the empty row's hint contains that string as one glob.
- **V8 (F7):** given T7, then no pass follows a cursor after its own `limit` unless its answer uses the rows.
- **V9 (F8):** given a cursor and a `responseChars`, then the reply's `warnings` say the size was not applied, and
  no published field note repeats it.

**After (frozen binary with P1, same prompts), beside the baseline and a control rerun of the baseline surface.**

| Task | Baseline calls | Control calls (same surface) | Final calls (P1) | Baseline input | Control input | Final input | Final output | Final wall |
|---|---|---|---|---:|---:|---:|---:|---:|
| T1 | 1/1/1 | 1/1/3 | 2/2/2 | 48,234 | 48,234 | 73,195 | 1,375 | 11.8 s |
| T2 | 3/2/2 | 2/2/2 | 2/2/2 | 49,219 | 49,219 | 48,497 | 1,554 | 12.0 s |
| T3 | 2/2/2 | 2/2/2 | 3/2/3 | 71,110 | 71,264 | 70,645 | 1,160 | 12.9 s |
| T4 | 2/2/2 | 2/2/2 | 3/3/3 | 72,422 | 72,491 | 72,743 | 1,329 | 13.0 s |
| T5 | 3/3/3 | 3/3/3 | 3/3/3 | 70,048 | 93,731 | 92,671 | 820 | 12.1 s |
| T6 | 2/2/2 | 2/3/2 | 3/4/4 | 72,666 | 72,666 | 99,276 | 1,918 | 16.7 s |
| T7 | 2/3/3 | 3/2/3 | 3/3/3 | 72,173 | 72,158 | 71,564 | 1,273 | 10.7 s |
| T8 | 3/7/5 | 6/8/6 | 7/5/8 | 142,785 | 199,921 | 231,747 | 3,872 | 28.1 s |
| T9 | 3/2/2 | 3/3/9 | 3/3/3 | 77,140 | 100,619 | 99,156 | 1,946 | 16.4 s |
| **Sum of medians** | 22 | 24 | 30 | 675,797 | 780,303 | 859,494 | 15,247 | 133.7 s |

Input is new plus cached tokens, median of three passes. Prefix per request: 23,205 baseline and control,
22,914 final. All 81 answers of these three arms matched their keys; failed calls: 1, 0 and 4. The final
binary cost more than both runs of the baseline surface, so P1 was reverted. The control shows how far two
runs of one surface drift apart: 15% in input, with T5, T8 and T9 moving by a request each.

**Everything landed (frozen binary, same prompts, median of three passes).**

| Task | Calls per pass | New input | Cached | Output | Wall |
|---|---|---:|---:|---:|---:|
| T1 | 2/2/2 | 3,728 | 67,862 | 1,502 | 13.2 s |
| T2 | 2/2/2 | 1,981 | 44,250 | 1,345 | 10.7 s |
| T3 | 2/2/2 | 5,442 | 62,489 | 1,008 | 13.0 s |
| T4 | 2/2/2 | 6,627 | 62,509 | 1,302 | 14.7 s |
| T5 | 3/3/3 | 4,545 | 84,980 | 858 | 15.1 s |
| T6 | 2/2/2 | 1,667 | 67,676 | 1,535 | 13.2 s |
| T7 | 1/1/2 | 1,457 | 46,402 | 864 | 10.6 s |
| T8 | 7/6/5 | 20,076 | 111,415 | 3,808 | 28.4 s |
| T9 | 2/2/2 | 4,802 | 66,765 | 1,601 | 14.3 s |
| **Sum of medians** | 22 | 50,325 | 614,348 | 13,823 | 133.2 s |

Prefix per request: 22,118 (23,205 at baseline; stub probe 22,724 against 23,811, −1,087). All 27 answers
matched their keys; 1 failed call. Input 664,142 against 675,797 (baseline) and 780,303 (control): inside
the noise, so no claim on totals. Wall times overlapped a gates run. These passes ran on a build that still
held the 47-character P7 clause and lacked the out-of-range `line` guidance restored afterwards.

- **V10 (all):** given at least 30 distinct tasks and paired passes on two surfaces, when per-task input tokens
  are compared, then the interval of the geometric-mean ratio excludes 1. Passed: 69 questions, 0.900×
  (0.848–0.953). Correctness parity not yet checked.

## Open questions

- **Q0. Answered:** a 69-question paired round (29 GitHub, 40 local; base and landed surfaces over one binary)
  measured input tokens per answer at 0.900× (95% CI 0.848–0.953), with tool calls and output unchanged
  ([round](../../packages/octocode-benchmark/results/rounds/20.2.0-2026-11-24-context-efficiency/README.md)).
  Still open: correctness was not judged in that round, and the native changes were not in it.
- **Q1. Non-blocking (was blocking):** the astSearch grammar is out of the listed view on the owner's
  instruction. Still owed: an AST question set wider than T9; it reverses if refused rules per call rise.
- **Q2. Blocking for T-9:** P7 needs either 29 more characters of instruction room or the continuation change
  (b), which alters `docs/TOOL_DATA_CONTRACT.md`. Settled by: the owner.
- **Q3. Non-blocking:** fields no stored call set in 800: structureSearch `accessedAt`, `modifiedAt`, `bytes`,
  `permissions`, `access`, `empty`, `minDepth`; localSearch `keyPath`, `fileLacks`, `unique`, `sort` (set in
  today's runs); astSearch `scope`, `parserGlobs`, `content`, `includeProvenance`. The stored tasks do not
  need them, so this is not evidence they are unused in the field. Settled by: the owner; no removal proposed.
- **Q4. Non-blocking:** the byte ceilings now equal the measured surfaces (44,100 / 53,400 / 67,200). Add a
  token ceiling beside them? Settled by: the owner of `MCP_SURFACE_BUDGET.md`.
- **Q5. Non-blocking:** a search that returns each declaring hit's block (`block:true` on `localSearch` and
  `ghSearchCode`) would make search → read one request in T3 and T6. Not designed here. Settled by: a design
  that prices and pages blocks inside search rows.

## Sources

- Core, working tree on `f2e76c1`: `packages/octocode-core/src/toolContract/publishedCompaction.ts:270`,
  `publishedJson.ts:204-238`, `tools/localSearch.ts:40`, `tools/lspSearch.ts:30`, `tools/_astRule.ts:186`,
  `tools/astSearch.ts:169`, `tools/_fields.ts:234-242`, `instructions.ts:87`, `surfaceBudget.ts:8-14`,
  `outputEvidence.ts:284-302`, `__tests__/localWorkflowGuidance.test.ts:36`, `docs/MCP_SURFACE_BUDGET.md`.
- Native, working tree on `ae3a84fc6`: `crates/runtime-base/src/policy/include.rs:23`,
  `crates/tool-local/src/local_search/enclosing.rs:15-18,182`, `types.rs:57-60`,
  `crates/tool-local/src/local_search/coverage.rs:248`, `crates/tool-ast/src/ast_search/matches/render.rs:710`.
- Measurements: 162 agent runs on 2026-11-24 (27 baseline, 27 control, 27 with P1 and P3, 27 with P1, 27 on
  the candidate surface, 27 with everything landed); token
  probes through a stub MCP server (`claude -p`, first-request input tokens); 134 stored transcripts under
  `packages/octocode-benchmark/campaigns/` dated 2026-11-22 to 24.
- [Writing effective tools for agents](https://www.anthropic.com/engineering/writing-tools-for-agents):
  "pagination, range selection, filtering, and/or truncation with sensible default parameter values"; errors
  should "clearly communicate specific and actionable improvements, rather than opaque error codes or
  tracebacks" (P6, P8); "Tools can consolidate functionality, handling potentially multiple discrete operations
  (or API calls) under the hood" (P4, Q5).
- [Effective context engineering for AI agents](https://www.anthropic.com/engineering/effective-context-engineering-for-ai-agents):
  "finding the smallest possible set of high-signal tokens that maximize the likelihood of some desired
  outcome"; "Context, therefore, must be treated as a finite resource with diminishing marginal returns" (D1);
  "progressive disclosure" (P2).
- [MCP specification 2025-06-18, tools](https://modelcontextprotocol.io/specification/2025-06-18/server/tools):
  "`title`: Optional human-readable name of the tool for display purposes", consistent with the measured zero
  tokens for `title`.
- Method and environment: `packages/octocode-benchmark/METHOD.md`, `TESTING_ENV.md`.
