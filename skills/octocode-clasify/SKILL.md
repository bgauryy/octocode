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
- Use it for an explicit classification request, or before reading a large known file when the target is semantic (no useful literal) and locating it avoids broad host reads. Literals and known anchors: search directly.
- Skip it when an exact check, held evidence, direct reasoning, or a cheap bounded read decides. File size, candidate count, or one search miss alone is not a reason.
- Optimize host context at acceptable quality; measure provider usage and latency separately. Unreported usage is unknown, not zero.

## Workflow
1. State the unresolved decision and what each outcome changes.
2. Repair scope, spelling, filters, and synonyms first; narrow roots/include/owner/repo on large repositories. A miss is not absence.
3. Unread known file (`localFetch` path, or `ghGetFileContent` owner/repo/path/ref; scrape text and Chrome snapshots too): one resource per file, `questionType:"locate"`, one atomic `target` per question; put every target for the same files in one matrix (≤25 cells, expanded pages count).
4. Huge files: add `prefilter:["term", …]` (literal words the answer likely contains) to the file resource: clasify captures only the 3 densest 600-line hit windows, in one call. Without it, whole-file locate pages through the file.
5. Unread `localSearch`/GitHub code-search results: send the search request as one resource; default judges paths/snippets/metadata, `candidateEvidence:"fileChunks"` only when snippets cannot route the next read.
6. Judge only when held evidence stays ambiguous and its disposition changes the next read, test, or edit; send the smallest sufficient observations.
7. Read the deciding source (below). Do not chain Scout → Judge automatically. Dependent questions go in a later call.

## Questions
One atomic fact per question; decision and constraints go in `instructions` (`reasoning` is trace only). `noul` = P(yes), not intensity. `choice` = one label plus its distribution (add `insufficient` when no label may be safe). `score` = expected level. Confidence is concentration, not correctness; there is no discard threshold. Research presets `locate`/`contribution`/`addsEvidence`/`supportsClaim` take only `target` (+`knownEvidence`); do not mix them with `type`/`instructions`/`criteria`.

## Input
Unread: `context:{tool, query}` with an absolute local path or GitHub owner/repo/path/branch; never paste bodies into `context.value`, never nest `queries[]`. Held: `context:{value}`. Save the request under `.octocode/` and run `octocode clasify --input .octocode/clasify-request.json` (or MCP `clasify`):
```json
{"reasoning":"Locate facts before reading.","resources":[{"context":{"tool":"localFetch","query":{"reasoning":"unread","path":"/abs/source","fullContent":true}}}],
 "questions":[{"id":"retry","questionType":"locate","target":"The condition that permits retrying a failed request."},
              {"id":"limit","questionType":"locate","target":"The maximum permitted retry count."}]}
```

## Results and verification
- Locate answer: `{exists, matches:[{startLine,endLine,probability}]}`. `exists` = does this page answer; `probability` = which declaration/passage inside it. Windows surround ranked passages; doc-comment hits may show the declaration head. Two matches = near-tie: read both. Expand or follow the source if the deciding statement is absent, even with a high score.
- Read the query's `best[questionId]` first: windows across pages ranked by `exists`, then `probability`. Across calls rank the same way; **never multiply them**. A `hints` entry for an identifier target means localSearch is exact and cheaper.
- Read the top windows together (≤5 ranges) with `localFetch` at `source.path` + `startLine/endLine` (GitHub: resource owner/repo/path, `branch` = returned `source.ref`; `source.path` includes owner/repo). High window + low `exists` = closest passage, not an answer.
- Follow `next.clasify` unchanged while coverage remains (exit 6); it carries the running `best`, so the final call's `best` is file-wide. `partial`, `error`, `insufficient`, and mid-band results mean narrow or read, never "no". Partial coverage never proves absence.
- Search resources: one page per returned file with its own `source.path`; `fileChunks` pages may expose `next.read`. Clasify applies no hidden threshold.
- Never infer identity, reachability, absence, or mutation safety from a verdict. Measure leverage as final quality plus actual host tokens.

No `jev`/`jevScout`/`jevReasoning`/`semanticAssess` aliases.
