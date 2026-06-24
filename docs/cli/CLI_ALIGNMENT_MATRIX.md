# CLI Search Alignment Matrix

Source-verified 2026-06-24 against the built `octocode search --scheme` surface and the `@octocodeai/octocode-core` CLI resources.

`search` is the single read-only CLI entry point. The named workflow commands that remain outside it are `clone`, `cache`, and `unzip`; setup/management commands (`install`, `skill`, `auth`, `login`, `logout`, `status`, `tools`, `context`) also remain.

Removed read-only commands: `cat`, `ls`, `find`, `diff`, `history`, `repo`, `pkg`, `binary`, `grep`, `lsp`, and `pr`.

## Capability Map

| Capability | Canonical CLI | OQL target | Backing tool/lane |
|---|---|---|---|
| Text search | `search "term" <path\|owner/repo> --lang ts` | `code` | `localSearchCode` / `ghSearchCode` |
| Regex search | `search --regex "<re>" <path> [--pcre2]` | `code` | `localSearchCode` |
| AST search | `search --pattern "<ast>" <path> --lang ts` / `--rule` | `code` | structural search in `octocode-engine` |
| File discovery | `search <query> <path> --search path` | `files` | `localFindFiles` / file predicates |
| Content read | `search <file> --content-view exact\|compact\|symbols` | `content` | `localGetFileContent` / `ghGetFileContent` |
| Directory tree | `search <dir\|owner/repo> --tree` | `structure` | `localViewStructure` / `ghViewRepoStructure` |
| Symbol outline | `search <file> --symbols` or `--op documentSymbols` | `semantics` | `lspGetSemantics` |
| LSP navigation | `search <file> --op references --symbol X --line N` | `semantics` | `lspGetSemantics` |
| Repository discovery | `search <keywords> --target repositories --lang TypeScript` | `repositories` | `ghSearchRepos` |
| Pull requests | `search owner/repo --target pullRequests`, `search owner/repo#N --target pullRequests`, or `search <PR-URL> --target pullRequests` | `pullRequests` | `ghHistoryResearch` |
| Commit history | `search owner/repo[/path] --target commits --patches` | `commits` | `ghHistoryResearch` |
| npm packages | `search zod --target packages` (full via OQL `params.mode:"full"`) | `packages` | `npmSearch` |
| Artifact inspection | `search <file> --target artifacts --inspect\|--list\|--strings` | `artifacts` | `localBinaryInspect` |
| Explicit file diff | `search <left> <right> --target diff` | `diff` | OQL diff lane |
| Dead-code research | `search --query '{"target":"research",...}'` | `research` | OQL research analysis |
| Proof graph | `search --query '{"target":"graph",...}'` | `graph` | OQL graph/LSP proof |
| Remote materialization | `search --query '{"target":"materialize",...}'` or `cache fetch` | `materialize` | `ghCloneRepo` / cache |

## Canonical Flag Names

Use one spelling per concept:

| Concept | Use | Do not reintroduce |
|---|---|---|
| Language / extension scope | `--lang` | `--type`, `--language` |
| LSP operation | `--op` | `--type` |
| Code/file result density | `--view discovery\|paginated\|detailed` | grep-style `--mode` |
| Content view | `--content-view exact\|compact\|symbols` (package `full` via OQL `params.mode`) | `--mode`, `--minify`, package `--full` |
| Literal text search | `--fixed` | `--fixed-string` |
| PCRE2 regex | `--pcre2` | `--perl-regex` |
| Per-file match cap | `--max-matches` | `--max-matches-per-file` |
| Directory-only discovery | `--entry directory` | `--dirs-only` |
| Reverse sort | `--sort-reverse` | `--reverse` |
| Commit diffs | `--patches` | `--diff` |

## Follow-Up Flows

| Goal | Flow |
|---|---|
| Orient then read | `search <dir> --tree` -> `search <file> --content-view symbols` -> `search <file> --content-view exact` |
| Find usage then prove | `search "term" <path> --view discovery` -> `search <file> --match-string "term" --content-view exact` |
| Remote repo proof | `search "term" <subpath> --repo owner/repo --lang ts` or `cache fetch owner/repo <path>` -> local `search` follow-ups |
| Symbol navigation | `search <file> --op documentSymbols` -> `search <file> --op references --symbol X --line N` |
| Package to source | `search <package> --target packages` -> `search "symbol" owner/repo` or `cache fetch owner/repo` |
| PR context | `search owner/repo/path --target commits` -> `search owner/repo#N --target pullRequests --patches --comments` |
| Archive research | `unzip archive.zip` or `search archive.zip --target artifacts --list` -> `search <localPath> --tree` |

## Verification Checklist

- `octocode --help` lists `search`, `clone`, `cache`, `unzip`, `skill`, and management commands only.
- `octocode <removed-command> --help` returns unknown command.
- `octocode search --scheme` lists active targets: `code`, `content`, `structure`, `files`, `semantics`, `repositories`, `packages`, `pullRequests`, `commits`, `artifacts`, `diff`, `research`, `graph`, and `materialize`.
- CLI resources in `octocode-core` expose no removed command specs.
- Docs and skills use only live commands in copy-pastable snippets.
