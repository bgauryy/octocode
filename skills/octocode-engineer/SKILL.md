---
name: octocode-engineer
description: "Use when investigating, implementing, reviewing, refactoring, or auditing code — local or remote — and the task needs code search, AST/LSP analysis, PR review, architecture assessment, dead-code sweep, binary inspection, or OQL graph research."
---

# Octocode Engineer

Use this skill to understand, review, or change code without guessing. This file is the **router**; detailed playbooks live in `references/`. Read the smallest set of references the scenario needs.

## 0. Transport default: CLI first

Default to the **Octocode CLI** quick commands (`search`, `pr`, `unzip`, `clone`, `cache fetch`). Use `search --search path` for file discovery, `search --tree` for structure, `search --content-view exact|compact|symbols` for reads, `search --pattern/--rule --lang <lang>` for structural code search, `search --op <semantic-op>` for LSP semantics, `search --target repositories` for repo discovery, `search --target packages` for package lookup, `search --target commits` for history, `search --target artifacts` for binary/archive inspection, and `search --target diff` for file diffs. If `octocode` is not installed, use `npx octocode <cmd>`. Use MCP tools only when the host provides them and the CLI is unavailable.

Hard rules:
- Prefer `--json` whenever another step depends on returned paths, refs, line numbers, or pagination.
- Read `octocode tools <name> --scheme` before every raw-tool call. Quick-command flags and raw-tool fields differ.
- Use `octocode search --scheme` / `search --explain` before relying on OQL for partial targets.
- For dead-code, reachability, or drift sweeps, start with `search target:"research"` as a broad candidate pass, then prove with LSP/AST/exact reads.
- Treat snippets as leads. Prove with `search --match-string --content-view exact`, AST, LSP, history, or tests.
- Follow returned `next.*`, pagination, char offsets, match/file pages. Never invent offsets or paths.
- Keep ≥2 plausible explanations alive for ambiguous bugs until evidence eliminates one.
- Reflect before final output: weakest claim, strongest counter, whether one cheap command changes the answer.

## 1. Reference routing

- **When doing OQL, `--repo` shortcut, surface selection, graph/reachability, `--explain`, or diagnostics**: read [`workflow.md`](./references/workflow.md); if the task is specifically dead-code / safe-delete / retained-by, also read [`workflow-graph.md`](./references/workflow-graph.md)
- **When tracing local code, symbols, AST/LSP, file reads, or pagination**: read [`research-local.md`](./references/research-local.md)
- **When inspecting archives, binaries, `.node`/`.wasm`, or unpacked archives**: read [`research-binary.md`](./references/research-binary.md)
- **When doing GitHub/npm research, cross-repo comparison, PRs, or commit history**: read [`research-external.md`](./references/research-external.md)
- **When you need exact CLI command names, flags, raw `tools`, or MCP fallback syntax**: read [`context-cli-mcp-commands.md`](./references/context-cli-mcp-commands.md)
- **When writing AST patterns or troubleshooting structural-search gotchas**: read [`context-ast-pattern-cookbook.md`](./references/context-ast-pattern-cookbook.md)
- **When a quality claim needs a metric number (dep-cruiser, knip, tsc, ruff, bandit)**: read [`measurement-tools.md`](./references/measurement-tools.md)
- **When doing an engineering research recipe (orientation, blast radius, dead export, refactor)**: read [`workflow-engineering-research.md`](./references/workflow-engineering-research.md)
- **When doing PR review, local diff review, staged changes, or file history**: read [`workflow-pr-local-review.md`](./references/workflow-pr-local-review.md)
- **When the PR or diff is large (>15 files) and needs parallel review lanes**: read [`workflow-review-parallel-strategy.md`](./references/workflow-review-parallel-strategy.md)
- **When validating or dismissing a specific finding before presenting it**: read [`workflow-validation-playbooks.md`](./references/workflow-validation-playbooks.md)
- **When running a quality signal or code-smell sweep**: read [`checklist-quality-signals.md`](./references/checklist-quality-signals.md)
- **When doing a PR or local diff review and need review domains**: read [`checklist-review-domains.md`](./references/checklist-review-domains.md)
- **When presenting investigation results, architecture findings, or verdicts**: read [`template-artifact-report.md`](./references/template-artifact-report.md)
- **When writing a PR or local changes review report**: read [`template-review-report.md`](./references/template-review-report.md)

## 2. Fast routing

- **Already on disk** → `research-local.md`
- **Remote repo/package** → `research-external.md`; clone when analysis spans >3 files or needs AST/LSP
- **Archive/binary** → `research-binary.md`; unpack before code research
- **PR or local diff review** → `workflow-pr-local-review.md` + review checklist + report template
- **Architecture/refactor/bug** → `workflow-engineering-research.md` + relevant research reference
- **Dead-code / reachability / safe-delete** → `workflow.md` (graph algorithm + OQL patterns)
- **Quality smell / security finding** → `checklist-quality-signals.md` → `workflow-validation-playbooks.md`
- **Exact CLI/raw-tool/MCP syntax** → `context-cli-mcp-commands.md`

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
When presenting investigation results or multi-finding reports, use [`template-artifact-report.md`](./references/template-artifact-report.md) — summary, flows, boundaries, quality findings, confidence, next step.
When writing a PR or local diff review, use [`template-review-report.md`](./references/template-review-report.md) — cap to ~5–7 key issues, concrete fixes.

## 5. Safety gates

Ask before continuing when a task would change a public contract, cross layers/packages, delete/rename shared things, affect many consumers, require an architectural tradeoff, or when evidence conflicts.
