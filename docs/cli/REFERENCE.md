# Octocode CLI Reference

`octocode` is the terminal interface for code research:

- Run all Octocode MCP tools directly from the shell.
- Use smart quick commands for files, trees, search/OQL, PRs, package lookup, LSP workflows, and repo cloning.
- Use raw `tools` for lower-level GitHub/local research and exact schema-driven calls.
- Manage Octocode setup for IDEs when needed.

## Usage

```bash
octocode <command> [options]

# Quick research commands
octocode search <keywords> <path|owner/repo>          # text/regex/AST, file discovery, tree/content/semantics; --scheme for OQL
octocode search <path|owner/repo> --tree              # directory tree
octocode search <file> --content-view exact           # exact file read
octocode search <file> --symbols                      # symbol outline
octocode search <owner/repo[#N]|PR-URL> --target pullRequests
octocode clone <owner/repo[/path][@branch]|url>
octocode cache fetch <owner/repo[@ref]> [path]
octocode search <keywords...> --target repositories
octocode search <package> --target packages
octocode search <file> --op references --symbol <name> --line <n>
octocode search <file> --target artifacts --inspect
octocode unzip <archive>

# Raw tool runner
octocode tools                                # all tools, concise descriptions
octocode tools --json --compact               # lean minified machine catalog
octocode tools --json --full                  # wrapped full all-tool schema catalog
octocode tools <name> --scheme
octocode tools <name> --scheme --json --compact # one minified machine schema
octocode tools <name> --queries '<json>' --compact
octocode context [--full] [--json]

# Agent setup
octocode skill (--add <github-path> | --name <octocode-skill> | --install-all) [--platform common|cursor|claude|codex|opencode|pi|copilot|gemini|all] [--mode symlink|copy|hybrid] [--force|--update] [--json]
```

## Command Index

| Command | Purpose |
|---------|---------|
| `search` | Search text/regex/AST, read content, browse trees, run semantic/OQL targets, and discover files by name/path/metadata. |
| `unzip` | Unpack an archive to `<octocode-home>/tmp/unzip/<name>-<timestamp>/`. |
| `clone` | Clone a GitHub repo or sparse subtree to `<octocode-home>/tmp/clone/`. |
| `cache` | Materialize remote files/trees/repos, inspect status, or clear tmp storage. |
| `tools` | List tools, read schemas, and run any MCP tool directly. |
| `context` | Print agent-facing protocol, system prompt, tool descriptions, and schemas. |
| `skill` | Fetch one GitHub Agent Skill folder, a GitHub skills library, one named Octocode skill, or all Octocode skills and install into Common, Cursor, Claude, Codex, OpenCode, Pi, GitHub Copilot, Gemini CLI, or all supported skill destinations. |
| `install` | Configure Octocode in supported MCP clients. |
| `auth` | Run auth subcommands: `login`, `logout`, `refresh`, or read-only `status`. |
| `login` / `logout` | Open the interactive auth picker or clear stored GitHub credentials directly. |
| `status` | Show token presence/source, auth identity, MCP install state, sync info, and cache paths. |

Removed commands: `cat`, `ls`, `find`, `diff`, `history`, `repo`, `pkg`, `binary`, `grep`, `lsp`, `pr`, `token`, and plural `skills`. Use `search <file>` / `target:"content"` for reads, `search <dir> --tree` for structure, `search --search path|both` or `target:"files"` for file discovery, `search --target commits` for commit history, `search --target pullRequests` for PR list/deep-read flows, `search --target repositories` for repository discovery, `search --target packages` for npm package lookup, `search --target artifacts` for binary/archive inspection, `search --target diff` for explicit file diffs, `search --op ...` for LSP semantics, singular `skill --add ... --platform ...` for agent skill installs, `auth status --json` for script-safe token/auth state, or `status --json` for the broader CLI/MCP/cache status envelope.

## Agent Flow

Agents should use this order:

1. `octocode context`
2. `octocode tools --json --compact`
3. `octocode tools <name> --scheme --json --compact`
4. `octocode tools <name> --queries '<json>' --compact`

Use `octocode context --full` for complete tool descriptions, and `octocode context --json` when automation needs a machine-readable `{ "context": "..." }` wrapper. Bare `octocode tools --json` is a lean discovery catalog; add `--compact` to minify it for agents. Read one full schema on demand with `octocode tools <name> --scheme --json --compact`. Use `octocode tools --json --full` only when automation truly needs every schema in one payload.

## UX Map

| Need | Use |
|------|-----|
| Map files and repos | `search --tree`, `search --search path`, `search --target repositories`, `search --target packages` |
| Search text, file metadata, or code structure | `search` (text/regex; `--search path` / `--target files` for file discovery; `--pattern`/`--rule` for structural AST/code shape) |
| Read less, cite exact evidence | `search <file> --content-view symbols`, `search <file> --match-string`, `search <file> --start-line ... --end-line ...` |
| Outline a file / trace symbols semantically | `search <file> --op documentSymbols` or `search <file> --symbols`, then `search <file> --op ... --symbol ... --line ...`; structural `search` matches can also provide anchors |
| Inspect PRs/history or clone for local analysis | `search --target pullRequests`, `search --target commits`, `clone`, `cache fetch`, or `search --repo` |
| Inspect archives/binaries | `search --target artifacts`, `unzip` |
| Install Agent Skills into clients | `skill (--add <github-path> | --name <octocode-skill> | --install-all) --platform common|cursor|claude|codex|opencode|pi|copilot|gemini|all` |
| Configure Octocode | `install`, `auth`, `login`, `logout`, `status` |
| Run any MCP tool directly | `tools --json --compact`, then `tools <name> --scheme --json --compact`, then `tools <name> --queries '<json>' --compact` |

## Global Options

| Option | Meaning |
|--------|---------|
| `--help` | Show help. |
| `--version` | Show version. |
| `--json` | Print JSON output. For tool runs this is the raw MCP envelope; for bare `tools --json` this is the lean catalog. |
| `--compact` | Print lean tool output. |
| `--no-color` | Disable ANSI color. Also honors `NO_COLOR=1`. |

Unknown flags are rejected before a command runs. The error lists valid flags for that command and suggests near-miss typos.

## Quick Commands

Auto-route based on target: a local path routes to local tools; `owner/repo[/path][@branch]` routes to GitHub. `search` is the canonical read-only OQL route. With `--repo owner/repo[@ref]`, `search` first materializes the remote target under Octocode's `.octocode` storage, then runs the local tool and returns absolute local-path hints. All commands support `--json`.

| Command | Routes to | What it does |
|---------|-----------|------|
| `search` | OQL over `localSearchCode` / `localFindFiles` / `localGetFileContent` / `localViewStructure` / `lspGetSemantics` / GitHub/npm/binary/history backends | Text/regex/AST search, file discovery, content reads, tree browsing, LSP semantics, and full typed OQL |
| `clone` | `ghCloneRepo` | Clone a repo or subtree to the Octocode home `tmp/clone/` cache |
| `cache` | `ghGetFileContent` / `ghCloneRepo` | Materialize a remote file/tree/repo, report storage status, or clear tmp outputs |
| `unzip` | `localBinaryInspect` (unpack) | Unpack an archive to a fresh `<octocode-home>/tmp/unzip/<name>-<timestamp>/` directory |

## Minimize First

Use the CLI in this order: map cheaply, search narrowly, then read the smallest proof slice.

- `--compact` trims CLI rendering; `--json` returns the raw envelope when automation needs it.
- `--concise` returns path/title-only discovery lists for search-style commands.
- `search <file> --content-view symbols` gives a line-numbered skeleton before reading bodies.
- `search <file> --match-string`, `--start-line`, and `--end-line` keep evidence reads small and quotable.
- `search --view discovery` finds files/paths cheaply; switch to paginated/detailed only after narrowing.
- For remote multi-step work, prefer `search --repo owner/repo[@ref]` or `cache fetch`: output hints include the absolute saved path so the next step can use local `search --tree`, `search <file>`, and `search --op`.

### search content and structure

`search` auto-routes a lone file to `target:"content"` and a lone directory or `owner/repo` to `target:"structure"`. Use `--content-view exact` for exact text, `--content-view compact` for compact code, and `--content-view symbols` for skeletons. Use `--tree` when you want a directory tree, and `--symbols` / `--op documentSymbols` when you want an LSP symbol outline.

```bash
octocode search src/index.ts --content-view exact
octocode search src/index.ts --content-view symbols
octocode search src/index.ts --match-string "runCLI" --content-view exact
octocode search src/index.ts --start-line 40 --end-line 90 --content-view exact
octocode search facebook/react/packages/react/index.js --content-view exact
octocode search packages/react/index.js --repo facebook/react --content-view symbols
octocode search src --tree
octocode search src --tree --depth 3 --ext ts,tsx
octocode search facebook/react --tree --branch 18.2.0 --depth 2
octocode search packages/react --repo facebook/react --tree --depth 2
octocode search src/index.ts --symbols
octocode search src --symbols --ext ts,tsx --limit 10
octocode search packages/react/index.js --repo facebook/react --symbols
octocode search src/index.ts --symbols --kind function
```

### search file discovery

The former `find` command is removed. Use `search` with `--search path`,
`--target files`, or full OQL for file discovery and file metadata filters.

```
search <query> [path|owner/repo]
    --search path|content|both      path = file rows; content = content predicate; both = independent files/code batch
    --target files|code             files returns file rows; code returns line/snippet rows
    --repo <owner/repo[@ref]>       use a GitHub corpus; materialize with --materialize auto|required when exact local proof is needed
    --source local|github|npm       force corpus interpretation when shorthand is ambiguous
    --path <subpath>                local root or GitHub repo subpath override; prefer positional paths when possible
    --ext <ext>                     extension filter, e.g. ts
    --lang <ext|language>           content/code scope, e.g. ts, typescript
    --name <glob> / --filename <glob>
    --path-pattern <glob>
    --regex <pattern>               path regex when paired with --search path
    --entry file|directory
    --min-depth <n> / --max-depth <n>
    --modified-within <window>      e.g. 7d, 2h, 1w
    --modified-before <window>
    --accessed-within <window>
    --size-greater <size> / --size-less <size>
    --permissions <mode>
    --executable / --readable / --writable
    --empty
    --exclude-dir <list>
    --details / --show-modified
    --limit <n> / --page <n> / --items-per-page <n>
    --json
```

Examples:

```bash
octocode search auth src --search path --ext ts
octocode search executeDirectTool . --target code --lang ts
octocode search useState --repo facebook/react --path packages/react --search both --ext js
octocode search auth bgauryy/octocode-mcp --search path --ext ts
octocode search auth . --search both --limit 20
octocode search --query '{"target":"files","from":{"kind":"local","path":"./src"},"where":{"kind":"all","of":[{"kind":"field","field":"extension","op":"=","value":"ts"},{"kind":"field","field":"basename","op":"glob","value":"*auth*"}]}}'
```

GitHub code search can return zero rows because the provider has not indexed a
repo/path. That is not absence. Recover with bounded local proof:

```bash
octocode search facebook/react/packages/react/src --tree --depth 2
octocode search useState packages/react/src --repo facebook/react --materialize required --lang js
octocode clone facebook/react/packages/react/src
octocode cache fetch facebook/react packages/react/src --depth tree
```

Use `cache fetch owner/repo path --depth file` for one remote file, or
`clone owner/repo` / `cache fetch owner/repo --depth clone` only when deliberate
whole-repo checkout is acceptable.

> **Structural AST search** lives under `search --pattern`/`--rule` (see the `search` section). Use those matches as anchors for `search --op` when symbol identity or references matter. The standalone `ast` command was removed.

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
octocode search <localPath-from-clone-output> --tree
octocode search "useState" <localPath-from-clone-output>
octocode search <localPath-from-clone-output>/packages/react/index.js --content-view exact
octocode search <localPath-from-clone-output>/packages/react/index.js --symbols
```

For one-step remote-as-local research, use `search` with `--repo owner/repo[@ref]` and, when exact local proof is needed, `--materialize auto|required`. If GitHub code search reports `providerUnindexed`, prefer a bounded subtree such as `packages/react/src` before using whole-repo clone/cache workflows.

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
      octocode search <localPath> --tree
      octocode search useState <localPath> --search both
      octocode search <localPath>/index.js --content-view exact
      octocode search <localPath>/index.js --op references --symbol useState --line <lineHint>
      ```

### search pull requests

```
search <owner/repo[#N]|PR-URL> --target pullRequests
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
    --items-per-page <n>
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
octocode search facebook/react --target pullRequests
octocode search facebook/react --target pullRequests --state merged --limit 20
octocode search facebook/react#29940 --target pullRequests --patches --comments
octocode search https://github.com/vercel/next.js/pull/65000 --target pullRequests --deep
```

### search commits

Use `search --target commits` for commit history on a GitHub repo, directory, or file. A commit headline that embeds `(#NNN)` links to its PR: deep-read it with `search owner/repo#NNN --target pullRequests`.

```
search <owner/repo[/path][@branch]> --target commits
    --since <iso>        ISO 8601, e.g. 2024-01-01T00:00:00Z
    --until <iso>
    --author <name>      filter by commit author
    --branch <ref>       branch or SHA to walk (also parsed from @branch)
    --patches            include per-commit file diffs (larger output)
    --limit <n>          max commits shown
    --page <n>
    --items-per-page <n>
    --json
```

Examples:

```bash
octocode search facebook/react/packages/react/src --target commits
octocode search bgauryy/octocode/README.md --target commits --patches
octocode search vercel/next.js --target commits --since 2024-06-01T00:00:00Z --author someone
# follow a "(#421)" headline → full PR:
octocode search bgauryy/octocode#421 --target pullRequests --deep
```

### search repositories

```
search <keywords...> --target repositories
    --topic <list>                    comma-separated GitHub topics
    --lang <lang>
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
octocode search "react state" --target repositories --lang TypeScript --stars '>1000'
octocode search --target repositories --topic mcp,agents --sort stars --limit 10
octocode search --target repositories --owner vercel --lang TypeScript --verbose
```

### search packages

The former `pkg` command is removed. Use `search --target packages`, which
routes through `target:"packages"` / `npmSearch` and returns the source
repository handoff when available.

```
search <package|keywords> --target packages
    --page <n>          result page for keyword searches
    --json
```

Examples:

```bash
octocode search zod --target packages
octocode search --query '{"target":"packages","from":{"kind":"npm"},"params":{"packageName":"@modelcontextprotocol/sdk","mode":"full"}}'
octocode search "http client typescript" --target packages --page 1
```

### search semantics

The former `lsp` command is removed. Use `search --op`, which routes through
`target:"semantics"` / `lspGetSemantics`.

```
search <file> --op <type> [--symbol <name>] [--line <n>]
    --repo <owner/repo[@ref]>   materialize remote repo first; file is repo-relative
    --branch <ref>              branch for --repo materialization
    --force-refresh             re-clone --repo materialization
    --op documentSymbols|diagnostic|definition|references|callers|callees|callHierarchy|hover|typeDefinition|implementation|workspaceSymbol|supertypes|subtypes
    --symbol <name>             required for position-anchored ops
    --line <n>                  required in practice for position-anchored ops
    --workspace-root <path>
    --format structured|compact
    --depth <n>                 call hierarchy depth
    --page <n>
    --items-per-page <n>
    --json
```

Run `search <file> --op documentSymbols` or `search <file> --symbols` first to get a real
`--line` value. Never guess `--line`. With `--repo`, the CLI materializes the
remote file locally and returns location hints with the absolute saved path.

Examples:

```bash
octocode search packages/octocode/src/cli/index.ts --op documentSymbols
octocode search packages/octocode/src/cli/index.ts --op references --symbol runCLI --line 73
octocode search packages/react/src/ReactHooks.js --repo facebook/react --op references --symbol useState --line 72
octocode search packages/octocode/src/index.ts --op definition --symbol runCLI --line 10 --format compact
octocode search packages/octocode/src/cli/index.ts --op hover --symbol runCLI --line 73
```

### search artifacts

```
search <file> --target artifacts
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
octocode search archive.zip --target artifacts --list
octocode search archive.zip --target artifacts --extract src/index.ts
octocode search release.tar.gz --target artifacts --decompress
octocode search lib.node --target artifacts --inspect
octocode search lib.node --target artifacts --strings --min-length 12
octocode search huge.bin --target artifacts --strings --scan-offset 67108863   # page past the first 64MB window
```

### unzip

Unpack an archive to a fresh `<octocode-home>/tmp/unzip/<name>-<timestamp>/` directory, then use local commands on the extracted tree. The command returns `localPath`; use that exact path for follow-up `search --tree`, `search`, `search <file>`, and `search --op` calls.

```
unzip <archive> [--json]
```

Examples:

```bash
octocode unzip app.zip
octocode unzip release.tar.gz
octocode unzip app.zip --json
octocode search <localPath-from-unzip-output> --tree
```

## Management Commands

### skill

Install one GitHub Agent Skill folder, every skill in a GitHub skills library path, one named Octocode skill, or every official Octocode skill into deterministic skill destinations. This command is built for agents and scripts: it defaults to `common`, accepts explicit destinations with `--platform`, `--target` is an alias, refreshes sources under `~/.octocode/skills`, and never opens an interactive chooser.

```
skill (--add <github-path> | --name <octocode-skill> | --install-all) [--platform common|cursor|claude|codex|opencode|pi|copilot|gemini|all] [--branch <ref>] [--mode symlink|copy|hybrid] [--force|--update] [--json]
```

Accepted GitHub path forms:

| Input | Meaning |
|-------|---------|
| `owner/repo/skills` | Discover and install every direct child skill folder in that library |
| `owner/repo/skills/my-skill` | Fetch from the default branch |
| `owner/repo@main/skills/my-skill` | Fetch from an explicit branch/tag/SHA |
| `https://github.com/owner/repo/tree/main/skills/my-skill` | Fetch the exact GitHub tree URL |
| `https://github.com/owner/repo/blob/main/skills/my-skill/SKILL.md` | Resolve the containing folder |

Supported platforms: `common`, `cursor`, `claude`, `codex`, `opencode`, `pi`, `copilot`, `gemini`, and `all`. Default platform: `common`. Default mode: `symlink`, which refreshes `~/.octocode/skills/<skill>` and links selected clients to it. Use `copy` for independent destination folders or `hybrid` to copy Claude targets and symlink the rest. The canonical destination table, with one `npx octocode` install command per location, lives in the [Skills Guide](https://github.com/bgauryy/octocode/blob/main/docs/SKILLS_GUIDE.md#platforms).

Examples:

```bash
npx octocode skill --name octocode-research
npx octocode skill --name octocode-research --platform pi
npx octocode skill --name octocode-research --platform copilot,gemini
npx octocode skill --add bgauryy/octocode@main/skills/octocode-research --platform cursor,codex --json
npx octocode skill --add owner/repo/skills --platform common
npx octocode skill --install-all --platform pi
```

Agent-safe failure behavior:

| Case | Exit |
|------|------|
| Missing source, invalid platform, invalid mode, or invalid GitHub path | `2` |
| GitHub path does not contain `SKILL.md`, GitHub library contains no skills, or the path cannot be fetched | `3` |
| One or more destination installs fail | `1` |

JSON output has one non-redundant shape: `skills[]` entries with source URL, `sourcePath` under `~/.octocode/skills`, concrete targets, and per-skill `summary`, plus top-level `platforms`, `mode`, and aggregate `summary`. Human output includes the selected mode, selected platforms, canonical source path, every destination path, and a final summary. Full DX guide: [Skills Guide](https://github.com/bgauryy/octocode/blob/main/docs/SKILLS_GUIDE.md).

### install

```
install --ide <client> [--method npx] [--force] [--check] [--rollback] [--backup-path <path>] [--json]
```

Configures the MCP server for an IDE. `--check` does a pre-flight only. `--rollback` restores the most recent backup.

Supported `--ide` values: `cursor`, `claude-desktop`, `claude-code`, `windsurf`, `zed`, `vscode-cline`, `vscode-roo`, `vscode-continue`, `opencode`, `trae`, `antigravity`, `codex`, `gemini-cli`, `goose`, `kiro`

### auth

```
auth [login|logout|refresh|status] [--status] [--hostname <host>] [--git-protocol ssh|https] [--force] [--yes] [--json]

login   [--hostname <host>] [--git-protocol ssh|https] [--force] [--json]
logout  [--hostname <host>] [--yes] [--json]
```

GitHub authentication. Interactive `login` / `auth login` opens an auth picker so humans can sign in via Octocode browser OAuth or the `gh` CLI; `--json` and non-TTY login stay deterministic and use Octocode OAuth. `logout` removes encrypted Octocode credentials. `refresh` refreshes stored Octocode credentials when possible; `gh-cli` tokens must be refreshed with `gh auth refresh`. `auth status --json` is the auth-only read probe and exits 0 even when unauthenticated. `--hostname` targets GitHub Enterprise.

### status

```
status [--hostname <host>] [--sync] [--json]
```

Shows auth state, token presence/source, MCP client install health, and cache info. `status --json` exits 0 for read-only inspection, even when no token is configured. `--sync` adds cross-client MCP sync analysis.

## Tool Runner

`octocode tools` uses the canonical direct-tool catalog from `@octocodeai/octocode-tools-core/direct`; the CLI does not maintain separate tool schemas. By default it prints every tool grouped by category with only the tool name and a concise description.

Agent-friendly schema flow:

- `octocode tools --json --compact` prints a lean discovery catalog with tool names, categories, short descriptions, compact field hints, and per-tool schema/run commands. It intentionally omits full field descriptions and nested schemas.
- `octocode tools <name> --scheme --json --compact` prints one machine-readable full schema for the chosen tool.
- `octocode tools --json --full` prints the full all-tool schema catalog as a wrapped JSON payload with `tools[]`; it is intentionally explicit because it is much larger.

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
| Local Code | `localSearchCode`, `localViewStructure`, `localFindFiles`, `localGetFileContent`, `lspGetSemantics`, `localBinaryInspect` |
| Package | `npmSearch` |
| Other | `oqlSearch` |

Examples:

```bash
octocode tools
octocode tools --json --compact
octocode tools localSearchCode --scheme
octocode tools localSearchCode --scheme --json --compact
octocode tools localSearchCode --queries '{"path":".","keywords":"runCLI"}' --compact
octocode tools ghSearchCode --queries '{"keywords":["useReducer"],"owner":"facebook","repo":"react"}' --compact
octocode tools ghCloneRepo --queries '{"owner":"facebook","repo":"react","sparsePath":"packages/react"}' --compact
```

## Environment

| Variable | Meaning |
|----------|---------|
| `OCTOCODE_TOKEN` | Highest-priority GitHub token. |
| `GH_TOKEN` | GitHub CLI compatible token. |
| `GITHUB_TOKEN` | GitHub token fallback. |
| `OCTOCODE_HOME` | Override Octocode data directory. Defaults: macOS `~/.octocode`, Windows `%APPDATA%\octocode`, Linux `${XDG_CONFIG_HOME:-~/.config}/octocode`. |
| `ENABLE_LOCAL` | Enable local filesystem tools (default: `true`; set `false` to disable). |
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
