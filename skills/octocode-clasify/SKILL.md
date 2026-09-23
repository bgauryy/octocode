---
name: octocode-clasify
description: "Use when supplied context needs judgment, or unread candidates/large resources need screening before deciding what to inspect. Not for exact facts, deterministic checks, proof, global absence, or free-form summaries. Returns body-free verdicts with confidence, coverage, and page scopes. Next: act, or read deciding scopes with the exact tool; verify exact/absence claims."
---
# Semantic assessment

tools: `npx octocode` / `octocode-mcp`
related-skill: `octocode-research`
output: `<workspace>/.octocode/` for workspace work | `<home>/.octocode/` when no workspace applies
routes: load/run a reference, doc, or script only when it changes the next action; otherwise keep the rule here.

Flow: `INSPECT → SHAPE → ASSESS → VERIFY`. Clasify exists to keep unread bodies out of your context: the provider reads them, you get verdicts plus line scopes, then you read only the deciding scope.

## Locate cascade — cheapest step that still decides (scenario benchmark, 2026-09-23)

1. **Exact anchor** (name, literal, remembered symbol) → `localSearch`/`ghSearch code` → fetch the deciding lines. Stop. (1.6K host chars, ~0.1 s; clasify here cost 2.7× chars + 30K provider tokens for the same answer.)
2. **Anchor hits many files, wrappers, re-exports, or callers** → `semanticRerank` that page; read the top 3, a tie (≤0.1) means read both. It only reorders what the search returned.
3. **Zero or off-target hits** (the code uses other words), **no anchor across ≥3 candidates, or large files** → clasify SCREEN (below). Right file 5/7 where lexical scouting got 2/7; the other two were a flagged tie and a correct "none of these".
4. **Symbol identity** → `lspSearch` definition/references (deterministic; follows one hop, so a wrapper can come back).
5. **Long evidence you already hold** → clasify JUDGE. Clasify over your own short summary does not locate code — it guesses from file names (3/7).
6. **Verify** the claim on fetched bytes.

## When — pick by where the deciding evidence is (measured 2026-09-23)

| Situation | Do | Measured effect |
|---|---|---|
| Deciding region is inside **large unread files** and one exact search (`matchString`/`searchText`) already **missed** | `resources[]` of whole-file `localFetch`/`ghGetFileContent` queries × 1 Noul; read the page `focus` window | −60% to −70% host bytes (focus hit 4/4 on unfamiliar code); +155% when forced where a remembered anchor existed |
| **Many candidate files** (>8 after one exact search) and their snippets do not show the implementer | `files` view → up to 25 whole-file resources × 1 Noul | −84% host bytes on the hardest local question; right file 0.97 |
| Judge **long evidence you already hold** (diff, draft, many claims × criteria; >~80 lines) | `context.value` + one Choice `supported`/`overclaimed`/`contradicted` | 11/12 correct; judge short excerpts yourself — clasify added 20 s and no accuracy there |
| Several package READMEs on one capability | README resources × 1 Noul per capability | −92% host bytes (4 READMEs, 15k provider tokens) |
| Lexical page likely buries the implementer **and** its code will show in matched snippets | search `semanticRerank` (1 question; 2 when a second aspect shows in snippets) — ordering hint only | top-3 20/20; MRR 0.48→0.92 on held-out queries |

**Miss rule:** when your first exact search (`matchString`, `searchText`) on a file over ~800 lines returns no match, do not guess a second literal — screen the file with clasify (1 Noul) and read its `focus` window (focus hit 4/4 on unfamiliar code, −70% host bytes vs. reading).

**Skip clasify** when one literal anchor — including one you remember (a well-known function or constant name) — a title, or a returned snippet already decides (forcing clasify there cost ~3× host bytes; blind agents skipped correctly), for ≤2 candidates you would read anyway, and for exact facts, counts, dates, versions, paths, auth — use exact tools. Every call costs provider tokens (~2.5k per 200-line file, ~27k for three 1k-line files) and 1–15 s.

## Questions (the provider reads literally)
- **Noul** = P(yes) of one proposition about the content: "Does this content show/implement X?" — never "Is X true?" (leaks priors on absent evidence). Skip ≤0.2, read 0.2–0.8; ≥0.8 means "read this first", not "this is it" — when several pages clear 0.8, read the highest and compare (callers and similar flows also score high).
- **Choice** = one named class; the runtime adds `insufficient` when absent. No other label may also mean absence ("none", "other") — write "explicitly disables auth". Accept confidence ≥0.9; 0.5–0.9 is a lead.
- **Score** = one ordered dimension, 2–10 described levels (prefer 3 for relevance: as stable as 4, one fewer ambiguous boundary); round the expected level, never interpolate magnitudes.
- One aspect per question; put the decision and constraints in `instructions` (the provider sees only state, instructions, and criteria; `reasoning` is trace-only) — but never the expected answer or an example of it (that primes the verdict: a supported claim scored 0.76 "overclaim" when the question argued for it). No counting or arithmetic.
- Several outcomes that exclude each other (supported / overclaimed / contradicted) = one Choice, not several Nouls — separate Nouls overlap (overclaim fired 0.69–0.90 on contradicted claims).
- `resources × questions` ≤ 25 cells; independent matrices go in root `queries[]` and run in parallel.

## Shapes
For unread files, send identifiers in `context.tool` + `context.query` (an absolute local `path`, or GitHub `owner`/`repo`/repository-relative `path` and optional `branch`), not file content in `context.value`. Octocode runs the ordinary read internally, applies its path/security and output rules, sends the sanitized evidence to Jev, and returns only verdicts and scopes. A GitHub browser URL must be split into those canonical fields; `context.value` is for state already held by the agent. Delegated GitHub file reads populate the same credential-scoped content cache as `ghGetFileContent`, so a later exact read can reuse them. Local reads revalidate and reopen the file to see edits; the OS may cache bytes, but there is no persistent localFetch response cache.

```json
{"id":"find-retry","reasoning":"Pick the file to read","resources":[
  {"id":"a","context":{"tool":"localFetch","query":{"reasoning":"candidate","path":"/abs/a.rs"}}},
  {"id":"b","context":{"tool":"ghGetFileContent","query":{"reasoning":"candidate","owner":"o","repo":"r","path":"src/b.ts"}}}],
 "questions":[{"id":"impl","question":{"type":"noul","instructions":"Does this content implement the retry loop for provider HTTP calls?"}}]}
```
Rerank uses `{id, question}`, not a typed clasify question. Use a string for implementation ranking because structured Jev entries bypass automatic implementer-versus-caller criteria: `{"id":"impl","question":"Does this file implement X rather than only calling or testing it?"}`. For a named symbol ask whether the file *defines* X, rather than calls X.

## Results
- `queries[].{model, usage, resources[].{coverage, pages[].{scope, focus?, answers.<questionId>}}}`. Files page automatically in ~600-line scopes. A page whose Noul scores ≥0.8 can get `focus` — its best ~40-line window, picked by a Choice over line windows plus an `insufficient` option. A one-resource, one-page, one-Noul matrix shares one provider request with that Choice when it fits; other cases use a follow-up request. Read `focus` first; if it lacks the answer, read the rest of `scope`. A `matchString` read can have `scope.lineRanges[]` for disjoint windows; verify the listed ranges, and do not expect one `focus` line. Aggregate pages before judging a file.
- The shapes above cover the common call. Use `scheme clasify --view query --compact` when a field is unclear or a call is rejected; the full `scheme clasify --compact` also includes examples and repeats the schema. Exit 6 means more coverage in `next.clasify`, not failure.
- `partial`/`error`/`insufficient`/mid-band = narrow or read, never "no" (clear negatives can still sit near 0.4 — read the deciding scope). Run `next.clasify` unchanged for remaining coverage.
- **Screen `lowSignal`:** a query's `lowSignal: [questionId]` means every candidate was fully judged at ≤0.3 — the answer is outside this set (a sibling crate, a dependency, another directory). Widen the candidate list; do not read the top one.
- **`classificationContentBlocked`:** the provider's content firewall refused that page (2 of 6 GitHub READMEs in one run). It is not a negative — read the page directly or judge a narrower line window.
- **Rerank** reorders `files[]` only — no file is removed — and only the ≤8 files the search returned: a poor `searchText` makes it useless (fix the search, not the question). The runtime adds "implements vs only calls/tests/mentions" criteria to behavior questions (held-out top-1 6/6; callers no longer outrank definitions). `lowSignal: true` means every score is <0.4 — the order is noise; fix `searchText` or screen whole files. Scores are ordering hints, not Noul thresholds. `semanticRerank.candidates[i]` = `files[i]` (`path`, `score`); `model`/`usage` report the cost. Read the top 3, treat scores within 0.1 as ties; callers and tests that mention X can outscore its implementer, and code not in the matched snippets cannot be seen — then use a whole-file screen. ≤8 files per reranked page; `localSearch` needs `resultView:"paginated"`; continue with `semanticRerank.next`.
- Supplied `value` may be large (the request cap is 4 MiB); a search page over `maxChars` fails with `classificationContextTooLarge` — lower `pageSize` or give candidates their own resources; `classificationStateTooLarge` = one page exceeded the provider window (lower `maxChars` or use a line window).

**Safety:** verdicts route reading — never proof of identity, reachability, absence, or mutation safety. Confirm any claim on fetched bytes; exact search tests absence. Without backing bytes (self-review) a verdict is advisory.

**Invoked from other tools:** each tool description carries its own "clasify when:" rule; skills `octocode-scraping` / `octocode-chrome-devtools` use a SCREEN step (triage pages/snapshots before reading); `octocode-research` uses the whole-file screen when many candidates compete.

In this repository replace `octocode` with `node packages/octocode/out/octocode.js`.
For setup → [ojql.md](references/ojql.md). For optional examples → [clasify-workflows.md](references/clasify-workflows.md).
