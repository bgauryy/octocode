---
name: octocode
description: Use when code research should leverage Octocode through the best available transport: Octocode MCP tools if this host exposes them, otherwise the `octocode` CLI or `npx octocode`. Covers local/GitHub/npm search, exact file reads, AST/LSP navigation, PR/history, package lookup, archives, and raw tool calls.
---

# Octocode

Use Octocode for evidence-driven code research across local files, GitHub, npm, PR/history, archives, and LSP/AST semantics.

## 1. Pick the transport

- **MCP first when available.** Scan the active tool list, and any host-provided tool discovery, for an Octocode MCP server or namespaced tools such as `mcp__octocode...__localSearchCode`, `ghSearchCode`, `ghGetFileContent`, `localViewStructure`, `lspGetSemantics`, `npmSearch`, or `localBinaryInspect`. Use those directly when present; read the tool/schema text before inputs, batch independent queries, and carry returned paths/lines/pages forward.
- **CLI fallback.** If MCP tools are not exposed, use `octocode`; if missing, use `npx octocode` for the same commands. Run `octocode --help` or `npx octocode --help` when flags are uncertain.
- If neither MCP nor CLI works, ask the user to install/run `npx octocode` or enable the Octocode MCP server. Do not silently replace Octocode research with plain `rg`/`grep`.

Use `OC` below as shorthand for either `octocode` or `npx octocode`.

## 2. Work smart

- Flow: orient cheap, search, read the smallest exact slice, then prove. Snippets are leads, not proof.
- Prefer quick commands; use raw `tools` only when quick commands cannot express the selector.
- Use `--json` when a later step depends on returned paths, line numbers, refs, pagination, or `location.*`. Use `--compact`/`--concise` for triage.
- Follow `next.*`, page, offset, and `location.localPath` hints. Never invent paths, line numbers, offsets, branches, or raw-tool fields.
- For remote work spanning several files, AST, or LSP, materialize first with `search --repo`, `cache fetch`, or `clone`, then continue against the returned local path.

## 3. Quick command map

| Need | CLI | MCP tool family |
|---|---|---|
| Map a tree | `OC search <path\|owner/repo> --tree --depth 2` | `localViewStructure` / `ghViewRepoStructure` |
| Find files | `OC search auth . --search path --ext ts` | `localFindFiles` |
| Search text/code | `OC search executeDirectTool . --lang ts` | `localSearchCode` / `ghSearchCode` |
| Read exact content | `OC search file.ts --content-view exact --match-string 'anchor'` | `localGetFileContent` / `ghGetFileContent` |
| Orient by symbols | `OC search file.ts --symbols` or `--op documentSymbols` | `lspGetSemantics` / content symbols |
| Trace a symbol | `OC search file.ts --op references --symbol runCLI --line 42` | `lspGetSemantics` |
| AST/code shape | `OC search . --pattern 'eval($X)' --lang js` or `--rule '<yaml>'` | `localSearchCode` structural mode |
| GitHub PRs/history | `OC search owner/repo#123 --target pullRequests --deep`; `--target commits` | `ghHistoryResearch` |
| Repos/packages | `OC search 'query' --target repositories`; `OC search zod --target packages` | `ghSearchRepos` / `npmSearch` |
| Archives/binaries | `OC search file.zip --target artifacts --list`; `OC unzip file.zip` | `localBinaryInspect` |
| Remote as local | `OC search term subdir --repo owner/repo --json`; `OC cache fetch owner/repo path`; `OC clone owner/repo` | `ghCloneRepo` plus local tools |

Quick commands auto-route existing paths as local and `owner/repo[/path][@ref]` or GitHub URLs as remote. Per-command truth lives in `OC <command> --help`.

## 4. Raw CLI tools

```bash
OC tools
OC tools <name> --scheme
OC tools <name> --queries '<json>' --json
```

Read `--scheme` before every raw call. `id`, `mainResearchGoal`, `researchGoal`, and `reasoning` are auto-filled by the CLI; do not pass them. `--queries` accepts one object, an array of up to 5, or `{"queries":[...]}`.

## 5. When deeper workflow matters

Read `references/recipes.md` for tiny command sequences. For large investigations, PR/local review, dead-code, architecture, or AST/LSP-heavy work, use the `octocode-engineer` skill and its references.

## Failure and auth

Bad input exits `2` and prints valid flags; `3` means not found; `4` auth; `5` tool error; `7` rate limited. Humans can run `OC login`; agents should check `OC auth status --json` or `OC status --json` and use `GITHUB_TOKEN`, `OCTOCODE_TOKEN`, or `GH_TOKEN` when available. Deep protocol: `OC context [--full]`. Docs: https://github.com/bgauryy/octocode/tree/main/docs
