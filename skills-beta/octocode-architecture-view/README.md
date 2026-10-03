# octocode-architecture-view

Turns a repository into one interactive HTML architecture map that shows layers, modules, dependencies, runtime flows, data stores, and external services. Every edge and step links back to `file:line`.

## When to use

- Onboarding: "show me how this system fits together."
- Before a refactor, to see layer violations, cycles, and blast radius (then hand the findings to `octocode-architect`).
- Explaining a flow such as a request path, tool call, or DB write to reviewers.

## How it works

```text
scripts/scan.mjs   repo ─▶ scan.json      components, import edges, signals, stores, hosts (zero deps, <1s on ~1.6k files)
agent + octocode          ─▶ model.json   layers, roles, runtime edges, flows, findings, explanation (overlay, see scheme/)
scripts/render.mjs scan+model ─▶ architecture.html   merge, derive cycles/upward edges/instability, validate, open
```

The page has five views: **Layers** (swimlanes), **Graph** (force layout with file drill-down), **Flows** (sequence diagrams, also playable on the map), **Matrix** (DSM), and **3D** (layer cake). It also includes search, a details panel with evidence links that open the editor, light/dark theme, and shareable URL-hash state.

## Quick start

```bash
node scripts/scan.mjs . --octocode            # writes .octocode/architecture-view/scan.json
# write .octocode/architecture-view/model.json (overlay; see references/modeling.md)
node scripts/render.mjs --scan .octocode/architecture-view/scan.json \
  --model .octocode/architecture-view/model.json --open
```

A scan-only render (`--scan` without `--model`) works too. It shows heuristic roles marked `?`.

## Files

| File | Role |
|---|---|
| `SKILL.md` | Workflow and gates |
| `scheme/architecture-model.json` | Model/overlay contract |
| `scripts/scan.mjs` | Deterministic extraction (JS/TS, Rust, Python, Go imports; manifests; signals) |
| `scripts/render.mjs` | Merge, derive, validate, inline, open |
| `assets/template.html` | The single predefined viewer (opens a demo when raw) |
| `references/modeling.md` | Layer taxonomy, evidence recipes, flow tracing |
| `references/visual-design.md` | Why each view exists; decluttering |
