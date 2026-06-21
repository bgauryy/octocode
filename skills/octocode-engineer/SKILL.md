---
name: octocode-engineer
description: "Use for codebase understanding, implementation, bug investigation, refactor planning, PR/local diff review, architecture review, blast-radius analysis, and RFC/design validation. A lean CLI-first router that selects the right Octocode references and evidence path: octocode commands, schema-first raw tools, local/GitHub/npm research, binary/archive inspection, AST, LSP, history, review workflows, quality checks, and artifact templates."
---

# Octocode Engineer

Use this skill to understand, review, or change code without guessing. The skill itself is the **router**; detailed playbooks live in `references/`. First identify the scenario, then read the smallest set of references that covers the evidence you need. Some tasks require several references — combine them deliberately instead of over-reading.

## 0. Transport default: CLI first

Default to the **Octocode CLI** quick commands (`ls`, `find`, `grep`, `cat`, `lsp`, `pr`, `history`, `repo`, `pkg`, `binary`, `unzip`, `clone`, `cache fetch`) because they are easy to validate with `--help`, support `--json`/`--compact`, and dogfood the same runners agents use through MCP. If `octocode` is not installed or not on `PATH`, run the CLI with `npx octocode <command>`; do not fall back to native search just because the global binary is missing. Use raw `octocode tools <name> --scheme` → `octocode tools <name> --queries '<json>'` when a quick command cannot express an exact field, pagination lane, content selector, or OQL gap. Use MCP tool calls only when the host provides them and the CLI is unavailable or the task explicitly needs MCP transport.

Hard rules:
- Prefer `--json` whenever another step depends on returned paths, refs, line numbers, diagnostics, or pagination.
- Read `octocode tools <name> --scheme` before every raw-tool call. Quick-command flags and raw-tool fields are not the same API.
- Use `octocode search --scheme` / `octocode search --explain` before relying on OQL routing for partial targets.
- For dead-code, reachability, unused-file, or dependency-drift sweeps, start with `octocode search` `target:"research"` as a broad candidate pass, then prove destructive edits with LSP references, AST import search, exact reads.
- Treat snippets as leads. Prove claims with exact `cat --match-string --mode none`, line ranges, selected PR patches, AST structural matches, LSP output, binary metadata, or tests.
- Follow returned hints, `next.*`, pagination, char offsets, match pages, file pages, comment pages, and commit pages. Do not invent offsets or local paths.
- Direct shell is allowed for `git status`, `git diff`, branch/log inspection, and repo maintenance around Octocode itself; use Octocode CLI/MCP for code research.

Decision-quality rules:
- Before deep research, name 1–3 working hypotheses and the evidence that would disconfirm each one. Keep them short; they guide tool choice, not the final answer.
- Interleave reasoning and observation: after every meaningful tool result, update scope, confidence, and the next evidence path instead of continuing the original plan blindly.
- Keep at least two plausible explanations or fix paths alive for ambiguous bugs, reviews, and refactors until evidence eliminates one.
- Before final output, run a reflection check: weakest claim, strongest alternate explanation, missing validation, and whether one cheap command could change the answer.

## 1. Pick the reference set first

Start with the primary reference, then add companions only when the scenario needs them.

| Task / question | Read |
|---|---|
| Local code research, implementation tracing, symbol lookup, AST/LSP, `matchString`, minify, pagination | [`references/research_local.md`](./references/research_local.md) |
| Archives, compressed files, `.node`/`.wasm`/native binaries, unzip/unpack then inspect | [`references/research_binary.md`](./references/research_binary.md) |
| GitHub/npm research, cross-repo comparison, package source lookup, PR/commit history, clone handoff | [`references/research_external.md`](./references/research_external.md) |
| CLI command names, flags, raw `tools`, and MCP fallback map | [`references/context_cli_mcp_commands.md`](./references/context_cli_mcp_commands.md) |
| AST pattern examples and structural-search gotchas | [`references/context_ast_pattern_cookbook.md`](./references/context_ast_pattern_cookbook.md) |
| External metrics/checkers (`dep-cruiser`, `knip`, `type-coverage`, `eslint`, `ruff`, `mypy`) | [`references/context_external_measurement_tools.md`](./references/context_external_measurement_tools.md) |
| General engineering research recipes: orientation, blast radius, dead export, refactor, review | [`references/workflow_engineering_research.md`](./references/workflow_engineering_research.md) |
| PR review, local diff review, staged changes, safe-to-merge, history review | [`references/workflow_pr_local_review.md`](./references/workflow_pr_local_review.md) |
| Large PR / large local diff parallel review | [`references/workflow_review_parallel_strategy.md`](./references/workflow_review_parallel_strategy.md) |
| Validate or dismiss a finding | [`references/workflow_validation_playbooks.md`](./references/workflow_validation_playbooks.md) |
| Quality and review domain checklists | [`references/checklist_quality_signals.md`](./references/checklist_quality_signals.md), [`references/checklist_review_domains.md`](./references/checklist_review_domains.md) |
| Final artifact/report shape | [`references/template_artifact_report.md`](./references/template_artifact_report.md), [`references/template_review_report.md`](./references/template_review_report.md) |

## 2. Combine references by scenario

| Scenario | Reference set |
|---|---|
| Trace local behavior or implement a local change | `research_local.md` + `workflow_engineering_research.md`; add `template_artifact_report.md` for non-trivial output |
| Review local/staged changes | `workflow_pr_local_review.md` + `research_local.md` + `checklist_review_domains.md` + `template_review_report.md` |
| Review a remote PR | `workflow_pr_local_review.md` + `research_external.md` + `checklist_review_domains.md` + `template_review_report.md`; add `research_local.md` after clone |
| Compare external libraries/repos | `research_external.md`; add `research_local.md` after clone and `context_ast_pattern_cookbook.md` if comparing code shapes |
| Inspect archive/binary package contents | `research_binary.md`; after `unpack`, continue with `research_local.md` |
| Architecture/refactor risk | `workflow_engineering_research.md` + `research_local.md` + `checklist_quality_signals.md`; add `context_external_measurement_tools.md` only when a metric/graph number matters |
| Dead-code/package-drift audit | `workflow_engineering_research.md` + `research_local.md`; add `context_external_measurement_tools.md` for knip confirmation |
| Suspicious quality/security finding | `checklist_quality_signals.md` + `workflow_validation_playbooks.md`; add `context_ast_pattern_cookbook.md` for AST proof |
      | Need exact syntax for any command/tool | `context_cli_mcp_commands.md` plus the workflow/reference for the task |

## 3. Fast routing

- **Already on disk** → start `research_local.md`.
- **Remote repo/package** → start `research_external.md`; clone and switch to local when analysis spans several files or needs AST/LSP.
- **Archive/binary** → start `research_binary.md`; unpack before local code research.
- **PR or local diff review** → start `workflow_pr_local_review.md` and add review checklist/template.
- **Architecture/refactor/bug investigation** → start `workflow_engineering_research.md`, then add the relevant research reference.
- **Quality smell or suspected issue** → start checklist, then validation playbook.
      - **Exact CLI/raw-tool/MCP syntax** → use `context_cli_mcp_commands.md`.

## 4. Minimal operating loop

1. State the goal and current scope in one line.
2. Read the matching reference above.
3. Write a compact hypothesis map: likely explanation, alternate explanation, and what evidence would disconfirm each.
4. Map before reading: structure/file discovery first, then exact slices.
5. Use AST for code shape; use LSP for symbol identity and blast radius.
6. Treat snippets as leads. Re-anchor with `matchString`, line ranges, AST, LSP, or history before citing.
7. After each tool observation, update confidence and choose the next cheapest proof step.
8. Mark confidence: `confirmed`, `likely`, or `uncertain`.
9. Reflect before reporting: weakest claim, strongest counter-evidence, missing validation, and whether one more cheap command would change the answer.
10. Stop and ask when the scope, contract, blast radius, or safest fix needs a user decision.

## 5. Evidence shortcuts

| Need | Evidence path |
|---|---|
      | Unknown tree | `octocode ls` → `octocode find` |
      | Unknown file | `octocode cat --mode symbols` |
      | Exact quote/line | `octocode cat --match-string ... --mode none` |
      | Code shape | `octocode grep <path> --pattern/--rule` or raw `localSearchCode(mode:"structural")` |
      | Definition/usages/call flow | `octocode lsp` or raw `lspGetSemantics` with a real lineHint |
      | Dead-code/package drift | `octocode search --query '{"target":"research",...}' --json`, then LSP/AST/knip proof |
      | Remote proof | `octocode cat <owner/repo/path> --match-string ... --mode none`; clone/cache for AST/LSP |
      | Why it changed | `octocode history` → `octocode pr` selected content |
      | Metric/cycle/coverage number | external tool reference, then ask before running |

## 6. Output

For quick tasks: answer with the finding, evidence, and next step.

For non-trivial work: use `template_artifact_report.md` and include only relevant sections: summary, flows, data/contracts, boundaries, quality findings, execution risks, confidence, and next step.

For reviews: use `template_review_report.md`, cap findings to the few that matter, dedupe existing PR comments, and include concrete fixes.

## 7. Safety gates

Ask before continuing when a task would change a public contract, cross layers/packages, delete/rename shared things, affect many consumers, require an architectural tradeoff, or when evidence conflicts.
