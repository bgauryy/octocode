# Octocode CLI Reference

`octocode` is the terminal interface for code research:

- Run all Octocode MCP tools directly from the shell.
- Use smart quick commands for files, trees, search, PRs, packages, LSP workflows, and repo cloning.
- Use raw `tools` for lower-level GitHub/local research and exact schema-driven calls.
- Manage Octocode setup for IDEs when needed.

## Usage

```bash
octocode <command> [options]

# Quick research commands
octocode cat <path|owner/repo/path>
octocode cat <repo-relative-file> --repo <owner/repo[@ref]>
octocode ls <path|owner/repo>                       # tree; a file or --symbols shows a symbol outline
octocode grep <keywords> <path|owner/repo>          # text/regex; --pattern/--rule for AST shape
octocode find <query> [path|owner/repo]
octocode clone <owner/repo[/path][@branch]|url>
octocode cache fetch <owner/repo[@ref]> [path]
octocode pr <owner/repo[#N]|PR-URL>
octocode repo <keywords...>
octocode pkg <package>
octocode lsp <file> --type <type> --symbol <name> --line <n>
octocode binary <file>
octocode unzip <archive>

# Raw tool runner
octocode tools
octocode tools <name> --scheme
octocode tools <name> --queries '<json>'
octocode context [--full] [--json]
```

## Command Index

| Command | Purpose |
|---------|---------|
| `cat` | Read local/GitHub files, or materialize `--repo` and read the saved local file, with minification, line ranges, and match slices. |
| `ls` | Browse local/GitHub trees, or materialize `--repo`; files or `--symbols` return symbol outlines. |
| `grep` | Search text/regex locally/GitHub, or materialize `--repo`; local and `--repo` support structural AST search. |
| `find` | Find files by name/path/metadata, or local/GitHub/clone-backed content matches. |
| `pr` | Search pull requests or deep-read one PR. |
| `history` | Inspect commit history for a GitHub repo, directory, or file. |
| `repo` | Discover GitHub repositories. |
| `pkg` | Search npm packages and hand off to source repositories. |
| `binary` | Inspect binaries (format/symbols/imports/deps), list/extract archives, decompress streams, or read strings. |
| `unzip` | Unpack an archive to `<octocode-home>/tmp/unzip/<name>-<timestamp>/`. |
| `clone` | Clone a GitHub repo or sparse subtree to `<octocode-home>/tmp/clone/`. |
| `cache` | Materialize remote files/trees/repos, inspect status, or clear tmp storage. |
| `lsp` | Run symbol-anchored semantic queries on local or clone-backed remote files. |
| `tools` | List tools, read schemas, and run any MCP tool directly. |
| `context` | Print agent-facing protocol, system prompt, tool descriptions, and schemas. |
| `install` | Configure Octocode in supported MCP clients. |
| `auth` | Run auth subcommands: `login`, `logout`, or `refresh`. |
| `login` / `logout` | Sign in or clear stored GitHub credentials directly. |
| `status` | Show token presence/source, auth identity, MCP install state, sync info, and cache paths. |

Removed commands: `token`, `skills`, and `auth status`. Use `status` to confirm whether a token is present.

## Agent Flow

Agents should use this order:

1. `octocode context`
2. `octocode tools`
3. `octocode tools <name> --scheme`
4. `octocode tools <name> --queries '<json>'`

Use `octocode context --full` for complete tool descriptions, and `octocode context --json` when automation needs a machine-readable `{ "context": "..." }` wrapper. Read schemas on demand with `octocode tools <name> --scheme`.

## UX Map

| Need | Use |
|------|-----|
| Map files and repos | `ls`, `find`, `repo`, `pkg` |
| Search text or code structure | `grep` (text/regex; `--pattern`/`--rule` for AST shape) |
| Read less, cite exact evidence | `cat --mode symbols`, `cat --match-string`, `cat --start-line ... --end-line ...` |
| Outline a file / trace symbols semantically | `ls <file>` or `ls <dir> --symbols`, then `lsp --type ... --symbol ... --line ...` |
| Inspect PRs/history or clone for local analysis | `pr`, `history`, `clone`, `cache fetch`, or quick-command `--repo` |
| Inspect archives/binaries | `binary`, `unzip` |
| Configure Octocode | `install`, `auth`, `login`, `logout`, `status` |
| Run any MCP tool directly | `tools <name> --scheme`, then `tools <name> --queries '<json>'` |

## Global Options

| Option | Meaning |
|--------|---------|
| `--help` | Show help. |
| `--version` | Show version. |
| `--json` | Print raw JSON MCP envelope for tool runs. |
| `--compact` | Print lean tool output. |
| `--no-color` | Disable ANSI color. Also honors `NO_COLOR=1`. |

Unknown flags are rejected before a command runs. The error lists valid flags for that command and suggests near-miss typos.

## Quick Commands

Auto-route based on target: a local path routes to local tools; `owner/repo[/path][@branch]` routes to GitHub. For `cat`, `ls`, `grep`, `find`, and `lsp`, `--repo owner/repo[@ref]` first materializes the remote target under Octocode's `.octocode` storage, then runs the local tool and returns absolute local-path hints. All commands support `--json`.

| Command | Routes to | What it does |
|---------|-----------|------|
| `cat` | `localGetFileContent` / `ghGetFileContent` / `ghCloneRepo` | Read and minify file content; `--repo` reads the saved local clone path |
| `ls` | `localViewStructure` / `ghViewRepoStructure` / `lspGetSemantics` / `ghCloneRepo` | Directory tree; a file or `--symbols` shows a symbol outline |
| `grep` | `localSearchCode` / `ghSearchCode` / `ghCloneRepo` | Text/regex search; `--pattern`/`--rule` for Octocode AST shape (local or `--repo`) |
| `find` | `localFindFiles` / `localSearchCode` / `ghSearchCode` / `ghCloneRepo` | Find files by name, path, or content |
| `clone` | `ghCloneRepo` | Clone a repo or subtree to the Octocode home `tmp/clone/` cache |
| `cache` | `ghGetFileContent` / `ghCloneRepo` | Materialize a remote file/tree/repo, report storage status, or clear tmp outputs |
| `pr` | `ghHistoryResearch` | List or deep-dive pull requests |
| `history` | `ghHistoryResearch` (commits) | Commit history for a repo, dir, or file (→ `#PR` deep-read) |
| `repo` | `ghSearchRepos` | Discover GitHub repositories |
| `pkg` | `npmSearch` | npm package metadata + source repo |
| `lsp` | `lspGetSemantics` | Definition, references, callers, callees, call hierarchy, hover, type definition, implementation |
| `binary` | `localBinaryInspect` | Archives, compressed files, native binaries |
| `unzip` | `localBinaryInspect` (unpack) | Unpack an archive to a fresh `<octocode-home>/tmp/unzip/<name>-<timestamp>/` directory |

## Minimize First

Use the CLI in this order: map cheaply, search narrowly, then read the smallest proof slice.

- `--compact` trims CLI rendering; `--json` returns the raw envelope when automation needs it.
- `--concise` returns path/title-only discovery lists for search-style commands.
- `cat --mode symbols` gives a line-numbered skeleton before reading bodies.
- `cat --match-string`, `--start-line`, and `--end-line` keep evidence reads small and quotable.
- `grep --mode discovery` finds files only; switch to paginated/detailed only after narrowing.
- For remote multi-step work, prefer `--repo owner/repo[@ref]` or `cache fetch`: output hints include the absolute saved path so the next step can use local `ls`, `grep`, `cat`, and `lsp`.

### cat

```
cat <path|owner/repo/path>
    --repo <owner/repo[@ref]>        materialize remote repo first; path is repo-relative
    --mode  none|standard|symbols    minification (default: standard)
    --branch <ref>                   branch for GitHub paths
    --match-string <s>               return only sections containing this string
    --match-regex                    treat --match-string as a regex
    --match-case-sensitive
    --start-line <n>                 first line (1-based)
    --end-line <n>
    --context-lines <n>              context lines around --match-string hits
    --page-size <n>                  characters per page
    --page <n>
    --char-offset <n>                character offset for continuation
    --char-length <n>
    --full-content                   return the whole file
    --content-type file|directory    GitHub content type
    --force-refresh                  bypass GitHub cache
    --json
```

Without `--repo`, GitHub targets use direct GitHub file fetch. With `--repo`, the CLI checks/materializes the requested branch and path under Octocode's `.octocode` storage, reads the saved local file, and returns hints with the absolute local path for follow-up local tools.

Examples:

```bash
octocode cat src/index.ts
octocode cat src/index.ts --mode symbols
octocode cat src/index.ts --match-string "runCLI" --mode none
octocode cat src/index.ts --start-line 40 --end-line 90 --mode none
octocode cat facebook/react/packages/react/index.js
octocode cat facebook/react/packages/react/index.js --branch 18.2.0
octocode cat packages/react/index.js --repo facebook/react --mode symbols
```

### ls

Shows structure at any zoom. A **directory** (local or GitHub) lists a **tree**; a local **file** — or any path materialized with `--repo` and `--symbols` — shows a **semantic symbol outline** (`lspGetSemantics type=documentSymbols`). The outline replaces the former `symbols` command.

```
ls [path|owner/repo]
    --repo <owner/repo[@ref]>  materialize remote repo first; path is repo-relative
    --symbols        show a symbol outline instead of a tree (local or --repo;
                     auto-enabled when the target is a file)
    --kind <kind>    outline: filter by kind — function, class, method, …
    --depth <n>      tree: recursion depth · outline: directory discovery depth
    --branch <ref>   branch for GitHub paths or --repo materialization
    --force-refresh  re-clone --repo materialization
    --pattern <glob> name filter (local tree only)
    --ext <list>     comma-separated extension whitelist (tree filter; outline: which source files)
    --sort name|size|time|extension   (local tree only)
    --reverse        reverse sort (local tree only)
    --files-only     list files only (local tree only)
    --dirs-only      list directories only (local tree only)
    --hidden         include dot-files (local tree only)
    --limit <n>      tree: cap entries · outline: max files (default: 10)
    --page <n>
    --page-size <n>  tree: entries/page · outline: symbols/file (default: 40)
    --json
```

For JavaScript/TypeScript the outline works **with no language server installed** via a native (oxc) fast path — syntax-only, no type inference; with a TS server present, results are type-aware. Each result carries `lsp.source` (`native` or `lsp`).

Examples:

```bash
octocode ls src
octocode ls src --depth 3 --ext ts,tsx
octocode ls facebook/react --branch 18.2.0 --depth 2
octocode ls packages/react --repo facebook/react --depth 2
octocode ls src/index.ts                       # file → symbol outline
octocode ls src --symbols --ext ts,tsx --limit 10
octocode ls packages/react/index.js --repo facebook/react --symbols
octocode ls src/index.ts --symbols --kind function
```

### grep

```
grep <keywords> <path|owner/repo>          text/regex search
grep <keywords> [path] --repo <owner/repo[@ref]>  materialize remote then local search
grep <path> --pattern <shape>              AST shape search (local or --repo)
grep <path> --rule <yaml>                  AST relational rule (local or --repo)
    --repo <owner/repo[@ref]>  materialize remote repo first; path is repo-relative
    --pattern <ast>    AST shape — switches grep to structural search (local or --repo).
                       Metavars: $X = one node, $$$ARGS = a list. e.g. 'eval($X)'
    --rule <yaml>      relational YAML rule — not/inside/has/all/any.
                       Mutually exclusive with --pattern. Local or --repo.
    --type <ext|lang>  filter by language or extension (ts, rust, typescript, "*.rs")
    --mode paginated|discovery|detailed   (local only, default: paginated)
    --concise          paths only, no snippets — cheapest orientation
    --include <glob>   include globs (local only)
    --exclude <glob>   exclude globs (local only)
    --context-lines <n> / --context <n>   context around each match (local only)
    --fixed / --fixed-string              literal string search (local only)
    --perl-regex                          advanced regex features (local only)
    --case-insensitive / --case-sensitive
    --whole-word / --invert-match
    --hidden / --no-ignore
    --files-only / --files-without-match
    --count-lines / --count-matches
    --only-matching     return only the matched substring(s), one per hit, not
                        the whole line — enumerates every hit on a minified
                        one-liner (local only)
    --match-window <n>  with --only-matching, chars of context kept each side of
                        the match (… marks trimmed sides); 0 = bare match (local only)
    --multiline / --multiline-dotall
    --match-length <n>  characters kept per match snippet (local only)
    --max-matches <n>   max matches per file (local only)
    --max-files <n>     max matched files returned (local only)
    --match-page <n>    page within one file's matches (local only)
    --branch <ref>      branch for GitHub paths or --repo materialization
    --force-refresh     re-clone --repo materialization
    --limit <n>         max files in output
    --page <n>
    --page-size <n>
    --json
```

Text/regex runs locally or on GitHub. With `--repo`, the CLI materializes the branch/path under Octocode's `.octocode` storage, runs local search, and returns hints with the absolute saved path. AST shape search (`--pattern`/`--rule`) runs locally, including clone-backed `--repo` paths — comments and strings never false-match.
Quote structural patterns with single quotes so the shell does not expand `$A`,
`$X`, or `$$$ARGS` before Octocode sees them. Once the shell has already
expanded a metavariable, the CLI cannot recover the original pattern.
For local searches, `--type` accepts short extensions and language aliases; for example `--type rust` maps to `.rs`, while `--type typescript` covers `.ts`, `.tsx`, `.mts`, and `.cts`.

Examples:

```bash
octocode grep runCLI src
octocode grep executeDirectTool src --type ts --mode discovery
octocode grep 'runCLI\s*\(' packages/octocode/src --perl-regex --context 1 --max-files 2
octocode grep '\w+\.cursor\.sh' bundle.min.js --only-matching            # enumerate every hit on a minified one-liner
octocode grep 'api\d' bundle.min.js --only-matching --match-window 12     # matched span + surrounding context
octocode grep "useState" facebook/react --type ts
octocode grep "useState" packages/react --repo facebook/react --type js --mode discovery
octocode grep "executeCloneRepo" bgauryy/octocode-mcp --concise
octocode grep src --pattern 'eval($X)'
octocode grep packages/react --repo facebook/react --pattern 'useState($$$ARGS)' --type js
octocode grep packages/octocode/src --pattern 'console.log($$$ARGS)' --type ts
octocode grep packages/octocode/src --pattern '$A && $A()' --type ts
# --rule is YAML. In single quotes `\n` stays LITERAL — use $'...' (bash/zsh
# interpret \n) or paste a real multiline string:
octocode grep src --rule $'rule:\n  pattern: await $C\n  inside:\n    kind: for_statement\n    stopBy: end'
octocode grep src --rule 'rule:
  pattern: await $C
  inside:
    kind: for_statement
    stopBy: end'
```

### find

```
find <query> [path|owner/repo]
    --repo <owner/repo[@ref]>       materialize remote repo first; --path/target is repo-relative
    --source auto|local|github      routing (default: auto)
    --search path|content|both      search mode (default: path)
    --ext <list>                    comma-separated extensions
    --path <subpath>                local root or GitHub repo subpath
    --branch <ref>                  branch for --repo materialization
    --force-refresh                 re-clone --repo materialization
    --owner <owner>                 GitHub owner
    --repo <repo>                   with --owner, GitHub repository filter
    --filename <name>               GitHub filename filter
    --limit <n>
    --page <n>
    --page-size <n>

    Local path filters:
    --name <glob>                   basename glob(s)
    --path-pattern <pattern>        full path pattern
    --regex <pattern>               basename regex
    --min-depth <n>
    --max-depth <n>
    --entry f|d                     file (f) or directory (d)
    --modified-within <window>      e.g. 7d, 2h, 1w
    --modified-before <window>
    --accessed-within <window>
    --size-greater <size>           e.g. 100k, 1m
    --size-less <size>
    --permissions <mode>
    --executable / --readable / --writable
    --empty
    --exclude-dir <list>
    --sort modified|name|path|size  path mode
    --details                       include metadata
    --show-modified                 include modified timestamps

    Local content filters (when --search content|both):
    --mode paginated|discovery|detailed
    --include <glob>
    --exclude <glob>
    --exclude-dir <list>
    --sort path|modified|accessed|created
    --sort-reverse
    --case-insensitive / --case-sensitive / --whole-word
    --fixed-string / --perl-regex
    --invert-match
    --hidden / --no-ignore
    --context-lines <n>
    --match-length <n>
    --max-matches-per-file <n>
    --max-files <n>
    --match-page <n>
    --multiline / --multiline-dotall
    --count-lines / --count-matches
    --files-only / --files-without-match
    --verbose                       GitHub only
    --concise                       flat paths for GitHub or lean local output
    --json
```

Without `--owner`, `--repo owner/repo[@ref]` is remote-as-local mode: Octocode materializes the requested repo/subpath under `.octocode`, runs `localFindFiles` / `localSearchCode`, and hints the absolute saved path for follow-up local commands. With `--owner`, `--repo <repo>` keeps the old GitHub search-filter meaning.

Examples:

```bash
octocode find auth src --source local --search path --ext ts
octocode find executeDirectTool . --source local --search content --ext ts
octocode find useState --repo facebook/react --path packages/react --search both --ext js
octocode find auth bgauryy/octocode-mcp --source github --search path --ext ts
octocode find auth . --search both --limit 20
```

> **AST / structural search** lives under `grep --pattern`/`--rule` (see the `grep` section). The standalone `ast` command was removed.

### clone

Clone a GitHub repo or subtree locally. Clones land at `<octocode-home>/tmp/clone/<owner>/<repo>/<branch>/` with a 24-hour cache. In the CLI, clone is enabled by default unless `ENABLE_CLONE=false`; MCP clone tools require `ENABLE_CLONE=true`.

```
clone <owner/repo[/path][@branch]|url>
    --branch <ref>      override branch (also parses from @branch syntax)
    --force-refresh     bypass 24-hour cache and re-clone
    --json
```

Accepted ref formats:

| Input | Effect |
|-------|--------|
| `owner/repo` | Full clone of default branch |
| `owner/repo/packages/core` | Sparse checkout of `packages/core` subtree |
| `owner/repo@main` | Full clone at branch `main` |
| `owner/repo@main/src` | Sparse checkout of `src` at branch `main` |
| `https://github.com/owner/repo/tree/main/src` | Same as above |

Examples:

```bash
octocode clone facebook/react
octocode clone facebook/react/packages/react
octocode clone facebook/react@18.2.0/packages/react
octocode clone https://github.com/vercel/next.js/tree/main/packages/next
```

Clone output includes the absolute saved path and local-tool hints. After cloning, use local tools against that path:

```bash
octocode ls <localPath-from-clone-output>
octocode grep "useState" <localPath-from-clone-output>
octocode cat <localPath-from-clone-output>/packages/react/index.js
octocode ls <localPath-from-clone-output>/packages/react/index.js --symbols
```

For one-step remote-as-local research, use `cat`, `ls`, `grep`, `find`, or `lsp` with `--repo owner/repo[@ref]`; these commands materialize first and immediately run the local tool.

### cache

      Materialize remote files, trees, or whole repos into Octocode's `<octocode-home>/tmp/` storage and print agent-facing local-tool hints. Files and API-fetched trees land in `tmp/tree`; git clones land in `tmp/clone`. `cache fetch` checks existing local materialization first; use `--force-refresh` only when you need to bypass it.

      ```
      cache fetch <owner/repo[@ref]> [path]
          --depth file|tree|clone  requested materialization depth (default: file with path, clone without path)
          --branch <ref>           branch, tag, or SHA
          --force-refresh          bypass existing tmp materialization and refresh
          --json

cache status [--json]
cache clear --clone|--repos|--tree|--binary|--unzip|--all [--json]
```

Examples:

      ```bash
      octocode cache fetch facebook/react packages/react
      octocode cache fetch facebook/react packages/react --depth tree
      octocode cache fetch facebook/react packages/react/index.js --depth file
      octocode cache status
      ```

      After `cache fetch`, continue locally with the returned absolute path:

      ```bash
      octocode ls <localPath>
      octocode find useState <localPath> --search both
      octocode grep "useState" <localPath>
      octocode cat <localPath>/index.js
      octocode lsp <localPath>/index.js --type references --symbol useState --line <lineHint>
      ```

### pr

```
pr <owner/repo[#N]|PR-URL>
    --pr <n>                     PR number (alternative to #N syntax)
    --state open|closed|merged
    --query <keywords>           keyword filter in list mode
    --author <user>
    --label <label>
    --base <branch>
    --sort created|updated|best-match|comments|reactions
    --order asc|desc
    --draft                      show only draft PRs
    --created <range>            e.g. >2024-01-01
    --merged-at <range>
    --concise                    flat "#number title" list — cheapest triage
    --limit <n>
    --page <n>
    --page-size <n>
    --patches                    include unified diffs
    --file <path>                diff for one file only
    --comments                   include comments
    --commits                    include commits
    --deep                       patches + comments + commits + reviews
    --match-string <s>           narrow returned content
    --char-length <n>
    --char-offset <n>
    --json
```

Examples:

```bash
octocode pr facebook/react
octocode pr facebook/react --state merged --limit 20
octocode pr facebook/react#29940 --patches --comments
octocode pr https://github.com/vercel/next.js/pull/65000 --deep
```

### history

Commit history for a repo, directory, or file — who changed what, when. A commit headline that embeds `(#NNN)` links to its PR: deep-read it with `pr owner/repo#NNN`.

```
history <owner/repo[/path][@branch]>
    --since <iso>        ISO 8601, e.g. 2024-01-01T00:00:00Z (commits mode)
    --until <iso>
    --author <name>      filter by commit author
    --branch <ref>       branch or SHA to walk (also parsed from @branch)
    --diff               include per-commit file diffs (larger output)
    --limit <n>          max commits shown (default: 20)
    --page <n>
    --page-size <n>
    --json
```

Examples:

```bash
octocode history facebook/react/packages/react/src
octocode history bgauryy/octocode/README.md --diff
octocode history vercel/next.js --since 2024-06-01T00:00:00Z --author someone
# follow a "(#421)" headline → full PR:
octocode pr bgauryy/octocode#421 --deep
```

### repo

```
repo <keywords...>
    --topic <list>                    comma-separated GitHub topics
    --language <lang>
    --owner <owner>                   owner or organization
    --stars <range>                   e.g. >100, 50..500
    --forks <range>
    --good-first-issues <range>
    --license <spdx>                  e.g. mit, apache-2.0
    --created <range>                 e.g. >2023-01-01
    --updated <range>
    --size <range>                    repository size in KB
    --match name,description,readme
    --sort stars|forks|help-wanted-issues|updated|best-match
    --archived true|false
    --visibility public|private
    --limit <n>
    --page <n>
    --concise                         flat "owner/repo" list
    --verbose                         structured repository objects
    --json
```

Examples:

```bash
octocode repo react state --language TypeScript --stars '>1000'
octocode repo --topic mcp,agents --sort stars --limit 10
octocode repo --owner vercel --language TypeScript --verbose
```

### pkg

```
pkg <package|keywords>
    --mode lean|full    lean (default) or full metadata
    --page <n>          result page for keyword searches
    --json
```

Examples:

```bash
octocode pkg zod
octocode pkg @modelcontextprotocol/sdk
octocode pkg "http client typescript"
```

> **Symbol outlines** moved to `ls` — `ls <file>` or `ls <dir> --symbols` (see the `ls` section). The standalone `symbols` command was removed.

### lsp

```
lsp <file> --type <type> --symbol <name> --line <n>
    --repo <owner/repo[@ref]>   materialize remote repo first; file is repo-relative
    --branch <ref>              branch for --repo materialization
    --force-refresh             re-clone --repo materialization
    --type   definition|references|callers|callees|callHierarchy
             hover|typeDefinition|implementation   (required)
    --symbol <name>             required
    --line <n>                  required
    --workspace-root <path>
    --format structured|compact
    --context-lines <n>
    --depth <n>                 call hierarchy depth
    --page <n>
    --page-size <n>
    --json
```

Run `grep` or `ls --symbols` first to get a real `--line` value. Never guess `--line`. With `--repo`, the CLI materializes the remote file locally, defaults `--workspace-root` to the saved repo root, and returns hints with the absolute saved path for follow-up local tools. Semantic misses such as `symbolNotFound`, `noLocations`, `noReferences`, `noHover`, or `noCalls` exit with code `3` so shell scripts can fail fast without parsing JSON.

All raw `lspGetSemantics` types are: `definition`, `references`, `callers`, `callees`, `callHierarchy`, `hover`, `documentSymbols`, `typeDefinition`, and `implementation`. The CLI `lsp` shortcut is for symbol-anchored types that require `--symbol` and `--line`; use `octocode ls <file|dir> --symbols` for `documentSymbols`. For TypeScript/JavaScript import aliases, `definition` follows local imports to the exported declaration when the language server first returns the import binding.

Examples:

```bash
octocode ls packages/octocode/src/cli/index.ts --symbols
octocode lsp packages/octocode/src/cli/index.ts --type references --symbol runCLI --line 73
octocode lsp packages/react/src/ReactHooks.js --repo facebook/react --type references --symbol useState --line 72
octocode lsp packages/octocode/src/index.ts --type definition --symbol runCLI --line 10 --format compact
octocode lsp packages/octocode/src/cli/index.ts --type hover --symbol runCLI --line 73
```

### binary

```
binary <file>
    (no flags)           auto-detect mode from extension
    --inspect            structure of a native binary: format, arch, symbols, imports, exports, sections, deps
    --list               list archive entries
    --extract <entry>    extract one archive member (exact path from --list)
    --strings            readable strings from a native binary
    --decompress         decompress a single-stream file
    --match <s>          filter extracted/decompressed lines
    --min-length <n>     strings: shortest run to keep (default 8)
    --max-entries <n>    list: cap entries
    --format <fmt>       decompress: force compression format
    --verbose            list: include size and mtime
    --offsets            strings: prefix each with hex byte offset (absolute)
    --scan-offset <n>    strings: byte offset to start the scan window — follow
                         the nextScanOffset cursor to page a large binary
                         losslessly (no string is split across windows)
    --char-offset <n>    strings/decompress/extract text continuation offset
    --char-length <n>    strings/decompress/extract text window length
    --page <n>
    --json
```

Inspection is fully native (no `file`/`xxd`/`strings`/binutils needed). Native binaries and unrecognized files default to `--inspect`; use `--list` before `--extract` — do not guess entry names.

`--strings` scans a 64MB window at a time and never discards the tail of a large binary: when more remains it prints a `nextScanOffset` cursor (and an `⚠` continuation hint to stderr). Re-run with `--scan-offset <n>` to keep scanning; the window is rewound to a safe break so no string is ever split across the boundary.

Examples:

```bash
octocode binary archive.zip --list
octocode binary archive.zip --extract src/index.ts
octocode binary release.tar.gz --decompress
octocode binary lib.node                 # inspect: format, symbols, deps
octocode binary lib.node --strings --min-length 12
octocode binary huge.bin --strings --scan-offset 67108863   # page past the first 64MB window
```

### unzip

Unpack an archive to a fresh `<octocode-home>/tmp/unzip/<name>-<timestamp>/` directory, then use local commands on the extracted tree. The command returns `localPath`; use that exact path for follow-up `ls`, `grep`, `cat`, and `lsp` calls.

```
unzip <archive> [--json]
```

Examples:

```bash
octocode unzip app.zip
octocode unzip release.tar.gz
octocode unzip app.zip --json
octocode ls <localPath-from-unzip-output>
```

## Management Commands

### install

```
install --ide <client> [--method npx] [--force] [--check] [--rollback] [--backup-path <path>] [--json]
```

Configures the MCP server for an IDE. `--check` does a pre-flight only. `--rollback` restores the most recent backup.

Supported `--ide` values: `cursor`, `claude-desktop`, `claude-code`, `windsurf`, `zed`, `vscode-cline`, `vscode-roo`, `vscode-continue`, `opencode`, `trae`, `antigravity`, `codex`, `gemini-cli`, `goose`, `kiro`

### auth

```
auth [login|logout|refresh] [--hostname <host>] [--json]

login   [--hostname <host>] [--git-protocol ssh|https] [--force] [--json]
logout  [--hostname <host>] [--yes] [--json]
```

GitHub OAuth authentication. `login` opens the device flow. `logout` removes encrypted credentials. `refresh` refreshes stored Octocode credentials when possible. `--hostname` targets GitHub Enterprise.

### status

```
status [--hostname <host>] [--sync] [--json]
```

Shows auth state, token presence/source, MCP client install health, and cache info. `--sync` adds cross-client MCP sync analysis.

## Tool Runner

`octocode tools` uses the canonical direct-tool catalog from `@octocodeai/octocode-tools-core/direct`; the CLI does not maintain separate tool schemas.

Raw tool calls use `--queries`. Legacy `--input` is not supported, and unsupported tool flags are rejected before execution.

`octocode context [--full] [--json]` prints the same system prompt, protocol, quick-command guidance, and tool descriptions used by the CLI help surface. `--json` wraps the text as `{ "context": "..." }`.

`--queries` accepts an object, array, or wrapped object:

```json
{ "path": ".", "keywords": "runCLI" }
[{ "path": ".", "keywords": "runCLI" }]
{ "queries": [{ "path": ".", "keywords": "runCLI" }] }
```

Direct CLI runs auto-fill required context fields when omitted.

### Tools

| Category | Tools |
|----------|-------|
| GitHub | `ghSearchCode`, `ghGetFileContent`, `ghViewRepoStructure`, `ghSearchRepos`, `ghHistoryResearch`, `ghCloneRepo` |
| Local | `localSearchCode`, `localViewStructure`, `localFindFiles`, `localGetFileContent`, `localBinaryInspect` |
| LSP | `lspGetSemantics` |
| Package | `npmSearch` |

Examples:

```bash
octocode tools
octocode tools localSearchCode --scheme
octocode tools localSearchCode --queries '{"path":".","keywords":"runCLI"}'
octocode tools ghSearchCode --queries '{"keywordsToSearch":["useReducer"],"owner":"facebook","repo":"react"}'
octocode tools ghCloneRepo --queries '{"owner":"facebook","repo":"react","sparsePath":"packages/react"}'
```

## Environment

| Variable | Meaning |
|----------|---------|
| `OCTOCODE_TOKEN` | Highest-priority GitHub token. |
| `GH_TOKEN` | GitHub CLI compatible token. |
| `GITHUB_TOKEN` | GitHub token fallback. |
| `OCTOCODE_HOME` | Override Octocode data directory. Defaults: macOS `~/.octocode`, Windows `%APPDATA%\octocode`, Linux `${XDG_CONFIG_HOME:-~/.config}/octocode`. |
| `ENABLE_LOCAL` | Enable local filesystem tools (default: `true`). |
| `ENABLE_CLONE` | Clone gate. CLI clone/materialization is enabled by default and only disabled by `ENABLE_CLONE=false`; MCP clone tools require `ENABLE_CLONE=true`. |
| `NO_COLOR` | Disable terminal color. |

## Exit Codes

| Code | Meaning |
|------|---------|
| `0` | Success. |
| `1` | General error. |
| `2` | Invalid input or unsupported flags. |
| `3` | Not found: unknown tool/command, missing symbol, or empty semantic result. |
| `4` | Authentication failure. |
| `5` | Tool/API execution error. |
| `7` | Rate limited. |
