# Clasify

Load when the SKILL.md `SEMANTIC?` gate admits a call, or for an explicit typed judgment. `clasify` is the only semantic tool (provider: Jev); it needs a configured classification key. Fields: `scheme clasify --view query --compact`.

**Scout** screens unread read-tool resources and returns typed judgments plus source scopes, never bodies. **Judge** classifies state already held in a resource `value`; nothing is retrieved.

## Admission
- Measured wins: classifying an explicit list instead of reading every item, locating inside a large known file (`prefilter` literals when the answer contains one), and absence screens over known files.
- To locate behavior, guess one literal and search first: clasify cost 2.6× the bytes when a literal was guessable and 22× for a literal target. If a snippet already states the fact, stop.
- Skip when an exact check, held evidence, direct reasoning, or a cheap bounded read decides; file size, candidate count, or one miss alone is no reason. Unreported provider usage is unknown, not zero.

## Requests
- State the decision each outcome changes; repair scope, spelling, filters, and synonyms first.
- Unread file (`localFetch` path or `ghGetFileContent` owner/repo/path/ref; saved scrape text and browser snapshots too): one `{tool, query}` resource per file, one atomic `ask` per `type:"locate"` question, every target for the same files in one matrix (≤25 cells; expanded pages count). Never paste bodies into `value` or nest `queries[]`.
- Huge file + rare literal the answer contains: `prefilter:["term"]` (read resources only) judges up to 3 600-line windows around hits. A term found everywhere → search it instead.
- Unread list results (`localSearch`, `ghSearchCode`, `astSearch`, `lspSearch` references, `ghSearchRepo`, `ghSearchHistory`, `artifactSearch`): send the list request as one resource; each candidate becomes a page with `next.read`. Ask `relevant` (or a routing `yesno` for bare metadata) plus `sufficient`: sufficient → use it, no read; relevant → run `next.read` unchanged; low → skip. Path lists (`structureSearch`, `ghStructure`) stay one page: ask a `choice`. Snippet scores stay flat (0.16–0.38); prefer a literal or per-file resources.
- Before a large fetch (file, PR, commit), send that fetch as the resource with `sufficient` and read only when it is low.
- Judge only ambiguous held evidence whose disposition changes the next read, test, or edit. Dependent questions go in a later call; no automatic Scout → Judge chain.

## Questions
`{id, type, ask}`, one atomic fact each; `goal` and `reasoning` accompany every question. `locate` → ranked source windows (needs unminified read or search resources). `yesno` = P(yes), optional `labels:{true,false}`. `choice` = one label (`labels:{label: meaning}`; add `insufficient` when no label is safe). `score` = ordered level (`labels:[low … high]`). Screens: `relevant`, `supports` (claim in `ask`), `adds` (+`known`), `sufficient`. Confidence is concentration, not correctness; no discard threshold. Legacy `questionType`/`target` and nested `context` still validate; do not mix forms. No `jev`/`semanticAssess` aliases.

```json
{"goal":"Find where failed requests are retried.","reasoning":"Locate before reading.",
 "resources":[{"tool":"localFetch","query":{"path":"/abs/client.ts"}}],
 "questions":[{"id":"retry","type":"locate","ask":"The condition that permits retrying a failed request."},
              {"id":"limit","type":"locate","ask":"The maximum retry count."}]}
```
Large requests: save under `.octocode/` and run `octocode clasify --input <file>`.

## Results
- Each resource lists `path`, `totalLines`, and `pages[]` of `{lines, answers}`; choice/score below 0.9 keep `confidence` and `probabilities`. No `coverage` = complete; `debug:true` adds runner-up matches and provider `usage`.
- `best[id]` rows are `{lines, exists, p}`: `exists` = this page answers, `p` = which window. Rows rank by `exists`, then `p` for equal `exists`; never multiply them.
- Follow `next.clasify` unchanged while coverage remains and the top `exists` is below 0.5 (rank lives in `carry`); then read the top `best` windows together (≤5 ranges) via `next.read` or `localFetch` `startLine/endLine`.
- Mid-band (0.36–0.69), `partial`, `error`, and `insufficient` mean narrow or read, never "no". High `p` + low `exists` is the closest passage, not an answer. An identifier hint means `localSearch` is cheaper.
- Verdicts rank attention only: never infer identity, reachability, absence, or mutation safety. Measure leverage as final quality plus actual host tokens.

Next: read the deciding windows, then return to the route that sent you.
