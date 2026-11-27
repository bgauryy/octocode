---
name: octocode-context-audit
description: "Load this skill first when asked to measure configured agent context (instructions, skills, MCP schemas, duplicate installs, Claude transcript usage): its script measures sizes and use instead of guessing."
---

# Context audit

Run the [audit script](scripts/context-audit.mjs):

```bash
node skills-dev/octocode-context-audit/scripts/context-audit.mjs --workspace <repo> --no-probe
```

- `--workspace` defaults to the current directory. `--no-probe` starts no MCP server; keep it when a server may have startup side effects. Without it, each stdio server gets `initialize` and every `tools/list` page within 20 s, or stays unmeasured (a `high` finding).
- `--days N` (default 30) picks this workspace's Claude transcripts by mtime. Use `--all-projects` only for a cross-project audit the user asked for.
- Each run writes `context-audit.json` to a fresh `<workspace>/.octocode/octocode-context-audit/<date>-audit-*/`; `--out DIR` replaces it and can overwrite a baseline. `--json` also prints it, `--html` adds `context-audit.html`, `--render <context-audit.json>` renders a saved report without collecting.

## What it measures

Sizes are characters against `BUDGET` (`scripts/lib/audit-common.mjs`); tokens ≈ characters ÷ 4, not billed input.

| Surface | Read from | Flags |
|---|---|---|
| Instructions | Workspace `AGENTS.md`, `CLAUDE.md`, `CLAUDE.local.md`, `GEMINI.md`, `.cursorrules`, Copilot and `.cursor/rules` files; `~/.claude/CLAUDE.md`, `~/.codex/AGENTS.md`, project `MEMORY.md` | Files and always-loaded total over budget (nested files count only in their directory; nested repositories and `.claude/worktrees` skipped) |
| Skills | `~/.claude/skills`, `.claude/skills`, `.agents/skills` | Long descriptions, broken links, one name from several sources |
| MCP | `~/.claude.json` (user, project), `.mcp.json`, `~/.cursor/mcp.json` | Instructions, tool descriptions, schemas and server totals over budget |
| Usage | Claude Code transcripts | Uncalled servers, tools and skills; error rates; shell reads outnumbering research calls. Skipped for Cursor servers and `.agents` skills, and on malformed lines or read failures |

## Act on it

1. Start with `high` findings. Zero calls in a short window is no reason to delete: check routing with `octocode-agentic` first, and leave installation repairs to `octocode-skills`.
2. After a repair, rerun to a new `--out` and compare: `node skills-dev/octocode-context-audit/scripts/context-audit.mjs --compare <before.json> <after.json>` prints size deltas and findings resolved, new and still open.
3. Stop when each high finding fills a row below. Lead with `N findings (high H, medium M, low L)` and the report path, then a row per finding acted on and the decisions left to the user (deletions, install changes). Report sizes, counts and tool names; never quote prompt or transcript text.

| Finding (severity, area) | Size (~tokens) | Owner file | Repair or reason to keep | Before → after |
|---|---|---|---|---|

After changing a collector, budget or output mode, run `node --test skills-dev/octocode-context-audit/scripts/tests/*.test.mjs`.
