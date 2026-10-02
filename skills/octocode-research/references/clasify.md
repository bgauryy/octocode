# Clasify

Load when the SKILL.md `SEMANTIC?` gate admits a call, or for an explicit typed judgment. `clasify` is the only semantic tool (provider: Jev); it needs a classification key (`OCTOCODE_CLASSIFICATION_API`). Fields: `scheme clasify --view query --compact`.

**Scout** screens unread read-tool resources and returns typed judgments plus source line windows, never bodies. **Judge** classifies state already held in a resource `value`; nothing is retrieved.

## Admission
- Pays off for: classifying an explicit list instead of reading every item, locating a described target inside a large known file (`prefilter` literals when the answer contains one), and absence screens over known files.
- To locate behavior, guess one literal and search first; a guessable literal is far cheaper than a classification call. If a snippet already states the fact, stop.
- Skip when an exact check, held evidence, direct reasoning, or a cheap bounded read decides; file size, candidate count, or one miss alone is no reason. Unreported provider usage is unknown, not zero.

## Requests
- Flat shape: `{goal, reasoning, resources, questions}` (or `queries[]` of such matrices). Each resource is `{id?, tool, query}` for an unread read or `{id?, value}` for held state; nested queries inherit `goal`/`reasoning`. The older nested `context`/`questionType`/`target` form is legacy input only.
- State the decision each outcome changes; repair scope, spelling, filters, and synonyms first.
- Unread file (`localFetch` path or `ghGetFileContent` owner/repo/path/branch; saved scrape text and browser snapshots too): one resource per file, one atomic `ask` per `type:"locate"` question, every target for the same files in one matrix (≤25 resource × question cells). Never paste bodies into `value`.
- Huge file + rare literal the answer contains: `prefilter:["term"]` on the read resource (not on a question) judges windows around the hits. A term found everywhere → search it instead.
- Unread list results (`localSearch`, `ghSearchCode`, `astSearch`, `lspSearch` references, `ghSearchRepo`, `ghSearchHistory`, `artifactSearch`): send the list request as one resource; each candidate becomes a page with `next.read`. Ask `relevant` (or a routing `yesno` for bare metadata) plus `sufficient`: sufficient → run that candidate's `next.read` for the deciding fact and stop screening (the provider saw the evidence, you did not); relevant → run `next.read` unchanged; low → skip. Path lists (`structureSearch`, `ghStructure`, `astTopology`) stay one page: ask a `choice` over the paths. Snippet scores are flat; prefer a literal or per-file resources.
- Before a large fetch (file, PR, commit), send that fetch as the resource with `sufficient`: high → run the page's bounded `next.read` instead of the whole fetch; low → skip it.
- Judge only ambiguous held evidence whose disposition changes the next read, test, or edit. Dependent questions go in a later call; no automatic Scout → Judge chain.

## Handoffs
A paged `localFetch`/`ghGetFileContent` of a large file without `matchString`, ranges, or a view, and a wide descriptive `localSearch`/`ghSearchCode` page, may carry `next.clasify` built from the goal. Run it unchanged; name the artifact kind in the goal so scores separate. Identifiers, literals, regexes, paths, and narrow pages get no handoff: search them.

## Questions
`{id, type, ask}`, one atomic fact each. `locate` → ranked source windows (needs an unminified file read or search resource). `yesno` = P(yes), optional `labels:{true,false}`. `choice` = one label (`labels:{label: meaning}`; add `insufficient` when no label is safe). `score` = ordered level (`labels:[low … high]`). Screens: `relevant`, `supports` (claim in `ask`), `adds` (+`known`), `sufficient`. Confidence is concentration, not correctness; no discard threshold. No `jev`/`semanticAssess` aliases.

```json
{"goal":"Find where failed requests are retried.","reasoning":"Locate before reading.",
 "resources":[{"tool":"localFetch","query":{"path":"/abs/client.ts"}}],
 "questions":[{"id":"retry","type":"locate","ask":"The condition that permits retrying a failed request."},
              {"id":"limit","type":"locate","ask":"The maximum retry count."}]}
```
Large requests: save under `.octocode/` and run `octocode clasify --input <file>`.

## Results
- Compact by default: each resource lists `path`, `totalLines`, and `pages[]` of `{lines, answers}`; answers are bare (`exists`, P(yes), a label or level) unless a choice/score is uncertain, which keeps `probabilities`. No `coverage` = complete. `debug:true` adds the per-page receipt, runner-up windows, and provider `usage`.
- `best[id]` lists answering windows as `{lines, exists, p}` (`path`/`r` when several resources): `exists` = this window answers, `p` = which window. Rows rank by `exists`, then `p`; never multiply them. `next.read` is the exact, commit-pinned read of the top row: run it unchanged and read other rows by `lines`.
- While a continuation remains and no window answers, `best` is absent and the ranking travels in `carry`: follow `next.clasify` unchanged instead of reading the closest passage.
- An identifier target adds a hint (and locally `next.localSearch`): search it instead. An oversized capture fails with `classificationContextTooLarge` and keeps `next.read`; narrow with `prefilter` or a range.
- `partial`, `error`, and `insufficient` mean narrow or read, never "no". High `p` + low `exists` is the closest passage, not an answer. Verdicts rank attention only: never infer identity, reachability, absence, or mutation safety; verify the deciding source.

Next: read the deciding windows, then return to the route that sent you.
