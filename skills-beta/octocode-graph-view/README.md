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
- OCR: Ollama with `ollama pull glm-ocr` behind the FreeLlama sidecar (`npx @octocodeai/freellama-mcp-server` starts `freellama serve` on `127.0.0.1:11435`; `FREELLAMA_SERVE_ENDPOINT` overrides). OCR calls are managed vision tasks in one affinity session per run; without a sidecar the raw host (`OLLAMA_HOST`, default `localhost:11434`) is used with no admission or session management. PNG crops and alpha flattening are built in (`sips` or ImageMagick only for other image formats).
- Clasify enabled in the Octocode configuration.
- `node scripts/preflight.mjs` prints which layers run here and how to enable each missing one.

## Limits

Verdicts triage leads; they do not prove defects. Function calls come from syntax-derived linkage and show only the hub files. LSP is opt-in because a cold server can take minutes. Tests: `node --test scripts/graph-view.test.mjs`.

`glm-ocr` keeps generating after the visible text ends (7 of 12 report strips ran to a 1,500-token cap), so reads are capped: 400 tokens per strip and about 6 tokens per expected label for the whole shot, in a 4K context. `check-report.mjs` reads each tab's panel in six strips, two in flight, and reports each tab's wall time (`ms`) and its slowest strip (`slowestStripMs`). Measured on an M2 Pro: the whole check takes 54 s (it was 2:50 on raw Ollama with a 1,500-token cap and 1:42 on the sidecar before the caps) and the whole-shot read 1.8 s, with the same labels found. Edges from files edited after the graph was built are reported stale; rebuild to retest them.
