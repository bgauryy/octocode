---
name: octocode-cli
description: Use `octocode-cli` subcommands to execute Octocode MCP tools from a terminal without wiring MCP. Use when the user asks to "run octocode from shell", "use octocode without MCP", "call ghSearchCode from CLI", or wants a one-off GitHub code/file/PR search in the terminal.
---

# Octocode CLI — Agent Playbook

Six subcommands, one tool each. Pick by task shape. Always add `--json` and pipe to `jq`.

## Task → Command

| What you need | Command | Key flags |
|---|---|---|
| Find where a symbol is defined or used | `search-code` | `--query 'symbol'` `--owner X --repo Y` |
| Read a specific file (whole or a window) | `get-file` | `--match-string 'anchor'` or `--start-line N --end-line M` or `--full-content` |
| Map a repo's layout | `view-structure` | `--depth 2` |
| Find repos by topic/popularity | `search-repos` | `--topics a,b` `--stars '>=100'` |
| PR history on a repo | `search-prs` | `--owner X --repo Y --merged` |
| Resolve a package to a repo + read metadata | `package-search` | `--name pkg --ecosystem npm` |

## Core discipline

These three rules decide whether the CLI is fast or slow:

1. **Parallelize independent calls.** Need files A, B, C from one repo? Issue all three `get-file` calls in one message so the shell runs them concurrently. Sequential loops waste wall-clock.
2. **One wide query beats five narrow ones.** First `search-code` should be the broadest that plausibly returns <20 hits. If you got reasonable results, *read them* — don't re-query to "narrow down". Iterating `search-code` is the #1 reason the CLI ever feels slow.
3. **Bulk via stdin when the same command runs >2 times.** One process start beats N.

```bash
printf '{"queries":[
  {"keywordsToSearch":["useState"],"owner":"facebook","repo":"react"},
  {"keywordsToSearch":["useEffect"],"owner":"facebook","repo":"react"}
]}' | octocode-cli search-code --json | jq
```

## Recipes by task shape

### Symbol lookup (find definition + a few callers)

```bash
# One search returns the defining file and call sites together
octocode-cli search-code --query 'runCLI' --owner bgauryy --repo octocode-mcp --limit 10 --json | jq

# Read the hit that looks like the definition
octocode-cli get-file --owner bgauryy --repo octocode-mcp \
  --path packages/octocode-cli/src/cli/index.ts \
  --match-string 'export function runCLI' --json | jq
```

### Workspace mapping (list packages + read each package.json)

```bash
# Structure first — one call, entire tree
octocode-cli view-structure --owner bgauryy --repo octocode-mcp --depth 2 --json | jq
```

Then fetch every `package.json` **in a single message** by issuing parallel `get-file` calls (your agent's parallel tool-use, not a sequential loop). A 5-package mapping is 1 structure call + 5 parallel fetches = 2 round-trips, not 6.

### Cross-repo symbol search

```bash
# ONE search with a distinctive keyword. The top hit's path is the answer.
octocode-cli search-code --query 'discriminatedUnion' --owner colinhacks --repo zod --limit 5 --json | jq
```

Don't re-query to "confirm". Once you have a path that plausibly defines the symbol, answer from it.

### Call-chain trace

```bash
# Seed with the entry function
octocode-cli search-code --query 'runCLI' --owner bgauryy --repo octocode-mcp --json | jq
```

For each callee name you find, search it to locate the next hop. **3–4 `search-code` calls reach a handler; 10+ means you're lost** — back out and try a broader seed query.

## Flags at a glance

- Lists are comma-separated: `--query 'a,b,c'` → `['a','b','c']`.
- Kebab → camel: `--match-string` → `matchString`, `--full-content` → `fullContent`.
- Boolean flags take no value: `--merged`, `--draft`, `--full-content`, `--with-comments`, `--with-commits`, `--npm-fetch-metadata`, `--python-fetch-metadata`.
- Numeric flags: `--limit`, `--depth`, `--start-line`, `--end-line`. Non-numeric exits `1`.
- `id`, `mainResearchGoal`, `researchGoal`, `reasoning` are auto-filled — don't pass them.
- `--json` prints compact `structuredContent` only (no text preamble, no envelope). Byte-equivalent to MCP payload. Pipe into `jq`.
- Exit `0` on success, `1` on missing flag / bad stdin / validation / tool error.

## Auth

Uses Octocode-stored creds or `gh` CLI token, in that order.

```bash
octocode-cli login   # or: gh auth login
```
