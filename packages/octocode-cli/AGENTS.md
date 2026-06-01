# Octocode CLI — Agent Guide

This document covers non-interactive usage, `--json` output shapes, tool execution, and which commands require human interaction. For general usage see [README.md](https://github.com/bgauryy/octocode-mcp/blob/main/packages/octocode-cli/README.md).

---

## Discovery (always safe in non-TTY)

```bash
octocode-cli tools                     # all tools: name + one-line description
octocode-cli tools <name>              # full input schema + example query
octocode-cli tools <n1> <n2> ...       # batch schemas
octocode-cli instructions              # full MCP instructions + all schemas (~2200 lines)
```

Start with `octocode-cli tools` to enumerate available tools, then `octocode-cli tools <name>` to get the exact input schema before running.

---

## Tool Execution

```bash
octocode-cli tools <name> --queries '<json-object>'         # single query
octocode-cli tools <name> --queries '<json-array>'          # batch queries (up to 5)
octocode-cli tools <name> --queries '<json-object>' --json  # raw JSON output
```

Shared fields (`id`, `researchGoal`, `reasoning`, `mainResearchGoal`) are auto-filled. Only provide tool-specific fields.

**Output shape** (`--json`):
```json
{ "content": [{ "type": "text", "text": "..." }], "structuredContent": {}, "isError": false }
```

**Examples:**
```bash
octocode-cli tools localSearchCode --queries '{"path":".","pattern":"TODO"}' --json
octocode-cli tools githubSearchCode --queries '{"keywordsToSearch":["useState"],"owner":"facebook","repo":"react"}' --json
octocode-cli tools githubGetFileContent --queries '{"owner":"bgauryy","repo":"octocode-mcp","path":"README.md"}' --json
```

---

## Auth

> **`auth login` always requires a human** — it opens a browser for GitHub OAuth device flow. It will hang or error in non-TTY environments.

| Command | Non-TTY | `--json` | Notes |
|---------|---------|---------|-------|
| `auth status` | ✅ | ✅ | Check if authenticated |
| `auth logout` | ✅ | ✅ | Sign out; skips TTY confirm (use `--yes` or `--json`) |
| `auth logout --yes` | ✅ | ✅ | Skip confirm prompt explicitly |
| `auth login` | ❌ | — | Requires browser (TTY only) |
| `auth login --force` | ❌ | — | Logout + re-login in one step (TTY only) |
| `auth refresh` | ✅ | ✅ | Refresh Octocode token; source-aware (see below) |
| `token` | ✅ | ✅ | Print resolved token |
| `token --validate` | ✅ | ✅ | Ping GitHub API to verify token + rate-limit |

```bash
octocode-cli auth status --json
# → { "authenticated": true, "username": "...", "hostname": "github.com",
#     "tokenSource": "octocode|gh-cli|env", "tokenExpired": false }

octocode-cli auth logout --json
# → { "success": true, "hostname": "github.com", "error": null }

octocode-cli auth refresh --json
# Octocode token:  { "success": true, "tokenSource": "octocode", "refreshable": true, "username": "..." }
# env-var token:   { "success": false, "tokenSource": "env", "refreshable": false, "error": "update the env var directly" }
# gh-cli token:    { "success": false, "tokenSource": "gh-cli", "refreshable": false, "hint": "gh auth refresh" }

octocode-cli token --validate --json
# → { "token": "...", "type": "octocode", "valid": true, "login": "user",
#     "rateLimit": { "remaining": 4999, "limit": 5000, "reset": 1748900000 } }
```

---

## Skills

### Search

`skills search` (no flags) outputs agent instructions to read and follow the `octocode-search-skill` protocol — designed for agents to execute a full multi-source search.

For direct human use, add `--direct`:

```bash
octocode-cli skills search "code review" --direct            # human: results from skills.sh grouped by repo
octocode-cli skills search "tdd" --direct --json             # machine: JSON result set
octocode-cli skills search "tdd" --direct --install          # fetch + install top result automatically
octocode-cli skills search "code review"                     # agent: outputs protocol URL + instructions
octocode-cli skills search "code review" --json              # agent: JSON protocol reference
```

**`skills search --direct --json` shape:**
```json
{
  "query": "code review",
  "count": 4,
  "results": [
    {
      "name": "code-review",
      "skillId": "code-review",
      "source": "mrgoonie/claudekit-skills",
      "totalInstalls": 421,
      "url": "https://github.com/mrgoonie/claudekit-skills",
      "readCmd": "octocode-cli skills read github:mrgoonie/claudekit-skills/code-review"
    }
  ]
}
```

### Read

Preview full `SKILL.md` content from local path or GitHub:

```bash
octocode-cli skills read /path/to/SKILL.md
octocode-cli skills read github:owner/repo/skill-name
octocode-cli skills read github:owner/repo/skill-name --json
```

### List

Scans **all** known skill directories on the OS across all targets.

```bash
octocode-cli skills list --json
octocode-cli skills list --target cursor --json    # filter to one target
```

**`skills list --json` shape:**
```json
{
  "targets": [
    {
      "target": "claude-code",
      "destDir": "/Users/you/.claude/skills",
      "exists": true,
      "skills": [
        {
          "folder": "octocode-engineer",
          "name": "Octocode Engineer",
          "description": "System-engineering skill...",
          "path": "/Users/you/.claude/skills/octocode-engineer/SKILL.md"
        }
      ]
    }
  ]
}
```

Scanned targets: `claude-code`, `claude-desktop`, `cursor`, `codex`, `opencode`, `agents` (`~/.agents/skills`)

### Install / Remove / Sync

```bash
octocode-cli skills install --targets claude-code,cursor --json
octocode-cli skills install --skill octocode-engineer --targets claude-code --json
octocode-cli skills install --targets cursor --dry-run --json      # preview only
octocode-cli skills install --local /path/to/SKILL.md --targets cursor --json
octocode-cli skills remove  --skill octocode-engineer --targets claude-code --json
octocode-cli skills remove  --local /path/to/SKILL.md --targets cursor --json
octocode-cli skills sync claude-code cursor --json                 # copy skills between targets
octocode-cli skills sync claude-code cursor --dry-run --json       # preview sync
```

---

## MCP

`mcp list` (no `--client`) scans all known MCP config files on the OS.

```bash
octocode-cli mcp list --json                                         # scan all OS MCP configs
octocode-cli mcp list --client cursor --json                         # search registry for cursor
octocode-cli mcp status [--client <id>] --json                       # servers in one config + env var status
octocode-cli mcp install --id <mcp-id> [--client <id>] [--env K=V] [--force] --json
octocode-cli mcp install --id id1,id2,id3 --client cursor --json     # batch install (parallel preflight)
octocode-cli mcp remove  --id <mcp-id> [--client <id>] --json
```

Batch preflight runs in parallel. For npm packages the preflight checks `registry.npmjs.org`; for PyPI packages `pypi.org`; otherwise the GitHub repo URL. Use `--force` to skip preflight.

**`mcp list --json` shape** (no `--client`):
```json
{
  "configs": [
    { "client": "cursor", "name": "Cursor", "configPath": "~/.cursor/mcp.json",
      "exists": true, "servers": ["octocode-mcp", "filesystem"] },
    { "client": "windsurf", "name": "Windsurf", "configPath": "...", "exists": false, "servers": null }
  ]
}
```

**`mcp list --installed --json` shape** (includes env var status):
```json
{
  "servers": [
    {
      "id": "octocode-mcp",
      "command": "npx",
      "envStatus": { "GITHUB_TOKEN": "set", "OCTOCODE_TOKEN": "missing" }
    }
  ]
}
```

Supported clients: `cursor`, `claude-desktop`, `claude-code`, `windsurf`, `zed`, `vscode-cline`, `vscode-roo`, `vscode-continue`, `opencode`, `trae`, `antigravity`, `codex`, `gemini-cli`, `goose`, `kiro`

---

## Install

```bash
octocode-cli install --ide cursor --json                         # install for a specific IDE
octocode-cli install --ide cursor --method direct --json
octocode-cli install --ide cursor --check --json                 # pre-flight: writable? already installed?
octocode-cli install --ide cursor --rollback --json              # restore most recent backup config
```

> Without `--ide`, runs interactive wizard in TTY. In non-TTY it exits with `exitCode 1`.

**`--check --json` shape:**
```json
{
  "ide": "cursor",
  "configPath": "~/.cursor/mcp.json",
  "configExists": false,
  "parentDirExists": true,
  "parentDirWritable": true,
  "action": "create",
  "method": "npx",
  "wouldOverwrite": false,
  "ready": true
}
```

**`install --json` success shape:**
```json
{ "success": true, "ide": "cursor", "configPath": "...", "method": "npx", "action": "create" }
```

---

## Sync

```bash
octocode-cli sync --json                                # sync all clients, JSON result
octocode-cli sync --dry-run --json                      # per-MCP diff, no writes
octocode-cli sync plan --json                           # alias for --dry-run
octocode-cli sync --status --json                       # analysis only
octocode-cli sync --force --json                        # auto-resolve conflicts
```

**`sync --dry-run --json` shape:**
```json
{
  "dryRun": true,
  "operations": [
    { "type": "add",      "mcpId": "octocode-mcp",  "presentIn": ["cursor"],  "missingIn": ["claude-code"], "hasConflict": false },
    { "type": "conflict", "mcpId": "playwright-mcp", "presentIn": ["cursor", "claude-code"], "missingIn": [], "hasConflict": true },
    { "type": "ok",       "mcpId": "filesystem",     "presentIn": ["cursor", "claude-code"], "missingIn": [], "hasConflict": false }
  ],
  "summary": { "totalClients": 2, "clientsWithConfig": 2, "totalUniqueMCPs": 3, "needsSyncCount": 1, "conflictCount": 1, "consistentMCPs": 1 }
}
```

---

## Cache

```bash
octocode-cli cache status --json
octocode-cli cache clean --all --json                          # clean + JSON result
octocode-cli cache clean --all --dry-run --json                # what would be freed, no delete
octocode-cli cache clean --all --yes --json                    # skip TTY confirm (non-TTY auto-skips)
octocode-cli cache clean --repos --skills --logs --json
```

**`cache clean --dry-run --json` shape:**
```json
{
  "dryRun": true,
  "plan": [
    { "target": "repos",  "path": "~/.octocode/repos",  "sizeBytes": 104857600, "sizeFormatted": "100 MB" },
    { "target": "skills", "path": "~/.octocode/skills", "sizeBytes": 2048000,   "sizeFormatted": "2 MB" }
  ],
  "totalBytes": 106905600,
  "totalFormatted": "102 MB"
}
```

**`cache clean --json` result shape:**
```json
{ "success": true, "cleaned": true, "freedBytes": 106905600, "freedFormatted": "102 MB", "targets": ["repos", "skills"] }
```

---

## Status

```bash
octocode-cli status --json                 # auth + MCP clients + cache
octocode-cli status --sync --json          # also includes sync analysis
```

**`status --sync --json` shape:**
```json
{
  "auth": { "authenticated": true, "username": "...", "tokenSource": "octocode" },
  "mcpClients": [
    { "client": "cursor", "exists": true, "serverCount": 2, "octocodeInstalled": true, "configPath": "..." }
  ],
  "cache": {
    "repos": { "sizeBytes": 104857600, "sizeFormatted": "100 MB" },
    "skills": { "sizeBytes": 0, "sizeFormatted": "0 B" },
    "logs":   { "sizeBytes": 1024, "sizeFormatted": "1 KB" },
    "totalBytes": 104858624, "totalFormatted": "100 MB"
  },
  "sync": {
    "summary": { "needsSyncCount": 1, "conflictCount": 0, "consistentMCPs": 2, "totalUniqueMCPs": 3 },
    "needsSync": [{ "mcpId": "octocode-mcp", "missingIn": ["claude-code"] }],
    "conflicts": []
  }
}
```

---

## Non-TTY Compatibility Summary

| Command | Safe without TTY | Notes |
|---------|-----------------|-------|
| `tools <name> --queries ...` | ✅ | Core tool execution |
| `tools` / `instructions` | ✅ | Discovery |
| `auth status --json` | ✅ | |
| `auth logout --json` | ✅ | Skips confirm automatically |
| `auth logout --yes` | ✅ | Explicit confirm skip |
| `auth refresh --json` | ✅ | Source-aware; fails safely for env/gh-cli tokens |
| `auth login` | ❌ | Browser required |
| `token --validate --json` | ✅ | Pings GitHub API |
| `install --ide <x> --json` | ✅ | Requires `--ide` flag |
| `install --ide <x> --check --json` | ✅ | Pre-flight only |
| `install --ide <x> --rollback --json` | ✅ | Config restore |
| `install` (no `--ide`) | ❌ | Interactive wizard |
| `skills list --json` | ✅ | |
| `skills install --targets <x>` | ✅ | Requires explicit targets |
| `skills install --dry-run --json` | ✅ | Preview only |
| `skills search --direct --json` | ✅ | Direct skills.sh search |
| `skills search` (no `--direct`) | ✅ | Outputs agent protocol |
| `skills sync <from> <to> --json` | ✅ | |
| `mcp list --json` | ✅ | |
| `mcp install --id <x>` | ✅ | Parallel preflight; use `--force` to skip |
| `mcp install --id a,b,c` | ✅ | Batch install |
| `sync --dry-run --json` | ✅ | Structured per-MCP diff |
| `sync plan --json` | ✅ | Alias for `--dry-run` |
| `sync --json` | ✅ | |
| `cache status --json` | ✅ | |
| `cache clean --all --json` | ✅ | Skips confirm in non-TTY |
| `cache clean --dry-run --json` | ✅ | Preview only |
| `status --sync --json` | ✅ | Full health check |

All commands that produce data support `--json`. Failed commands always set `process.exitCode = 1`.

---

## Exit Codes

| Code | Meaning |
|------|---------|
| `0` | Success |
| `1` | Error (auth failure, missing arg, tool error, preflight failed, etc.) |
