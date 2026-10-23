Load when the task class, mode, proof bar, read span, or multi-pass plan is unclear: bugs, root causes, consequential claims (callers, cycles, reachability, deletion, architecture), loops, workers, handoffs.

## Classify the task

Skip stages a known anchor answers; a lookup is a direct read.

| Class | Definition of Done |
|---|---|
| bug | supported contract violated, lobby root-cause rule holds, regression check exists |
| feature | consumers, acceptance tests, and a compatibility decision set |
| enhancement | contract holds; baseline, target, experiment, regression guard exist |
| unknown | one missing fact has its cheapest check or a focused question |

## Pick a mode

| Mode | Use when | Chain |
|---|---|---|
| Map | area or owners unknown | literal + synonyms → repos/packages → tree/search → exact finalist reads → clusters |
| Validate | idea, claim, or build-vs-reuse | reframe/invert → local → external → Advocate/Critic → build, narrow, or drop |
| Investigate | symptom needs a cause | structure → symptom search → exact boundary reads → graph/LSP/AST → history/tests |
| Plan / Architecture | change needs its safest step | contract → entry points → graph/LSP scope → exact boundaries → safest step/tradeoffs |

- Pick surfaces (local, GitHub, packages, PR/history, docs/web) by the claim; name spanned surfaces you skip. Local ↔ upstream: [workflows](workflows.md#local-and-remote-together).
- Advocate/Critic: strongest evidence for and against; rebut the claim most likely to flip the decision.
- Reuse (repository landscape): discover repos and packages; separate fit from risk; verify finalists via source, tests, releases, issues, license; report comparison, integration path, gaps. Popularity proves nothing.

## Prove

Ladder: `candidate → exact evidence → claim-specific corroboration → verification → verdict`.

| Finding | Minimum corroboration |
|---|---|
| cycle | graph `cycles` + exact imports; type-only vs runtime |
| affected scope | graph `impact`/`dependents`/`path` + exact reads + LSP references/callers |
| security sink | sink shape + exact read + sources/callers + guard check |
| test gap | changed symbol + no test references + nearby test read |
| performance | exact hot path; benchmark only when runtime proof matters |

- Keep syntax, semantic, history, artifact, and runtime proof separate; list checks not run.

## Root cause

[Hypothesis test](../SKILL.md#rules-that-always-apply); a suspicious line, commit, or correlation stays a candidate until all four hold:

1. Expected vs actual, and the contract behind it.
2. Reproduce (or equivalent runtime evidence); first divergent boundary.
3. Rivals, each with its most distinguishing check: source, references, history, tests.
4. Changing the cause changes the symptom; if it cannot run, state equivalent evidence and limits.

Report per [output](../output.md#response).

## Read the smallest span

`localFetch` and `ghGetFileContent` share selectors and pagination (GitHub adds `owner`, `repo`, `ref`). Output is `<line>\t<text>`; non-adjacent windows are separated by one unnumbered `--` line; cite by line number.

- Quote or edit only verbatim text: reads without `minify` (raw, the default) or `matchString` reads; `minify:"standard"` drops comments and indentation. History text and patches are always exact.
- Known literal: `matchString` (a list is a grep map); `contextLines` default 3 (max 100); wider: `ranges`.
- Several regions, one call: `"ranges":["95-105","255-265"]`.
- Whole declaration: `block:true` on a match or range; check the span closes it, else use `astSearch symbols` `line`/`endLine`. Never guess an end.
- `minify:"symbols"`: doc headings with lines (read a heading to the next equal-or-higher one) or code signatures (read exact source before explaining behavior). Underlined headings or `minifyFallback`: exact search; a missing entry is not absence.
- Broad unread source where only some passages matter: [context selection](clasify.md#select-inside-search-and-fetch) returns selected raw windows; omit minify. A known deciding span is usually an ordinary exact read.
- Unanchored reads page by byte budget: copy `next.continueWindow`. `fullContent:true` is for small files; past its limit it returns page one with `pagination.hasMore` and `next.continueWindow`.

## Campaigns

- Loop: `frame one question → cheapest call that can change the answer → observe status → learn → next call`.
- Start: set corpus, question, mode, surfaces, and a Stop condition (done when the class's Definition of Done holds; escalate when the budget is spent or a prerequisite is missing); check reach ([octocode](octocode.md)). Measure claims resolved, not calls.
- Ledger: goal, anchors (paths, lines, ids, refs, `next.*`/`hints`; never invented), open hypothesis tests (rivals, cheapest distinguishing check), tried shapes.
- Direct research may pursue independent directions sequentially. When an orchestrator selects a multi-actor workflow, it assigns only directions with distinct observations, keeps one ledger and the merged verdict, and gives each actor an authorized Task contract: objective (purpose), constraints, output (`claim, evidence, verdict, confidence` per finding), and a Definition of Done. Actors pick the domain method and return evidence; they do not select more actors. Disagreement is evidence; the orchestrator re-checks each load-bearing anchor. See [workflow composition](../../octocode-workflows/references/composition.md).
- At decision boundaries and when stopping, report per [output](../output.md#response), naming verification and open gaps (unresolved alternatives, exhausted budgets, missing prerequisites).

## Handoff

Pick the protocol by what the receiver must do:

| Situation | Protocol | Slots |
|---|---|---|
| Blocked, or the next step needs someone else's evidence, decision, or action | SBAR | situation (question, scope/ref), background (checks run), assessment (evidence, confidence, gaps), request (next) |
| Unfinished work moves to another actor or session | Handoff packet | current state (question, scope/ref, checks run), evidence (anchors), blockers (gaps), next action, constraints, next-call ids (`next.*` calls copied unchanged) |

- Durable handoff (authorized, or [output](../output.md#saved-report) says save): the saved report; no evidence records.
- Machine consumer: save `<report-stem>.evidence.jsonl` beside the report with the header's `status` and `confidence` values; summarize in chat and link it:

```json
{"id":"ev1","type":"exact-file","source":"local","locator":"src/example.ts:42","quoteOrFact":"X calls Y","quality":"primary"}
{"id":"cl1","claim":"A supports X","status":"Likely","confidence":"medium","supportingEvidenceIds":["ev1"],"counterEvidenceIds":[],"nextCheck":"read tests"}
```
