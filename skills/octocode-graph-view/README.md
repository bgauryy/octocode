# octocode-graph-view

Live code graph of a repository section: files, imports, functions and calls from `octocode graph`, cross-checked with LSP, AST outlines on demand. A lean local Node server (no dependencies) streams every page to a browser viewer over one WebSocket, so the graph grows as chunks are ingested and pages arrive. Start with [SKILL.md](SKILL.md).

```text
scripts/serve.mjs   section ─▶ http://127.0.0.1:<port>/?t=<token>   live viewer + same result as build
scripts/build.mjs   section ─▶ data.json, events.jsonl, brief.txt, brief-full.txt   (headless)
scripts/render.mjs  data.json (+ model.json) ─▶ briefs
```

Every graph node is checked against its file on disk (exists, lines in range, unchanged since the snapshot). The viewer shows phases and per-chunk progress, live hubs, cycles, ranked findings and graph-vs-LSP agreement, every octocode tool call with its page or cursor and timing (a live check of the local tools), and for each node its file location with editor links, imports with every line, functions with on-demand LSP callers/callees/references, and the AST outline.

## Setup

- The local launcher `node packages/octocode/out/octocode.js` (or `OCTOCODE_CLI`, else `octocode` on PATH). Run from the target workspace.
- Node 24+; `rsync` for the frozen-copy fallback; a browser. The page loads d3 7.9.0 from cdnjs.
- `node scripts/preflight.mjs` reports what runs here.

## Limits

Calls between chunks are bound by name until LSP confirms them; LSP probes the functions of hub files and of stitched calls (cold servers take 20 s to 2 min). Tests: `node --test scripts/*.test.mjs`.
