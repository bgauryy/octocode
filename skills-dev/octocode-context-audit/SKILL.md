---
name: octocode-context-audit
description: "Load this skill first when asked to measure configured agent context (instructions, skills, MCP schemas, duplicate installs, Claude transcript usage): its script measures sizes and use instead of guessing. Behavioral diagnosis: octocode-agentic."
---

# Context audit

Run the dependency-free [audit script](scripts/context-audit.mjs):

```bash
node skills-dev/octocode-context-audit/scripts/context-audit.mjs --workspace <repo> --no-probe
```

- `--no-probe` starts no MCP server; keep it when a configured server may have startup side effects. Without it, each stdio server gets `initialize` and every `tools/list` page within 20 s; a timeout, invalid page or repeated cursor leaves that server unmeasured (a `high` finding).
- `--days N` (default 30) selects this workspace's Claude transcripts by file mtime. Use `--all-projects` only for a cross-project audit the user asked for.
- Each run writes `context-audit.json` to a fresh `<workspace>/.octocode/octocode-context-audit/<date>-audit-*/`; `--out DIR` writes there instead and can overwrite a baseline. `--json` also prints the report, `--html` adds `context-audit.html`, and `--render <context-audit.json>` renders a saved report later without collecting.

## What it measures

Sizes are characters against `BUDGET` in `scripts/lib/audit-common.mjs`; tokens are characters ÷ 4, an estimate, not billed input.

| Surface | Read from | Flags |
|---|---|---|
| Instructions | `AGENTS.md`, `CLAUDE.md`, `CLAUDE.local.md`, `GEMINI.md`, `.cursorrules`, `copilot-instructions.md`, `.cursor/rules/*.mdc` in the workspace; `~/.claude/CLAUDE.md`, `~/.codex/AGENTS.md`, the project's `MEMORY.md` | Files and always-loaded total over budget. Nested files load only in their directory; nested repositories go to `nestedRepositoriesSkipped`; `.claude/worktrees` is skipped |
| Skills | `~/.claude/skills`, `.claude/skills`, `.agents/skills` | Long descriptions, broken links, one name installed from different sources |
| MCP | `~/.claude.json` (user and this project), `.mcp.json`, `~/.cursor/mcp.json` | Instructions, tool descriptions, schemas and server totals over budget |
| Usage | Claude Code transcripts | Servers, tools and skills never called; error rates; shell reads outnumbering research calls. Cursor servers and `.agents` skills are not judged, and malformed lines or read failures suppress these flags |

## Act on it

Start with `high` findings. Report sizes, counts and tool names; never quote prompt or transcript text. Zero calls in a short window is no reason to delete: check routing with `octocode-agentic` first, and leave installation repairs to `octocode-skills`. After a repair, rerun to a new `--out` and compare with `node skills-dev/octocode-context-audit/scripts/context-audit.mjs --compare <before>/context-audit.json <after>/context-audit.json`: each surface's size delta and the findings resolved and new. Done when each high finding has its measured size, owning file and a repair or a reason to keep it.

After changing a collector, budget or output mode, run `node --test skills-dev/octocode-context-audit/scripts/tests/*.test.mjs`.
