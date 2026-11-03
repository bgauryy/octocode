---
name: octocode-context-audit
description: "Audit bloated, slow or confusing agent context, or measure context cost before adding instructions, skills or MCP servers. Inventory configured inputs, probe availability when appropriate, and compare observed Claude transcript usage."
---

# Context audit

## Run

Capture a baseline with the dependency-free [audit script](scripts/context-audit.mjs).

```bash
node <skill>/scripts/context-audit.mjs --workspace <repo> --no-probe
```

Keep `--no-probe` when a configured server may have startup side effects. Omit it to measure stdio servers via `initialize` and every `tools/list` page. A deadline, invalid page or repeated cursor leaves that server unmeasured. Keep stderr private.

Use `--days N` to select Claude transcripts by file mtime. The default scope is this workspace. Use `--all-projects` only for a cross-project audit. Each run saves `context-audit.json` under a fresh dated directory in `<workspace>/.octocode/octocode-context-audit/`. An explicit `--out DIR` can overwrite a baseline. `--json` also prints the full report.

## What it collects

| Surface | Read the evidence as |
|---|---|
| Instructions, rules and memory index | Separate always-loaded files from nested rules. Nested repos and agent worktrees are listed separately. |
| User and workspace skills | Look for long descriptions, broken links and a name installed from different sources in Claude or workspace agent directories. |
| Claude user/project and Cursor MCP configurations | Compare instructions, tool descriptions and schemas with the report’s character budgets. The estimate of four characters per token cannot establish actual model input or billing. |
| Claude Code transcripts | Check tool and skill calls, error rates and shell reads against research calls. Judge only Claude-loaded surfaces. With missing sessions or malformed lines, report the calls actually observed. Absence and window-wide error rates remain unknown. |

## Act on the report

Start with `high` findings. Read each flag against its measured scope and budget. Move reference detail out of always-loaded files, keep per-turn rules, and retain one canonical skill installation. Check routing and schemas with `octocode-agentic-prompts` before treating zero calls in a short window as grounds for deletion. Use `octocode-skills` for structural or installation repairs.

If repairs are authorized, change the owning source, rerun to a fresh destination and compare the saved evidence and retained behavior. Follow [output.md](output.md) for the BLUF, each high finding in 5W2H, the before/after delta and HTML delivery. Use `--html` when requested up front or `--render <saved-context-audit.json>` to render the accepted snapshot later.

When changing a collector, budget or output mode, run `node --test <skill>/scripts/tests/*.test.mjs`.
