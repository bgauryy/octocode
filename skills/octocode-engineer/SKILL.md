---
name: octocode-engineer
description: "Use when investigating, implementing, reviewing, refactoring, or auditing code — local or remote — and the task needs code search, AST/LSP analysis, PR review, architecture assessment, dead-code sweep, binary inspection, or OQL graph research."
---

# Octocode Engineer

Use this skill to understand, review, or change code without guessing. This file is the **router**; detailed playbooks live in `references/`. Read the smallest set of references the scenario needs.

## 0. Transport default: CLI first

Default to the **Octocode CLI** quick commands (`ls`, `find`, `grep`, `cat`, `lsp`, `pr`, `history`, `repo`, `pkg`, `binary`, `unzip`, `clone`, `cache fetch`, `search`). If `octocode` is not installed, use `npx octocode <cmd>`. Use MCP tools only when the host provides them and the CLI is unavailable.

Hard rules:
- Prefer `--json` whenever another step depends on returned paths, refs, line numbers, or pagination.
- Read `octocode tools <name> --scheme` before every raw-tool call. Quick-command flags and raw-tool fields differ.
- Use `octocode search --scheme` / `search --explain` before relying on OQL for partial targets.
- For dead-code, reachability, or drift sweeps, start with `search target:"research"` as a broad candidate pass, then prove with LSP/AST/exact reads.
- Treat snippets as leads. Prove with `cat --match-string --mode none`, AST, LSP, history, or tests.
- Follow returned `next.*`, pagination, char offsets, match/file pages. Never invent offsets or paths.
- Keep ≥2 plausible explanations alive for ambiguous bugs until evidence eliminates one.
- Reflect before final output: weakest claim, strongest counter, whether one cheap command changes the answer.

## 1. Reference routing

- **When doing OQL, `--repo` shortcut, surface selection, graph/reachability, `--explain`, or diagnostics**: read [`workflow.md`](./references/workflow.md); if the task is specifically dead-code / safe-delete / retained-by, also read [`workflow-graph.md`](./references/workflow-graph.md)
- **When tracing local code, symbols, AST/LSP, file reads, or pagination**: read [`research_local.md`](./references/research_local.md)
- **When inspecting archives, binaries, `.node`/`.wasm`, or unpacked archives**: read [`research_binary.md`](./references/research_binary.md)
- **When doing GitHub/npm research, cross-repo comparison, PRs, or commit history**: read [`research_external.md`](./references/research_external.md)
- **When you need exact CLI command names, flags, raw `tools`, or MCP fallback syntax**: read [`context_cli_mcp_commands.md`](./references/context_cli_mcp_commands.md)
- **When writing AST patterns or troubleshooting structural-search gotchas**: read [`context_ast_pattern_cookbook.md`](./references/context_ast_pattern_cookbook.md)
- **When a quality claim needs a metric number (dep-cruiser, knip, tsc, ruff, bandit)**: read [`context_external_measurement_tools.md`](./references/context_external_measurement_tools.md)
- **When doing an engineering research recipe (orientation, blast radius, dead export, refactor)**: read [`workflow_engineering_research.md`](./references/workflow_engineering_research.md)
- **When doing PR review, local diff review, staged changes, or file history**: read [`workflow_pr_local_review.md`](./references/workflow_pr_local_review.md)
- **When the PR or diff is large (>15 files) and needs parallel review lanes**: read [`workflow_review_parallel_strategy.md`](./references/workflow_review_parallel_strategy.md)
- **When validating or dismissing a specific finding before presenting it**: read [`workflow_validation_playbooks.md`](./references/workflow_validation_playbooks.md)
- **When running a quality signal or code-smell sweep**: read [`checklist_quality_signals.md`](./references/checklist_quality_signals.md)
- **When doing a PR or local diff review and need review domains**: read [`checklist_review_domains.md`](./references/checklist_review_domains.md)
- **When presenting investigation results, architecture findings, or verdicts**: read [`template_artifact_report.md`](./references/template_artifact_report.md)
- **When writing a PR or local changes review report**: read [`template_review_report.md`](./references/template_review_report.md)

## 2. Fast routing

- **Already on disk** → `research_local.md`
- **Remote repo/package** → `research_external.md`; clone when analysis spans >3 files or needs AST/LSP
- **Archive/binary** → `research_binary.md`; unpack before code research
- **PR or local diff review** → `workflow_pr_local_review.md` + review checklist + report template
- **Architecture/refactor/bug** → `workflow_engineering_research.md` + relevant research reference
- **Dead-code / reachability / safe-delete** → `workflow.md` (graph algorithm + OQL patterns)
- **Quality smell / security finding** → `checklist_quality_signals.md` → `workflow_validation_playbooks.md`
- **Exact CLI/raw-tool/MCP syntax** → `context_cli_mcp_commands.md`

## 3. Operating loop

1. State goal and scope in one line.
2. Read the matching reference.
3. Write a compact hypothesis map: likely explanation, alternate, and what would disconfirm each.
4. Map before reading — structure/file discovery first, then exact slices.
5. Use AST for code shape; use LSP for symbol identity and blast radius.
6. After each observation, update confidence and choose the next cheapest proof step.
7. Mark confidence: `confirmed`, `likely`, or `uncertain`.
8. Stop and ask when scope, contract, blast radius, or safest fix requires a user decision.

## 4. Output

Quick tasks: finding, evidence, next step.
When presenting investigation results or multi-finding reports, use [`template_artifact_report.md`](./references/template_artifact_report.md) — summary, flows, boundaries, quality findings, confidence, next step.
When writing a PR or local diff review, use [`template_review_report.md`](./references/template_review_report.md) — cap to ~5–7 key issues, concrete fixes.

## 5. Safety gates

Ask before continuing when a task would change a public contract, cross layers/packages, delete/rename shared things, affect many consumers, require an architectural tradeoff, or when evidence conflicts.
