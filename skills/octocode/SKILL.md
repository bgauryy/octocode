---
name: octocode
description: Drive the `octocode` CLI to research code from a terminal without wiring MCP — across LOCAL files and EXTERNAL GitHub/npm with one toolset. Use when the user asks to "run octocode from shell", "use octocode without MCP", "search GitHub code from the terminal", or wants a one-off local/remote code, file, repo, PR, or package lookup in the shell.
---

# Octocode CLI — Agent Playbook

Binary: `octocode`. Run `octocode --help` to see everything; it prints an `<AGENT_INSTRUCTIONS>` block plus the full command list. Two surfaces:

1. **Quick commands** — friendly shortcuts that auto-route a local path vs an `owner/repo`. Use these first.
2. **Raw tools** — `octocode tools <name> --queries '<json>'` runs an Octocode MCP tool directly. Read the schema FIRST.

## Discipline (read this first)

- **Do NOT hallucinate** paths, line numbers, or fields — verify with the tools. Search snippets are *discovery, not proof*; re-read the exact text before quoting.
- **Flow:** locate → search → read the smallest slice → prove. Pick the cheapest tool that answers the current question.
- **One wide query beats five narrow ones.** If results are reasonable, read them — don't re-query to "narrow down".
- **Parallelize independent calls** — issue them in one message so the shell runs them concurrently.
- **Output:** clean YAML by default (read it directly). Add `--json` for the raw envelope, `--compact` for the leanest text.

## Task → quick command

| What you need | Command |
|---|---|
| Directory / repo layout | `octocode ls <path\|owner/repo> [--depth 2]` |
| Read (and minify) a file | `octocode cat <path\|owner/repo/path> [--mode none\|standard\|symbols] [--match-string '<anchor>']` |
| Text / regex code search | `octocode grep '<keywords>' <path\|owner/repo> [--type ts]` |
| Find files by name/path/content | `octocode find '<query>' [path\|owner/repo] [--search path\|content\|both]` |
| Code-shape search (AST, local) | `octocode grep <path> --pattern '<shape>'` (`--rule '<yaml>'` for relational) |
| Symbol outline of a file/dir | `octocode ls <dir> --symbols` · `octocode cat <file> --mode symbols` |
| Symbol identity (defs/refs/callers) | `octocode lsp <file> --type references --symbol <name> --line <n>` |
| Discover GitHub repositories | `octocode repo '<keywords>' [--language ts --stars '>100']` |
| Pull requests (list or deep-read) | `octocode pr <owner/repo[#N]\|PR-URL> [--deep]` |
| npm package + source repo | `octocode pkg <package>` |
| Inspect an archive / binary | `octocode binary <file>` |
| Unpack an archive for multi-file work | `octocode unzip <archive>` |
| Clone a repo/subtree for deep work | `octocode clone <owner/repo[/path][@branch]>` |

Quick commands route by their argument: a path that exists locally (or starts with `./`, `/`, `../`) is local; `owner/repo[/path]`, `owner/repo@branch`, or a `github.com` URL is GitHub. Per-command details: `octocode <command> --help`.

## Raw tools (when a quick command doesn't fit)

```bash
octocode tools                       # list every tool with its required fields
octocode tools <name> --scheme       # READ THE SCHEMA FIRST — fields, types, bounds, defaults
octocode tools <n1> <n2> --scheme    # read several schemas at once
octocode tools <name> --queries '<json>'          # run it (one object, or an array of ≤5)
octocode tools <name> --queries '<json>' --json   # raw envelope
```

`id`, `mainResearchGoal`, `researchGoal`, `reasoning` are auto-filled; don't pass them. Batch up to 5 independent sub-queries in one call:

```bash
octocode tools ghSearchCode --queries '[
  {"keywordsToSearch":["useState"],"owner":"facebook","repo":"react"},
  {"keywordsToSearch":["useEffect"],"owner":"facebook","repo":"react"}
]' --json | jq
```

## Recipes

**Symbol lookup (definition + callers):**
```bash
octocode grep 'runCLI' bgauryy/octocode-mcp --type ts --limit 10
octocode cat bgauryy/octocode-mcp/packages/octocode/src/cli/index.ts --match-string 'export function runCLI' --mode none
```

**Workspace mapping (layout + each package.json):** one `ls --depth 2`, then parallel `cat` calls for each `package.json` in a single message.

**Deep multi-file work in one repo (>~3 files):** `octocode clone owner/repo`, then run `grep`, `grep --pattern` (AST), `ls --symbols`, `lsp`, and `cat` on the local clone instead of many GitHub round-trips.

## On failure

A bad flag prints the valid flags for that command and exits `2`. Exit codes: `0`=ok, `2`=bad-input, `3`=not-found, `4`=auth, `5`=tool-error, `7`=rate-limited.

## Auth

Auth is for humans: `octocode login` (or `gh auth login`). Agents/CI pass a token via env — `GITHUB_TOKEN`, `OCTOCODE_TOKEN`, or `GH_TOKEN`. Check with `octocode status`.

## Deeper context

`octocode context` (add `--full`) prints the agent protocol, MCP system prompt, and tool descriptions — use it only to optimize deeper or autonomous research. Full docs: https://github.com/bgauryy/octocode/tree/main/docs
