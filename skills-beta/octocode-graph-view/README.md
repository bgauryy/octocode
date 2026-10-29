# octocode-graph-view

Interactive HTML graph of a repo section (files, functions, imports, calls, layers, flows), ranked insights, and a small brief an agent reads instead of source. Graph findings are tested by independent evidence layers (AST, usage counts, clasify, image crops, DOM layout, local OCR, git history, optional LSP). Start with [SKILL.md](SKILL.md).

```text
scripts/chunk.mjs   big repo ─▶ plan.json, chunks/*, overview/data.json   (one graph snapshot has a file budget)
scripts/build.mjs   section ─▶ data.json     octocode graph: files, functions, imports, calls, insights
agent (optional)    evidence ─▶ model.json   layers, runtime edges, flows, findings (scheme/model.json)
scripts/render.mjs  data (+model, +verify) ─▶ report.html, brief.txt (about 600 tokens), brief-full.txt, shot-*.png, measure-*.json
scripts/verify.mjs  leads ─▶ verify.json     AST-confirmed connections, usage counts, clasify questions, image crops, OCR, optional LSP, verdicts
scripts/check-report.mjs  finished page ─▶ report-check.json   screenshot and OCR round trip
```

The page has Files and Functions views, search, zoom, and tabs for Insights, Evidence, Model, Tokens and Node.

## Setup

- The local launcher `node packages/octocode/out/octocode.js` (or `OCTOCODE_CLI`, else `octocode` on PATH); Chrome (`CHROME_PATH` overrides discovery); `rsync` for the frozen-copy fallback.
- Ollama running with `ollama pull glm-ocr`; PNG crops and alpha flattening are built in (`sips` or ImageMagick only for other image formats); `OLLAMA_HOST` overrides `localhost:11434`.
- Clasify enabled in the Octocode configuration.
- `node scripts/preflight.mjs` prints which layers run here and how to enable each missing one.

## Limits

Verdicts triage leads; they do not prove defects. Function calls come from syntax-derived linkage and show only the hub files. LSP is opt-in because a cold server can take minutes. Tests: `node --test scripts/graph-view.test.mjs`.

OCR of dense text is slow: `check-report.mjs` reads each tab's panel in six strips, about 2 minutes per tab measured on an M2 Pro.
