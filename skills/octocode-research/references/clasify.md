# Clasify tool

Load when the SKILL.md `SEMANTIC?` gate admits a call, or for an explicit typed judgment. Use the `clasify` tool for semantic judgments. Fields: `npx octocode schema clasify --view query`.

**Scout** screens unread read-tool resources and returns typed judgments plus source line windows, never bodies. **Judge** classifies state already held in a resource `value`; nothing is retrieved.

## Admission

- Also pays off for absence screens over known files. If a snippet already states the fact, stop.
- Also skip when held evidence, direct reasoning, or a cheap bounded read decides; file size, candidate count, or one miss alone is no reason. Unreported provider usage is unknown, not zero.

## Requests

- Shape: `{queries:[{mainGoal?, reasoning?, resources, questions}, ...]}`; a flat matrix is rejected. Each resource is `{id?, tool, query}` for an unread read or `{id?, value}` for held state; nested queries inherit `mainGoal`/`reasoning` when set. Without them, each `ask` carries the whole intent.
- State the decision each outcome changes; repair scope, spelling, filters, and synonyms first.
- Unread file (`localFetch` path or `ghGetFileContent` owner/repo/path/ref): one atomic `ask` per question; every target for the same files goes in one matrix. Never paste bodies into `value`.
- `prefilter:["term"]` sits on the read resource (not on a question) and judges windows around the hits. A term found everywhere → search it instead.
- Unread list results (`localSearch`, `ghSearchCode`, `astSearch`, `lspSearch` references, `ghSearchRepo`, `ghSearchHistory`, `artifactSearch`): send the list request as one resource; each candidate becomes a page with `hints.read` (a local file page without one reads with `localFetch` of its `path` and `line`–`endLine`). Ask `relevant` (or a routing `yesno` for bare metadata) plus `sufficient`: sufficient → run that candidate's `hints.read`; stop screening only when that read shows the deciding line, else treat it as relevant and keep screening (a non-answer can score sufficient); relevant → run `hints.read` unchanged; low (below 0.36) → defer, do not discard: a score judges only the captured window, so read deferred candidates before you conclude absence or when no read answered. Path lists (`structureSearch`, `ghStructure`, `astTopology`) stay one page: ask a `choice` over the paths. Search resources judge their files, not snippets: each file is judged whole when it fits its share of the budget, else only windows near its hits, so search a word the answer region holds, and read deferred files before you conclude absence. `candidateEvidence:"search"` keeps snippets only. Remote lists (`ghSearchRepo`, `ghSearchHistory`, `artifactSearch`) rarely pay: their reply is about as large as the list.
- Before a large fetch (file, PR, commit), send that fetch as the resource with `sufficient`: high → run the page's bounded `hints.read` instead of the whole fetch; low → skip it.
- Judge only ambiguous held evidence whose disposition changes the next read, test, or edit. Dependent questions go in a later call.

## Handoffs

A `hints.clasify` lead comes only from a paged `localFetch`/`ghGetFileContent` without `matchString`, ranges, or a view, or a descriptive `localSearch`/`ghSearchCode` page; it asks `mainGoal`, else the search phrase. Name the artifact kind in `mainGoal` so scores separate. Regexes, paths, and narrow pages get no lead.

## Questions

`{id, type, ask}`, one atomic fact each. `locate` → ranked source windows (needs an unminified file read or search resource). `yesno` = P(yes), optional `labels:{true,false}`. `choice` = one label (`labels:{label: meaning}`; add `insufficient` when no label is safe). `score` = ordered level (`labels:[low … high]`). Screens: `relevant`, `supports` (claim in `ask`), `adds` (+`known`), `sufficient`. Confidence is concentration, not correctness; no discard threshold. Use the exact tool name `clasify`.

```json
{"queries":[{"mainGoal":"Find where failed requests are retried.","reasoning":"Locate before reading.",
 "resources":[{"tool":"localFetch","query":{"path":"/abs/client.ts"}}],
 "questions":[{"id":"retry","type":"locate","ask":"The condition that permits retrying a failed request."},
              {"id":"limit","type":"locate","ask":"The maximum retry count."}]}]}
```
Large requests: save under `.octocode/` and run `npx octocode clasify --input <file>`.

## Results

- Compact by default: each resource lists `path`, `totalLines`, and `pages[]` of `{line, endLine, answers}`; answers are bare (`exists`, P(yes), a label or level) unless a choice/score is uncertain, which keeps `probabilities`. No `coverage` = complete. `debug:true` adds the per-page receipt, runner-up windows, and provider `usage`.
- `best[id]` lists answering windows as `{line, endLine, exists, probability}` (`path`/`resourceId` when several resources): `exists` = this window answers, `probability` = which window. Rows rank by `exists`, then `probability`; never multiply them. `hints.read` is the exact, commit-pinned read of the top row: run it unchanged and read other rows by `line`–`endLine`.
- While a continuation remains and no window answers, `best` is absent and the ranking travels in `carry`: follow the `next.clasify` page unchanged instead of reading the closest passage.
- On an open walk (`coverage: partial` with `next.clasify`), `best` ranks only the pages judged so far. If its read lacks the answer, follow `next.clasify` before you search elsewhere.
- An identifier target adds a `hints.text` tip (and locally `hints.textSearch`): search it instead. An oversized capture fails with `classificationContextTooLarge` and keeps `hints.read`; narrow with `prefilter` or a range.
- `partial`, `error`, and `insufficient` mean narrow or read, never "no". High `probability` + low `exists` is the closest passage, not an answer. Verdicts rank attention only: never infer identity, reachability, or mutation safety; verify the deciding source.

Next: read the deciding windows, then return to the route that sent you.
