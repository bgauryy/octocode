---
name: octocode-code-graph
description: "Use when mapping dependencies, change impact, cycles, layering, dead code, reachability, or architecture risk."
---

# Octocode code graph

Translate repository file topology into ranked code hypotheses, then prove, or dismiss them with exact imports, AST shape, LSP identity, and checks.
Flow: `FRAME → TRIAGE → GRAPH → PROVE → RANK → REPORT`.

Workspace output contract: chat-only findings stay in chat. New saved reports default to `<workspace>/.octocode/octocode-code-graph/`; scratch evidence uses `<workspace>/.octocode/tmp/octocode-code-graph/`. User-approved source edits keep their named paths. Never fall back to a user-level Octocode home for artifacts.

## Rules

- Prefer exposed Octocode MCP tools. In this monorepo use `node packages/octocode/out/octocode.js`; elsewhere use `npx octocode`.
- Read `tools <name> --scheme --json --compact` immediately before raw calls; batch independent probes up to the reported limit.
- Orient cheaply. Use the graph only for dependencies, dependents, paths, cycles, reachability, or dead-code candidates—not symbol lookup.
- A graph edge is syntactic file evidence, not symbol identity. A graph smell is a hypothesis, not a defect.
- Never recommend deletion from reachability or `deadCode` alone. Never call a cycle harmful until exact runtime edges and an impact are shown. <!-- style-lint: ignore-line passive-voice -->
- Preserve warnings, truncation, inferred entrypoints, `includeTests`, exclusions, empty/error status, and unavailable LSP capabilities in the verdict.

## Workflow

1. **FRAME:** define the question, repository root, language, entrypoints, tests policy, exclusions, budget, and consequence being predicted. <!-- style-lint: ignore-line passive-voice -->
2. **TRIAGE:** load `references/graph-triage.md` when choosing the cheapest graph operation and turning its signal into hypotheses.
3. **GRAPH:** run one bounded graph probe. Follow `next.*` and pagination instead of rebuilding queries.
4. Load `references/false-positive-controls.md` when cycles, reachability, dead code, barrels, or verification might hide scope/runtime alternates.
5. Load `references/issue-catalog.md` when mapping surviving cycles, fan-in/out, paths, or unreachable nodes to code risks.
6. **PROVE:** load `references/proof-ladder.md` before any issue, blast-radius, layering, or deletion verdict; upgrade file topology with exact reads, structural search, and LSP.
7. **RANK:** run the smallest relevant test, build, typecheck, diagnostic, or exact negative check. The command exit status—not assertion summaries—controls success.
8. **REPORT:** load `references/output.md` when reporting findings, confidence, gaps, and next actions.

## Stop gates

Stop and report a candidate—not an issue—when the graph is partial, entrypoints are uncertain, an edge cannot be exact-read, AST/LSP cannot distinguish the alternate, or no observable impact exists. Ask before public-contract rewires, cross-package moves, or deletes/renames.

## Verification

For skill changes run `node scripts/eval-code-graph.mjs --self-test`, then `node scripts/eval-code-graph.mjs --json` to grade false-positive controls before review. Run `node skills/octocode-skills/scripts/skill-review.mjs skills/octocode-code-graph --json`, then smoke an unused repository question through graph → exact edge → AST/LSP → check; full eval score, zero review findings, and an evidence-complete smoke are the ship gate.

Research provenance lives in `references/references.md`; load it only when auditing why these rules exist.
