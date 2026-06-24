# CLI Search Unification Status

Status: completed 2026-06-24.

`octocode search` is the canonical read-only CLI surface. It is backed by OQL and covers code search, content reads, structure, file discovery, semantics, repositories, packages, PRs, commits, artifacts, diffs, research, graph proof, and materialization.

## Removed Commands

The following read-only shortcuts have been removed from the active CLI registry and from `@octocodeai/octocode-core` CLI resources:

- `cat`
- `ls`
- `find`
- `diff`
- `history`
- `repo`
- `pkg`
- `binary`
- `grep`
- `lsp`

Use these replacements. The left column names the removed shortcut class, not a
callable command form:

| Removed shortcut | Replacement |
|---|---|
| Content read shortcut | `search <file> --content-view exact\|compact\|symbols` |
| Tree/structure shortcut | `search <dir> --tree` |
| File-symbol shortcut | `search <file> --symbols` or `search <file> --op documentSymbols` |
| File-discovery shortcut | `search <query> <path> --search path` or `--target files` |
| Text-search shortcut | `search <term> <path> --lang <lang>` |
| Semantic shortcut | `search <file> --op references --symbol <name> --line <n>` |
| Repository-discovery shortcut | `search <keywords> --target repositories` |
| Commit-history shortcut | `search owner/repo/path --target commits` |
| Package shortcut | `search <name> --target packages` |
| Artifact shortcut | `search <file> --target artifacts --inspect\|--list\|--strings` |
| Diff shortcut | `search <left> <right> --target diff` |

The workflow commands that remain outside `search` are:

- `clone` for explicit Git clone/subtree workflows.
- `cache` for materialization/cache status/clear workflows.
- `unzip` for archive unpacking to a local tree.

Setup and management commands also remain outside `search`: `install`, `skill`, `auth`, `login`, `logout`, `status`, `tools`, and `context`.

## Canonical CLI Spellings

To keep the agent-facing surface simple, do not reintroduce duplicate aliases:

| Concept | Canonical flag |
|---|---|
| Language/extension scope | `--lang` |
| LSP operation | `--op` |
| Code/files result density | `--view` |
| Content view / package detail | `--content-view`; OQL `params.mode` for package `lean`/`full` |
| Literal text search | `--fixed` |
| PCRE2 regex | `--pcre2` |
| Per-file match cap | `--max-matches` |
| Directory-only filtering | `--entry directory` |
| Reverse sort | `--sort-reverse` |
| Commit/PR diff content | `--patches` |

## GitHub Index Misses

GitHub code search can return zero results for indexed-provider reasons,
especially in large repos such as `facebook/react`. Treat `providerUnindexed`
as a routing hint, not absence:

```bash
node packages/octocode/out/octocode.js search facebook/react/packages/react/src --tree --depth 2
node packages/octocode/out/octocode.js search useState packages/react/src --repo facebook/react --materialize required --lang js
node packages/octocode/out/octocode.js clone facebook/react/packages/react/src --json
node packages/octocode/out/octocode.js cache fetch facebook/react packages/react/src --depth tree --json
```

For a single remote file use `cache fetch owner/repo path --depth file`; for an
intentional whole-repo checkout use `clone owner/repo` or
`cache fetch owner/repo --depth clone`.

## Validation Commands

Run from the monorepo root after rebuilding `octocode-core` and this repo:

```bash
node packages/octocode/out/octocode.js --help
node packages/octocode/out/octocode.js search --scheme
for removed in cat ls find diff history repo pkg binary grep lsp pr token skills; do
  ! node packages/octocode/out/octocode.js "$removed" --help
done
node packages/octocode/out/octocode.js search package.json --content-view exact --json
node packages/octocode/out/octocode.js search packages/octocode/src --tree --json
node packages/octocode/out/octocode.js search parser packages/octocode/src --search path --ext ts --json
node packages/octocode/out/octocode.js search zod --target packages --json
```

For docs, run the CLI command-documentation tests and scan copy-pastable snippets:

```bash
yarn workspace octocode test tests/cli/skill-doc-commands.test.ts tests/cli/help-modules.test.ts
rg -n "\\boctocode\\s+(cat|ls|find|diff|history|repo|pkg|binary|grep|lsp|pr|token|skills)\\b" README.md docs .agents/skills
```

## Source Locations

| Surface | Source |
|---|---|
| CLI handler/flags | `packages/octocode/src/cli/commands/search.ts` |
| CLI registry | `packages/octocode/src/cli/commands/index.ts` |
| CLI static resources | sibling `octocode-mcp-host/packages/octocode-core/src/resources/cli/` |
| OQL schema text | `packages/octocode-tools-core/src/oql/schemeText.ts` |
| OQL shorthand lowering | `packages/octocode-tools-core/src/oql/shorthand.ts` |
| Reference docs | `docs/cli/REFERENCE.md` and `docs/cli/CLI_ALIGNMENT_MATRIX.md` |

When editing `octocode-core` resources locally, rebuild that package and refresh the file dependency before testing this repo:

```bash
(cd ../octocode-mcp-host/packages/octocode-core && yarn build)
yarn install
yarn build
```
