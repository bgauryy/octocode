---
name: octocode-architecture-view
description: "Use when someone wants to see or explain a system's architecture visually: layers, modules, dependencies, runtime flows, data stores, or external services as an interactive HTML map (layers, force graph, sequence flows, dependency matrix, 3D). Triggers: architecture diagram, visualize the codebase, system map, how modules connect, onboarding overview. Not for deciding or refactoring a design → octocode-architect."
---

# Octocode Architecture View

tools: `npx octocode` / `octocode-mcp`
related-skill: `octocode-architect`
output: `<workspace>/.octocode/` for workspace work | `<home>/.octocode/` when no workspace applies
routes: load/run a reference, doc, script, or scheme only when it changes the next action; otherwise keep the rule here.

Turn a repository into one self-contained, interactive HTML architecture map that a developer can read in minutes and verify line by line. Artifacts (`scan.json`, `model.json`, `architecture.html`) go to `<output>/architecture-view/`.

Flow: `SCAN → MODEL → RENDER → VERIFY → OPEN`.

## Rules

- When extracting structure, let the scan do the mechanics (components, import edges, signals, stores, externals); why: it is deterministic and costs no context. You write only the semantic overlay: layers, roles, runtime edges, flows, findings, and the explanation.
- Every claim in the overlay carries evidence (`path` + `line`) read from exact source. Scan heuristics stay marked `guess` until you confirm them; the viewer shows them with `?`.
- Separate declared architecture (docs, manifests, ownership rules), observed structure (imports), and runtime behavior (spawn, FFI, HTTP, DB, MCP). Import edges alone never prove a runtime flow.
- Prefer fewer, truer nodes: fold noise with `mergeInto`, drop fixtures with `exclude`, keep about 15–60 components on screen. The matrix view covers larger graphs.
- A derived cycle or upward edge is a candidate. Confirm the closing edge before you call it a defect, or mark a legal callback `allowed`.
- Never edit the template per project. All variation comes from the model, so the output stays one predictable file.

## Workflow

1. **SCAN** — run `node scripts/scan.mjs <root> [--exclude <prefixes>] [--octocode]`. Read the stdout summary: component rows, kinds and layers as guesses, heaviest edges, detected stores and hosts. `--octocode` adds `astTopology` runtime-cycle evidence when `OCTOCODE_BETA` is available. Rerun with `--exclude` when fixtures, vendored code, or benchmark corpora dominate.
2. **MODEL** — read declared architecture first (README, ARCHITECTURE/PACKAGES docs, manifests), then trace with `octocode-research`: entrypoints, registration sites, process/FFI/network/DB boundaries, and 3–6 representative flows. Load `references/modeling.md` for the layer taxonomy, evidence recipes per edge kind, and flow tracing. Write `<output>/architecture-view/model.json` against `scheme/architecture-model.json`: overlay only, keyed by scan ids.
3. **RENDER** — run `node scripts/render.mjs --scan <output>/architecture-view/scan.json --model <output>/architecture-view/model.json --open`. It merges, derives fan-in/out, instability, cycles, and upward edges, validates references, and writes `architecture.html` next to the scan from `assets/template.html`.
4. **VERIFY** — treat every render warning (missing node, unknown layer, missing evidence path) as a model bug. Fix it and rerun. Check that each flow reads end-to-end in the Flows view and that no confirmed node still shows `?`. Load `references/visual-design.md` when the map looks cluttered or a view misleads.
5. **OPEN** — `--open` launches the default browser. Report the path, the layer story in 3–5 lines, the traced flows, and findings with their confidence. Say what stayed heuristic.

## Gates

- Ask before you scan outside the workspace or add network enrichment. The views load D3 and 3d-force-graph from jsdelivr, so say so when the user needs an offline artifact.
- For review-only or decision requests, hand findings to `octocode-architect`. This skill maps; it does not refactor.
- Keep outputs in `<output>/architecture-view/`. Do not write into source directories or commit.
