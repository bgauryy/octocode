---
name: octocode-context-audit
description: "Load this skill first when asked to measure configured agent context (instructions, skills, MCP schemas, duplicate installs, Claude transcript usage): its script reports sizes without guessing. Behavioral diagnosis: octocode-agentic."
---

# Context audit

## Run

Capture a baseline with the dependency-free [audit script](scripts/context-audit.mjs).

```bash
node <skill>/scripts/context-audit.mjs --workspace <repo> --no-probe
```

Keep `--no-probe` when a configured server may have startup side effects. Omit it to measure stdio servers via `initialize` and every `tools/list` page. A deadline, invalid page or repeated cursor leaves that server unmeasured. Keep stderr private.

Use `--days N` to select Claude transcripts by file mtime. The default scope is this workspace. Use `--all-projects` only for a cross-project audit the user asked for; report counts and tool names, never prompt or transcript text. Each run saves `context-audit.json` under a fresh dated directory in `<workspace>/.octocode/octocode-context-audit/`. An explicit `--out DIR` can overwrite a baseline. `--json` also prints the full report.

## What it collects

| Surface | Read the evidence as |
|---|---|
| Instructions, rules and memory index | Separate always-loaded files from nested rules. Nested repos and agent worktrees are listed separately. |
| User and workspace skills | Look for long descriptions, broken links and a name installed from different sources in Claude or workspace agent directories. |
| Claude user/project and Cursor MCP configurations | Compare instructions, tool descriptions and schemas with the report’s character budgets. The estimate of four characters per token cannot establish actual model input or billing. |
| Claude Code transcripts | Check tool and skill calls, error rates and shell reads against research calls. Judge only Claude-loaded surfaces. With missing sessions or malformed lines, report the calls actually observed. Absence and window-wide error rates remain unknown. |

## Act on the report

Start with `high` findings. Read each flag against its measured scope and budget. Move reference detail out of always-loaded files, keep per-turn rules, and retain one canonical skill installation. Check routing and schemas with `octocode-agentic` before treating zero calls in a short window as grounds for deletion. Use `octocode-skills` for structural or installation repairs.

If repairs are authorized, change the owning source, rerun to a fresh destination and compare the saved evidence and retained behavior. Follow [output.md](output.md) for the BLUF, each high finding in 5W2H, the before/after delta and HTML delivery. Use `--html` when requested up front or `--render <saved-context-audit.json>` to render the accepted snapshot later.

When changing a collector, budget or output mode, run `node --test <skill>/scripts/tests/*.test.mjs`.

Done when each high finding has a measured size, owning file and a repair or reason to keep it, and each repair has a rerun delta.
