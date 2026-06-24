# Research Playbook

How to gather cited evidence with Octocode. Pick MCP if available; otherwise use the CLI. Do not guess facts that tools can verify.

## Local codebase evidence

Use when the current repo matters.

| Need | MCP | CLI |
|---|---|---|
| Map structure | `localViewStructure` | `octocode search <path> --tree` |
| Find files | `localFindFiles` through OQL `target:"files"` | `octocode search --search path` |
| Search code | `localSearchCode` | `octocode search` |
| Read exact code | `localGetFileContent` | `octocode search <file> --content-view exact` |
| AST shape proof | `localSearchCode(mode:"structural")` | `octocode search <path> --pattern '<shape>' --lang <language>` |
| Symbols / LSP | `lspGetSemantics` | `octocode search <file> --symbols` / `octocode search <file> --op ...` |

Local flow:

```text
search --tree → search --search path / text search → symbols → matchString/line range → AST/LSP → cited current-state evidence
```

## External evidence

Use for prior art, package choices, cross-repo comparison, and history.

| Need | MCP | CLI |
|---|---|---|
| Package → repo | `npmSearch` | `octocode search <name> --target packages` |
| Discover repos | `ghSearchRepos` | `octocode search <keywords> --target repositories` |
| Map repo | `ghViewRepoStructure` | `octocode search owner/repo --tree` |
| Search GitHub | `ghSearchCode` | `octocode search kw owner/repo` |
| Read GitHub file | `ghGetFileContent` | `octocode search owner/repo/path --content-view exact` |
| PR/commit history | `ghHistoryResearch` | `octocode pr` / `octocode search owner/repo/path --target commits` |
| Clone for deep proof | `ghCloneRepo` | `octocode clone` |

External flow:

```text
search --target packages/repositories → ghViewRepoStructure → ghSearchCode path/content discovery → ghGetFileContent proof → commits/PR rationale
```

Clone and switch to local tools when analysis spans several files or needs AST/LSP.

## Binary/archive evidence

Use when the source is packaged or compiled.

| Need | MCP | CLI |
|---|---|---|
| Inspect binary metadata | `localBinaryInspect(mode:"inspect")` | `octocode search <file> --target artifacts --inspect` |
| List archive entries | `localBinaryInspect(mode:"list")` | `octocode search <file> --target artifacts --list` |
| Extract one entry | `localBinaryInspect(mode:"extract")` | `octocode search <file> --target artifacts --extract <entry>` |
| Decompress stream | `localBinaryInspect(mode:"decompress")` | `octocode search <file> --target artifacts --decompress` |
| Inspect strings | `localBinaryInspect(mode:"strings")` | `octocode search <file> --target artifacts --strings` |
| Unpack archive | `localBinaryInspect(mode:"unpack")` | `octocode unzip` |

Flow: identify/list → extract one entry or unpack all → run local tools on the returned `localPath`.

## Research plan — run only the tracks that matter

| Scenario | Research tracks |
|---|---|
| Existing-system change | Local current state + local blast radius; external prior art if options are unclear |
| Greenfield choice | External prior art + package/repo comparison; local constraints if repo exists |
| Migration | Local current state + contracts/data flows + external migration examples |
| Library/package adoption | npm/package metadata + repo source + local integration points |
| Refactor plan | Local structure + LSP references/callers + AST duplication/smell checks |
| RFC validation | Map each claim to local/external evidence; mark confirmed/likely/uncertain |

## Evidence rules

- Local claims need `file:line`.
- External code claims need GitHub file path/line or PR/commit link.
- Snippets are leads; use `matchString`, line ranges, AST, LSP, or history before citing.
- Key recommendations need at least one supporting source and one counterpoint or rejected alternative.

## Recovery

| Situation | Move |
|---|---|
| Local search empty | broaden search, inspect structure, try symbols/AST variants |
| GitHub search empty | use repo structure/path search, known files, or clone |
| No external prior art | say so; rely on local constraints and unresolved questions |
| Evidence conflicts | present conflict and decision rule |
| Scope too broad | split into multiple RFCs or phases |
| Two attempts fail | summarize what is known and ask for direction |
