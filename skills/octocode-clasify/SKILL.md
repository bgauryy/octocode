---
name: octocode-clasify
description: "Use when an explicit classification request needs a typed judgment, or when a semantic answer should be located inside an unread known file before the host reads it. Locate returns small source verification windows (two on a near-tie) plus P(answer), ranked across pages in best; use direct search for literals. Batch independent same-evidence questions in one matrix. Supports unread Scout resources, saved scrape/browser artifacts, and supplied-state judgments; not proof, missing facts, or summaries."
---
# Clasify

tools: `npx octocode` / `octocode-mcp`
related-skill: `octocode-research`
output: `<workspace>/.octocode/` for workspace work | `<home>/.octocode/` when no workspace applies
routes: no supporting files; use the live `scheme clasify --view query --compact` contract for fields.

Clasify is the only semantic tool (Jev is its provider). **Scout** screens unread read-tool resources and returns typed judgments plus source scopes, never bodies. **Judge** classifies state already held in a resource's `value`; nothing is retrieved.

## Admission
- Use it for three measured wins: classifying an explicit list instead of reading every item, locating inside a large known file (with `prefilter` literals when the answer contains one), and absence screens over known files. To locate behavior, guess one literal and search it first: clasify cost 2.6× the bytes when a literal was guessable and 22× for a literal target. Literals and known anchors: search directly. If a search snippet already states the fact, stop.
- Skip it when an exact check, held evidence, direct reasoning, or a cheap bounded read decides. File size, candidate count, or one search miss alone is not a reason.
- Optimize host context at acceptable quality; measure provider usage and latency separately. Unreported usage is unknown, not zero.

## Workflow
1. State the unresolved decision and what each outcome changes.
2. Repair scope, spelling, filters, and synonyms first; narrow roots/include/owner/repo on large repositories. A miss is not absence.
3. Unread known file (`localFetch` path, or `ghGetFileContent` owner/repo/path/ref; scrape text and Chrome snapshots too): one resource per file (`{tool, query:{path}}`; the whole file is read), `type:"locate"`, one atomic `ask` per question; put every target for the same files in one matrix (≤25 cells, expanded pages count).
4. Huge files and a rare literal the answer contains: add `prefilter:["term", …]` to a `localFetch`/`ghGetFileContent` resource (other resources reject it). Clasify judges up to 3 600-line windows around the hits and continues any rest through `next.clasify`. If the term occurs throughout the file, search that literal instead. Without a literal, whole-file locate pages through `next.clasify`.
5. Unread list results (`localSearch`, `ghSearchCode`, `astSearch` match/symbols, `lspSearch` references, `ghSearchRepo`, `ghSearchHistory`, `artifactSearch` discovery): send the list request as one resource. Each candidate becomes its own page with `path` (or `source.item`) and `next.read`. Ask relevance plus `sufficient` in the same matrix (`relevant` for content-bearing items; a routing `yesno` "likely implements X" for bare metadata such as repo names), then per page: sufficient → use the captured evidence, no read; relevant but insufficient → run `next.read` unchanged; low → skip. Bare path lists (`structureSearch`, `ghStructure`) stay one page: ask a `choice` over the paths. Before a large fetch (file, PR, commit), send that fetch as the resource with `sufficient` first and read only when it is low. Snippet screening scores stay flat (measured 0.16–0.38), so they rarely separate candidates: guess a literal, or classify the files themselves (a `locate` question makes a `localSearch`/`ghSearchCode` resource read file chunks, or use one read resource per file). `locate` needs contiguous original lines: unminified `localFetch`/`ghGetFileContent` or a search resource with a locate question; other resources take typed questions only.
6. Judge only when held evidence stays ambiguous and its disposition changes the next read, test, or edit; send the smallest sufficient observations.
7. Read the deciding source (below). Do not chain Scout → Judge automatically. Dependent questions go in a later call.

## Questions
One atomic fact per question, as `{type, ask}`. Required `goal` is sent with every question and in each page's evidence state; required `reasoning` is sent in that same state. Decision and constraints that belong to one question go in `ask`. `yesno` = P(yes), not intensity (optional `labels:{true,false}`). `choice` = one label (`labels:{label: meaning}`; add `insufficient` when no label may be safe). `score` = expected level (`labels:[low … high]`). Confidence is concentration, not correctness; there is no discard threshold. `sufficient` = P(the captured evidence already states the target answer, so no read is needed). `relevant`/`supports`/`adds` (+`known`) screen evidence. The older `questionType`/`target` and `type`/`instructions`/`criteria` forms and nested `context` resources still validate; do not mix forms inside one question.

## Input
Unread: `{tool, query}` with an absolute local path or GitHub owner/repo/path/branch (the read inherits goal/reasoning); never paste bodies into `value`, never nest `queries[]`. Held: `{value}`. Save the request under `.octocode/` and run `octocode clasify --input .octocode/clasify-request.json` (or MCP `clasify`):
```json
{"goal":"Searching for retry handling. Need files that decide whether a failed request is retried.","reasoning":"Locate facts before reading.","resources":[{"tool":"localFetch","query":{"path":"/abs/source"}}],
 "questions":[{"id":"retry","type":"locate","ask":"The condition that permits retrying a failed request."},
              {"id":"limit","type":"locate","ask":"The maximum permitted retry count."}]}
```

## Results and verification
- Each `resources[]` row states its file `path` and `totalLines` once, then `pages[]` as `{lines:[start,end], answers}`; `answers` maps every question id to a bare verdict: locate `exists`, yesno/screen P(yes), a choice label, or a score level. A choice/score whose top probability is below 0.9 keeps `{choice|score, confidence, probabilities}`. A single supplied value has `answers` on the resource. No `coverage` means complete. `debug:true` returns full pages (runner-up `matches`, `source`, `scope`) plus provider `usage` (calls, tokens, ms).
- `best[questionId]` rows are `{lines, exists, p}` (`r`/`path` only when the matrix has several resources or files). `exists` = does that page answer; `p` = which window inside it. Rows whose `exists` lies within 0.05 tie and `p` orders them; **never multiply them**. `next.read` is the exact read of the top row. Expand or follow the source if the deciding statement is absent, even with a high score.
- Read `best` when its first `exists` is at least 0.5, or when no `next.clasify` remains. While the walk is open and the top `exists` is lower, `best` is omitted and the rank is only in `carry` — follow `next.clasify` before reading. A `hints` entry for an identifier target means localSearch is exact and cheaper.
- Read the top windows together (≤5 ranges) with `localFetch` at the resource `path` + `startLine/endLine` (GitHub: resource owner/repo/path at the read ref). High `p` + low `exists` = closest passage, not an answer. A requested line range without `coverage` judged that range.
- Follow `next.clasify` unchanged while coverage remains (exit 6). The final call's `best` is file-wide. `partial`, `error`, `insufficient`, and mid-band results (0.36–0.69, where the measured errors fell) mean narrow or read, never "no". Partial coverage never proves absence.
- Search resources: one page per returned file with its own `path`; candidate pages may expose `next.read`. Clasify applies no hidden threshold.
- An absence screen only ranks what to read: low `exists` on complete coverage lowers priority. Never infer identity, reachability, absence, or mutation safety from a verdict. Measure leverage as final quality plus actual host tokens.

No `jev`/`jevScout`/`jevReasoning`/`semanticAssess` aliases.
