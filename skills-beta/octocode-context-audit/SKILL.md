---
name: octocode-context-audit
description: "Use when agent sessions feel bloated, slow, or confused, or before adding instructions, skills, or MCP servers: measures what the harness loads every session (AGENTS.md/CLAUDE.md, memory index, skill descriptions, MCP instructions and tool schemas), probes configured MCP servers live, mines Claude Code transcripts for tool/skill usage and error rates, and flags oversize, broken, duplicate, unused, or bypassed surfaces. Not for rewriting prompt text → octocode-prompt-optimizer."
---
# Context audit

tools: `node` (zero dependencies)
related-skill: `octocode-prompt-optimizer`, `octocode-skills`
output: `<workspace>/.octocode/context-audit/` (`context-audit.json`, `context-audit.html`)

Every session pays for its always-loaded context before the first task token. This skill measures that cost, compares it with what agents actually use, and ranks what to cut.

## Run
```bash
node <skill>/scripts/context-audit.mjs --workspace <repo> [--days 30] [--all-projects] [--no-probe] [--out DIR] [--json]
```
- `--days` bounds the transcript window (by file mtime); `--all-projects` mines every Claude Code project, not only this workspace.
- `--no-probe` skips spawning stdio MCP servers (use when a server has side effects on start).
- Open `context-audit.html` for the ranked preview; feed `context-audit.json` to an agent for follow-up.

## What it collects
| Area | Source | Flags |
|---|---|---|
| Instructions | root and nested `AGENTS.md`/`CLAUDE.md`/`GEMINI.md`/rules, `~/.claude/CLAUDE.md`, the project memory index | file > 8K chars, always-loaded total > 20K, memory index > 10K; nested repos and agent worktrees are listed, not counted |
| Skills | `~/.claude/skills`, `.claude/skills`, `.agents/skills` | broken links, one name from different sources, description > 1,024 chars, descriptions total > 12K |
| MCP | `~/.claude.json` (user + project), `.mcp.json`, `~/.cursor/mcp.json`; stdio servers answer `initialize` + `tools/list` | server that fails to start (with its stderr), instructions > 4K, tool schema > 8K, server total > 60K |
| Usage | `~/.claude/projects/<workspace>/*.jsonl` | Claude servers or tools never called, skills never invoked, tools with ≥20% error rate, raw grep/cat/find shell calls outnumbering research-tool calls |

Budgets are review points (≈ 4 chars per token), not failures. Cursor/other-client servers are measured but not judged by Claude transcripts.

## Act on the report
1. Fix `high` first: a server that cannot start, always-loaded instructions far over budget, or a server nobody calls.
2. Move reference detail out of always-loaded files into linked docs or skills; keep rules the agent must follow every turn.
3. Remove broken skill links and duplicate installs; keep one canonical source.
4. For unused or error-prone tools, check routing text and schemas with `octocode-prompt-optimizer` before deleting — zero calls in a short window is a candidate, not proof.
5. Re-run after changes and compare `context-audit.json` totals.
