---
name: octocode-clasify
description: "Use when an explicit classification request needs a typed judgment, or when a semantic answer should be located inside an unread known file before the host reads it. Locate returns small source verification windows (two on a near-tie) plus P(answer), ranked across pages in best; use direct search for literals. Batch independent same-evidence questions in one matrix. Supports unread Scout resources, saved scrape/browser artifacts, and supplied-state judgments; not proof, missing facts, or summaries."
---
# Clasify

tools: `npx octocode` / `octocode-mcp`
related-skill: `octocode-research`
output: `<workspace>/.octocode/` for workspace work | `<home>/.octocode/` when no workspace applies
routes: no supporting files; use the live `scheme clasify --view query --compact` contract for fields.

Clasify is the only semantic tool (Jev is its provider). **Scout** screens unread read-tool resources and returns typed judgments plus source scopes, never bodies. **Judge** classifies state already held in `context.value`; nothing is retrieved.

## Admission
- Use it for an explicit classification request, or before reading a large known file when the target is semantic (no useful literal) and locating it avoids broad host reads. Literals and known anchors: search directly. If a search snippet already states the fact, stop.
- Skip it when an exact check, held evidence, direct reasoning, or a cheap bounded read decides. File size, candidate count, or one search miss alone is not a reason.
- Optimize host context at acceptable quality; measure provider usage and latency separately. Unreported usage is unknown, not zero.

## Workflow
1. State the unresolved decision and what each outcome changes.
2. Repair scope, spelling, filters, and synonyms first; narrow roots/include/owner/repo on large repositories. A miss is not absence.
3. Unread known file (`localFetch` path, or `ghGetFileContent` owner/repo/path/ref; scrape text and Chrome snapshots too): one resource per file, `questionType:"locate"`, one atomic `target` per question; put every target for the same files in one matrix (≤25 cells, expanded pages count).
4. Huge files and a rare literal the answer contains: add `prefilter:["term", …]`. Clasify keeps the 3 densest 600-line windows. Terms that occur throughout the file cover it, so search that literal instead. Without a literal, whole-file locate pages through `next.clasify`.
5. Unread list results (`localSearch`, `ghSearchCode`, `astSearch` match/symbols, `lspSearch` references, `ghSearchRepo`, `ghSearchHistory`, `artifactSearch` discovery): send the list request as one resource. Each candidate becomes its own page with `source.path` or `source.item` and `next.read`. Ask relevance plus `sufficient` in the same matrix (`contribution` for content-bearing items; a routing `noul` "likely implements X" for bare metadata such as repo names), then per page: sufficient → use the captured evidence, no read; relevant but insufficient → run `next.read` unchanged; low → skip. Bare path lists (`structureSearch`, `ghStructure`) stay one page: ask a `choice` over the paths. Before a large fetch (file, PR, commit), send that fetch as the resource with `sufficient` first and read only when it is low. `candidateEvidence:"fileChunks"` only when snippets cannot route the next read. `locate` needs contiguous original lines: unminified `localFetch`/`ghGetFileContent` or `fileChunks`; other resources take typed questions only.
6. Judge only when held evidence stays ambiguous and its disposition changes the next read, test, or edit; send the smallest sufficient observations.
7. Read the deciding source (below). Do not chain Scout → Judge automatically. Dependent questions go in a later call.

## Questions
One atomic fact per question. Required `goal` is sent with every question and in each page's evidence state; required `reasoning` is sent in that same state. Decision and constraints that belong to one question go in `instructions`. `noul` = P(yes), not intensity. `choice` = one label plus its distribution (add `insufficient` when no label may be safe). `score` = expected level. Confidence is concentration, not correctness; there is no discard threshold. `sufficient` = P(the captured evidence already states the target answer, so no read is needed). Research presets `locate`/`contribution`/`sufficient`/`addsEvidence`/`supportsClaim` take only `target` (+`knownEvidence`); do not mix them with `type`/`instructions`/`criteria`.

## Input
Unread: `context:{tool, query}` with an absolute local path or GitHub owner/repo/path/branch; never paste bodies into `context.value`, never nest `queries[]`. Held: `context:{value}`. Save the request under `.octocode/` and run `octocode clasify --input .octocode/clasify-request.json` (or MCP `clasify`):
```json
{"goal":"Searching for retry handling. Need files that decide whether a failed request is retried.","reasoning":"Locate facts before reading.","resources":[{"context":{"tool":"localFetch","query":{"goal":"Find the retry decision.","reasoning":"unread","path":"/abs/source","fullContent":true}}}],
 "questions":[{"id":"retry","questionType":"locate","target":"The condition that permits retrying a failed request."},
              {"id":"limit","questionType":"locate","target":"The maximum permitted retry count."}]}
```

## Results and verification
- Each `resources[].pages[]` row is that resource's result: `source` plus `answers`, the score for every question (`noul`, choice, score, or locate `exists` and `matches`).
- Locate answer: `{exists, matches:[{startLine,endLine,probability}]}`. `exists` = does this page answer; `probability` = which declaration/passage inside it. Windows surround ranked passages; doc-comment hits may show the declaration head. Two matches = near-tie: read both. Expand or follow the source if the deciding statement is absent, even with a high score.
- Read `best[questionId]` when its first `exists` is at least 0.5, or when no `next.clasify` remains. Windows are ranked by `exists`, then `probability`; **never multiply them**. While the walk is open and the top `exists` is lower, `best` is omitted and the rank is only in `carry` — follow `next.clasify` before reading. A finished walk with no `best` has one window: read that page's `matches`. A `hints` entry for an identifier target means localSearch is exact and cheaper.
- Read the top windows together (≤5 ranges) with `localFetch` at `source.path` + `startLine/endLine` (GitHub: resource owner/repo/path, `branch` = returned `source.ref`; `source.path` includes owner/repo). High window + low `exists` = closest passage, not an answer. A requested line range with `coverage: complete` judged that range; `scope.totalLines` is the file length.
- Follow `next.clasify` unchanged while coverage remains (exit 6). The final call's `best` is file-wide. `partial`, `error`, `insufficient`, and mid-band results mean narrow or read, never "no". Partial coverage never proves absence.
- Search resources: one page per returned file with its own `source.path`; `fileChunks` pages may expose `next.read`. Clasify applies no hidden threshold.
- Never infer identity, reachability, absence, or mutation safety from a verdict. Measure leverage as final quality plus actual host tokens.

No `jev`/`jevScout`/`jevReasoning`/`semanticAssess` aliases.
