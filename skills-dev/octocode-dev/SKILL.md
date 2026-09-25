---
name: octocode-dev
description: "Use when auditing, hardening, or cleaning up an Octocode tool end to end inside the octocode monorepo: core input schema, description, and MCP/CLI instructions; schema↔Rust implementation alignment; data flow from input through provider/API to output; efficiency, algorithms, and caching; output shape (pagination, truncation, rigid or redundant fields, regex hacks); agent workflow hints and next.* routing; config support across CLI/MCP/code; docs drift; repo cleanup. Triggers include check each tool, tool audit, octocode dev, schema misalignment, redundant input, output redundancy, pagination gap, tool data flow. Not for using the tools to research other code → octocode-research; not for a single known bug fix."
---

# Octocode Dev

tools: `npx octocode` / `octocode-mcp`
related-skill: `octocode-research`
output: `<workspace>/.octocode/` for workspace work | `<home>/.octocode/` when no workspace applies
routes: load/run a reference, doc, or script only when it changes the next action; otherwise keep the rule here.

Audit and improve Octocode's own tools, one tool at a time, across every layer from the core contract to the rendered output — then fix what is proven and delete what is redundant.

Flow: `SCOPE → MAP → CONTRACT → IMPL → OUTPUT → WORKFLOW → CONFIG+DOCS → FIX → VERIFY`. SCOPE picks the tools and whether this run is audit-only or audit+fix. MAP runs the inventory. Each audit lane records findings; FIX applies only verified ones; VERIFY runs the real CLI and MCP path.

Reports: `<output>/octocode-dev/<date>-<tool|all>.md` from `assets/audit-report.md`; scratch: `<output>/tmp/octocode-dev/`. Source edits keep their real paths.

## Lobby rules
- Read and follow the repo `AGENTS.md` first. **Never `git commit`, never `git stash`** — other sessions and a checkpoint bot share the tree; compare baselines with `git show <rev>:<path>`.
- Dogfood: inspect with `node packages/octocode/out/octocode.js` (`$OCTO`) and the Octocode MCP tools before raw grep. Every friction you hit is itself a finding for the tool under audit.
- Know the owner before editing. Public names, schemas, descriptions, and instructions are authored in core (`../octocode-mcp-host/packages/octocode-core`); interfaces never hand-write tool guidance; native owns execution and output shaping. A fix in the wrong layer is drift, not a fix — map with `references/surface-map.md`.
- Evidence per finding: file:line + a reproducing `$OCTO` call or test. Inventory hits, field-effect labels, and descriptions are claims until code confirms them. Separate "confirmed" from "candidate".
- Pre-existing failures are not yours: baseline tests/lint before editing and attribute every failure (concurrent sessions often refactor native/core in parallel).
- One tool per lane. For `all`, audit the shared runtime (envelope, response, continuations, cache, security) once, then each tool; parallelize independent tool lanes with `octocode-subagent` only when the scope justifies it.
- Fix scope: remove before adding. Prefer deleting a redundant field, branch, alias, or doc paragraph over layering a new one. No compatibility shims unless the user asks. Never lower coverage floors.
- Stop and ask before: breaking a public schema field, changing defaults users rely on, editing the sibling core repo when the user scoped the run to this repo, or history/git surgery.

## Lane map — load the reference for the lane in play

| Lane | Question | Reference |
|------|----------|-----------|
| MAP | Where does each layer of this tool live? what fields exist and where are they read? | `references/surface-map.md` + `scripts/tool-inventory.mjs` |
| CONTRACT | Is the schema, description, and instruction text accurate, minimal, and decidable for an agent? | `references/contract-audit.md` |
| IMPL | Is every input consumed as documented? Is the data flow input→prepare→provider/API→result lean, correct, cached? | `references/implementation-audit.md` |
| OUTPUT | Is the output complete, paginated, untruncated, non-redundant, not rigid? | `references/output-audit.md` |
| WORKFLOW | Will an agent chain this tool smartly (next.*, hints, diagnostics, reasoning)? | `references/workflow-audit.md` |
| CONFIG+DOCS | Is every config knob supported in code, CLI, and MCP, and are docs true? | `references/config-docs-audit.md` |
| FIX+VERIFY | How do I land a change across core→native→CLI/MCP and prove it? | `references/fix-and-verify.md` |

## Smart routes — load only what the current step needs
- At MAP, run `node .agents/skills/octocode-dev/scripts/tool-inventory.mjs [tool ...]` (add `--json` to keep in scratch) — prints per tool: native module, evidence files, variants, zero-hit input fields (candidate unused/misnamed), fields missing from `field-effect-coverage.json`, and undescribed fields. Then load `references/surface-map.md` for the layer paths.
- When judging schema/description/instruction wording, load `references/contract-audit.md`; for rewriting MCP or CLI instruction text, use `octocode-prompt-optimizer` — it owns context budget, decidable boundaries, and runtime alignment.
- When tracing a field or call through the runtime, load `references/implementation-audit.md`; use `lspSearch` callers/references to prove reachability rather than grep counts.
- When evaluating returned payloads or `next.*`, load `references/output-audit.md`; execute continuations, never trust `hasMore` alone.
- When judging agent chaining, load `references/workflow-audit.md`.
- When touching config or docs, load `references/config-docs-audit.md`.
- Before any edit and before reporting done, load `references/fix-and-verify.md` — it holds the regen order and the stale-server gotchas.
- For the shared acceptance bar (per-tool matrix, minification matrix, pagination rules), read `docs/MCP_TOOL_QUALITY_AND_AGENT_WORKFLOW.md`; for response field semantics, `docs/TOOL_DATA_CONTRACT.md`. Cite them — do not restate them in reports.

## Related routes
- `octocode-research` to prove callers, reachability, and upstream behavior with evidence.
- `octocode-prompt-optimizer` for description and instruction rewrites (CONTRACT, WORKFLOW).
- `octocode-clean-agentic-code` for the cleanup pass: dead exports, shims, stale tests/docs, residue.
- `octocode-eval-benchmark` when a change claims faster, fewer tokens, or better routing — measure before/after.
- `rust-best-practices` for Rust idiom, allocation, subprocess, and ReDoS questions inside IMPL.
- `octocode-roast` when the user wants a ranked blunt critique instead of fixes.

## Done gate
- Every audited tool has a lane-by-lane row in the report: confirmed findings with evidence, candidates, fixed, deferred-with-reason.
- When any fix landed, it passed the `references/fix-and-verify.md` gate through the real CLI **and** MCP path, or the report says which surface was not exercised and why.
- No new redundancy: the diff removed at least as much duplicated guidance/code as it added, or the report justifies the addition.
