# CLI Search Unification Plan

> One command — `search` — for all read-only code research via OQL.  
> Side-effecting actions (`clone`, `cache`, `unzip`) stay as named commands.
>
> **Decision:** do the unification, but treat this as a gated migration. The
> current executable CLI is not yet ready for command removal: `octocode-core`
> already documents future `search` flags that the built CLI still rejects.

---

## Scope

Commands that `search` replaces:

| Command | OQL target |
|---|---|
| `grep` | `code` · `files` |
| `cat` | `content` |
| `find` | `files` · `code` (content-backed file discovery) |
| `ls` | `structure` · `semantics` (documentSymbols) |
| `lsp` | `semantics` |
| `repo` | `repositories` |
| `pr` | `pullRequests` · `diff` (PR lane) |
| `history` | `commits` |
| `pkg` | `packages` |
| `binary` (inspect/list/strings/extract/decompress) | `artifacts` |
| `diff` | `diff` (needs local-file lane — see §Blockers) |

Commands that **stay unchanged** (write to disk):  
`clone` · `cache` · `unzip` · any future artifact unpack/write mode

**Hard migration rule:** no quick command or `octocode-core` command spec is
deleted until the executable CLI, `search --help`, `search --scheme`, and the
agent-facing `octocode-core` `search.ts` agree on the same flags and examples.

**OQL source of truth:** the full language scheme, field descriptions, language
selector logic, evidence rules, and required transformer inventory now live in
[`docs/OCTOCODE_QUERY_LANGUAGE.md`](https://github.com/bgauryy/octocode/blob/main/docs/OCTOCODE_QUERY_LANGUAGE.md).
This plan tracks CLI shorthand migration only; do not duplicate the OQL language
contract here.

---

## OQL Language Rating

### Strengths

| Area | Score | Notes |
|---|:---:|---|
| Semantic coverage | 10/10 | 14 active targets; all 11 read-only quick-command operations map cleanly |
| Type safety | 9/10 | Discriminated unions per target; typed `params`, `where`, `fetch` per lane |
| Agent ergonomics | 9/10 | JSON-first, structured output, `next.*` pre-filled continuations, `--explain` routing |
| Orthogonality | 9/10 | `where` / `scope` / `fetch` / `controls` are independent axes |
| Evidence model | 10/10 | `answerReady`, `kind`, `proofGrade` per row — unique in the field |
| Batch | 8/10 | 1–5 independent queries in one call |
| Diagnostics | 9/10 | Typed codes, repair hints, routing transparency via `PUSHDOWN/RESIDUAL/ROUTE` |

### Weaknesses

| Area | Score | Problem |
|---|:---:|---|
| CLI shorthand coverage | 5/10 | `buildSugar()` passes only 9/~50 needed fields today |
| Runtime/context drift | 3/10 | `octocode-core` `search.ts` advertises future flags (`--target`, `--op`, `--raw`, `--limit`, `--mode`) that the built CLI does not parse yet |
| `--type` collision | 4/10 | Same flag means language filter (`ts`) **and** LSP operation (`references`) — confirmed in `shorthand.ts` |
| Target auto-detection | 5/10 | No file-vs-directory routing; shorthand always emits `target:"code"` |
| Schema completeness | 7/10 | `contextLines` and `invertMatch` missing from `controls.search` |
| Local diff | 4/10 | OQL has PR and GitHub two-ref diff lanes; the missing lane is local-file vs local-file |
| `--mode` overload | 5/10 | Means search view in grep context AND content minification in cat context |

### Verdict

**OQL as a research language: 9/10.**  
The schema is complete, typed, and well-designed. It is the right language.

**OQL CLI shorthand today: 5/10.**  
The bottleneck is the shorthand layer, not the language. The `--type` collision and missing
auto-routing are design issues that must be resolved before `search` can absorb the quick commands.

---

## Design Issues to Resolve First

### 0. Runtime/context alignment

Before adding more behavior, align the three surfaces that agents see and run:

| Surface | Required state before removal work |
|---|---|
| `packages/octocode/src/cli/commands/search.ts` | Parses every flag used by the migration examples, including `--target`, `--op`, `--view`, `--mode`, `--raw`, `--limit`, `--page`, `--page-size`, and `--search` |
| `packages/octocode-tools-core/src/oql/shorthand.ts` | Owns shorthand lowering for all absorbed commands, not only text/regex/structural search |
| `octocode-mcp-host/packages/octocode-core/src/resources/cli/search.ts` | Documents only flags the built CLI accepts in the same PR, or is clearly marked as target-state docs |

This phase must include negative smoke tests proving the formerly unknown flags
are now accepted by `search` before any old command is touched.

### 1. `--type` collision

Current state in `shorthand.ts`:
- `--type ts` → `scope.language` / `langType` (language filter for grep/cat)
- `lsp --type references` → `params.type` (LSP operation)

Both land on the same `--type` flag in a unified command. Values are disjoint
(`ts`/`js`/`py` vs `references`/`callers`/`definition`) but conflating them is fragile.

**Resolution:** split into two flags:
- `--lang <ext|language>` — scope language / extension filter (rename from `--type` for grep/cat/ls)
- `--op <lsp-type>` — LSP operation type (`definition`, `references`, `callers`, …)

`--lang` is already a flag (structural engine language for `--pattern`/`--rule`), so the rename
unifies: `--lang ts` means "TypeScript" in both structural and filter contexts.  
`--op` is new and unambiguous.

### 2. Target auto-routing from positional

The shorthand must infer `target` from the positional argument:

| Positional resolves to | Emitted target |
|---|---|
| local file | `content` |
| local directory | `code` (default) or `structure` (with `--tree`) |
| `owner/repo` GitHub ref | `code` (GitHub search) |
| explicit `--target files` | `files` (find/file-discovery replacement) |
| local file + `--op <lsp-type>` | `semantics` |
| local dir + `--tree` | `structure` |

### 3. `--mode` overload

| Context | `--mode` today | Resolution |
|---|---|---|
| grep search view | `paginated\|discovery\|detailed` | rename to `--view` |
| cat content minification | `none\|standard\|symbols` | keep `--mode` |

Values are disjoint; a single flag could self-dispatch by value, but explicit rename is cleaner.

---

## Schema Gaps (must fix before removing `grep` / content-backed `find`)

**File:** `packages/octocode-tools-core/src/oql/types.ts`  
**File:** `packages/octocode-tools-core/src/oql/adapters/local.ts:searchControls()`

| Add to `controls.search` | Passes to `localSearchCode` | `grep` flag |
|---|---|---|
| `contextLines?: number` | `contextLines` | `--context-lines` |
| `invertMatch?: boolean` | `invertMatch` | `--invert-match` |

---

## Blockers (cannot remove until fixed)

| Blocker | Fix |
|---|---|
| **`search` parser/context drift** | Built `search` must accept the target-state flags already described by `octocode-core` before any command can be removed. |
| **`diff` local lane** | `diffLanes.ts` has `prPatch` (GitHub PR) and `directFile` (GitHub two-ref). Add `localFile: { leftPath, rightPath }` → `localGetFileContent` × 2 + line diff in the diff adapter. |

---

## CLI Shorthand — Complete Flag Map

All CLI argument parsing changes live in
`packages/octocode/src/cli/commands/search.ts`; canonical shorthand lowering
lives in `packages/octocode-tools-core/src/oql/shorthand.ts`.

### global routing flags

| Flag | OQL field / behavior |
|---|---|
| `--target <target>` | Explicit target override; required for non-positional targets such as `repositories`, `pullRequests`, `commits`, `packages`, `artifacts`, `diff`, and `files` |
| `--repo <owner/repo[@ref]>` | Remote-as-local materialization input for local-capability targets |
| `--branch <ref>` | `from.ref` / materialization ref |
| `--limit <n>` | `limit` |
| `--page <n>` | `page` |
| `--page-size <n>` | `itemsPerPage` or content character window, depending on target |
| `--json` / `--compact` | renderer only |

### grep → `target:"code"` / `target:"files"`

| Flag | OQL field | Schema gap? |
|---|---|:---:|
| `--case-insensitive` | `where.text.case:"insensitive"` | |
| `--case-sensitive` | `where.text.case:"sensitive"` | |
| `--whole-word` | `where.text.wholeWord:true` | |
| `--fixed` | text predicate (already literal; alias) | |
| `--multiline` | `where.regex.multiline:true` | |
| `--multiline-dotall` | `where.regex.dotAll:true` | |
| `--files-only` | `filesOnly:true` | |
| `--files-without-match` | `filesWithoutMatch:true` | |
| `--concise` | `view:"discovery"` | |
| `--view paginated\|discovery\|detailed` | `view` | |
| `--count-lines` | `controls.search.countLinesPerFile` | |
| `--count-matches` | `controls.search.countMatchesPerFile` | |
| `--only-matching` | `controls.search.onlyMatching` | |
| `--unique` | `controls.search.unique` | |
| `--count` | `controls.search.countUnique` | |
| `--match-window <n>` | `controls.search.matchWindow` | |
| `--match-length <n>` | `controls.search.matchContentLength` | |
| `--max-matches <n>` | `controls.search.maxMatchesPerFile` | |
| `--match-page <n>` | `controls.search.matchPage` | |
| `--max-files <n>` | `controls.budget.maxFiles` | |
| `--hidden` | `scope.hidden` | |
| `--no-ignore` | `scope.noIgnore` | |
| `--include <glob>` | `scope.include` | |
| `--exclude <glob>` | `scope.exclude` | |
| `--page <n>` | `page` | |
| `--page-size <n>` | `itemsPerPage` | |
| `--limit <n>` | `limit` | |
| `--branch <ref>` | `from.ref` | |
| `--force-refresh` | `materialize.forceRefresh` | |
| `--context-lines <n>` | `controls.search.contextLines` | ✅ new |
| `--invert-match` | `controls.search.invertMatch` | ✅ new |

### find → `target:"files"` / `target:"code"`

`find` is explicitly in scope. Name/path discovery should use `target:"files"`;
content-backed discovery can use `target:"code"` or `target:"files"` with a text
predicate, depending on the old-command mode.

| Flag / behavior | OQL field |
|---|---|
| positional query | `where.field` over `basename`/`path`, or `where.text` for content-backed discovery |
| `[path|owner/repo]` | `from` + `scope.path` |
| `--source auto\|local\|github` | source-routing policy; keep as CLI-only validation over `from.kind` |
| `--search path\|content\|both` | choose field predicates, text predicate, or a two-query batch |
| `--name <glob>` / `--filename <name>` | field predicate `basename glob <glob>` / GitHub filename filter |
| `--path <subpath>` | `scope.path` / GitHub path scope |
| `--path-pattern <glob>` | field predicate `path glob <glob>` |
| `--regex <re>` | regex predicate for local path discovery |
| `--entry f\|d` | field predicate `entryType:file|directory` |
| `--ext <list>` | `scope.language` or extension include globs |
| `--include <glob>` / `--exclude <glob>` | `scope.include` / `scope.exclude` |
| `--max-depth <n>` / `--min-depth <n>` | `scope.maxDepth` / files-target params if supported |
| `--size-*`, `--modified-*`, `--accessed-*`, `--permissions`, `--empty`, `--executable`, `--readable`, `--writable` | files-target field predicates or local-only params |
| content flags (`--fixed-string`, `--perl-regex`, `--case-*`, `--whole-word`, `--invert-match`, `--context-lines`, counts, match paging) | same mappings as `grep` for the content-backed branch |
| `--hidden` / `--no-ignore` | `scope.hidden` / `scope.noIgnore` |
| `--concise` | `view:"discovery"` |
| `--limit`, `--page`, `--page-size` | pagination controls |

### cat → `target:"content"` (auto-routed when positional is a file)

| Flag | OQL field |
|---|---|
| `--mode none\|standard\|symbols` | `fetch.content.contentView:"exact"\|"compact"\|"symbols"` |
| `--match-string <s>` | `fetch.content.match.text` |
| `--match-regex` | `fetch.content.match.regex` |
| `--match-case-sensitive` | `fetch.content.match.caseSensitive` |
| `--start-line <n>` | `fetch.content.range.startLine` |
| `--end-line <n>` | `fetch.content.range.endLine` |
| `--context-lines <n>` | `fetch.content.range.contextLines` |
| `--char-offset <n>` | `fetch.content.charOffset` |
| `--char-length <n>` | `fetch.content.charLength` |
| `--full-content` | `fetch.content.fullContent` |
| `--raw` | render-layer only: print `row.content` body, no YAML |

### ls → `target:"structure"` / `target:"semantics"`

`--symbols` switches target to `semantics` + `params.type:"documentSymbols"`.

| Flag | OQL field |
|---|---|
| `--symbols` | `target:"semantics"`, `params.type:"documentSymbols"` |
| `--kind <kind>` | `params.kind` (post-filter) |
| `--depth <n>` | `fetch.tree.maxDepth` |
| `--include <glob>` | `scope.include` |
| `--ext <list>` | `scope.include` globs |
| `--files-only` | field predicate `entryType:file` |
| `--dirs-only` | field predicate `entryType:directory` |
| `--sort <field>` | `controls.search.sort` |
| `--hidden` | `scope.hidden` |
| `--tree` | force `target:"structure"` on a directory |

### lsp → `target:"semantics"`

| Flag | OQL field |
|---|---|
| `--op <lsp-type>` | `params.type` (renamed from `--type`) |
| `--symbol <name>` | `params.symbolName` |
| `--line <n>` | `params.lineHint` |
| `--workspace-root <path>` | `params.workspaceRoot` |
| `--depth <n>` | `params.depth` |
| `--format structured\|compact` | `params.format` |

### repo → `target:"repositories"`

| Flag | OQL field |
|---|---|
| `--topic <list>` | `params.topicsToSearch` |
| `--lang <lang>` | `params.language` |
| `--owner <owner>` | `params.owner` |
| `--stars <range>` | `params.stars` |
| `--sort <field>` | `params.sort` |
| `--limit <n>` | `limit` |
| `--concise` | `view:"discovery"` |

### history → `target:"commits"`

| Flag | OQL field |
|---|---|
| `--since <iso>` | `params.since` |
| `--until <iso>` | `params.until` |
| `--author <name>` | `params.author` |
| `--branch <ref>` | `params.branch` |
| `--diff` | `params.includeDiff` |
| `--limit <n>` | `limit` |

### pr → `target:"pullRequests"`

| Flag | OQL field |
|---|---|
| `--pr <n>` | `params.prNumber` |
| `--state open\|closed\|merged` | `params.state` |
| `--author <user>` | `params.author` |
| `--patches` | PR patch mode |
| `--comments` | `params.commentPage:1` |
| `--commits` | `params.commitPage:1` |
| `--deep` | all content flags |
| `--file <path>` | `params.files:[path]` |
| `--match-string <s>` | `params.matchString` |
| `--concise` | `view:"discovery"` |

### pkg → `target:"packages"`

| Flag | OQL field |
|---|---|
| `--mode lean\|full` | `params.mode` |
| `--page <n>` | `page` |

### binary (read modes) → `target:"artifacts"`

| Flag | OQL field |
|---|---|
| `--inspect` | `params.mode:"inspect"` |
| `--list` | `params.mode:"list"` |
| `--strings` | `params.mode:"strings"` |
| `--extract <entry>` | `params.mode:"extract"`, `params.archiveFile` |
| `--decompress` | `params.mode:"decompress"` |
| `--detailed` | `params.verbose` |
| `--match <s>` | `params.matchString` |
| `--min-length <n>` | `params.minLength` |

---

## Render Gap

| Gap | Fix |
|---|---|
| `--raw` (body-only stdout) | When `target:"content"` + `--raw`: print `row.content` directly, suppress YAML envelope. Render-layer only — no OQL field needed. |

---

## `octocode-core` CLICommandSpec Migration

Each command is documented as a `CLICommandSpec` in
`octocode-mcp-host/packages/octocode-core/src/resources/cli/<command>.ts`.
These files drive agent-facing `--help`, `AGENT_INSTRUCTIONS`, and the CLI context block.
**Removing a command without migrating its spec breaks agent understanding.**

### Mechanism

1. All guidance (scheme, whenToUse, examples, options) from removed command files is
   absorbed into `search.ts` before the command is deleted.
2. After absorption, delete the command spec file and remove its import + entry from
   `octocode-mcp-host/packages/octocode-core/src/resources/cli/index.ts`
   (`COMMAND_SPECS` array).
3. The updated `search.ts` is the **sole source of truth** for agent understanding of all
   absorbed operations.

**Step 0 — documentation absorption is not enough:** `octocode-core`
`search.ts` currently describes the target state, but the built CLI does not yet
parse that full surface. Treat Step 0 as complete only when `search --help`,
`search --scheme`, and the executable parser all agree with the target-state
`search.ts`.

### Critical guidance that must survive in `search.ts`

| Source | Must-keep guidance |
|---|---|
| `grep.ts` | GitHub empty ≠ absence; structural pattern must be complete AST node; `--view discovery` for paths-only |
| `find.ts` | `--search path|content|both`; `--repo` remote-as-local handoff; empty-result recovery by loosening name/ext/path filters |
| `lsp.ts` | Always pass `--line`; inference picks first text occurrence (wrong on comment/import); `references` ≠ deletion proof; run `documentSymbols` first |
| `cat.ts` | `--mode none` for exact text; `symbols` for orientation; `--raw` for body-only output |
| `ls.ts` | Use first when layout unknown; file positional auto-triggers symbol outline |
| `binary.ts` | Run `--list` before `--extract`; do not guess entry names |
| `pkg.ts` | Follow source repository for implementation evidence |
| `diff.ts` | For git working-tree diffs, use git directly |
| `history.ts` | Headline `(#NNN)` → follow up with `--target pullRequests --pr <n>` |

### Per-phase `octocode-core` changes

Each deletion goes in the same PR as the CLI implementation removal for that phase.

| Phase | Delete spec file(s) | Remove from `index.ts` |
|---|---|---|
| 3 | `repo.ts` · `pkg.ts` · `history.ts` · `pr.ts` | `repo, pkg, history, pr` |
| 4 | `binary.ts` | `binary` |
| 5 | `lsp.ts` · `ls.ts` | `lsp, ls` |
| 6 | `find.ts` · `grep.ts` · `cat.ts` | `find, grep, cat` |
| 7 | `diff.ts` | `diff` |

---

## Step 1 — Alignment Check (gate before any removal)

**No command is removed until its alignment check passes.**

For each absorbed command: run the old command and the `search` equivalent side-by-side.
Verify output shape, field names, pagination, and error behaviour match.
Fix any gap in `search` before marking the command done.
All 11 must pass before any spec file is deleted from `octocode-core`.

### Definition of done (per command)

1. Built `search --help` accepts and lists every flag used by that command's equivalence examples.
2. `search` equivalent produces the same result fields as the old command.
3. All old-command flags listed in §CLI Shorthand have a working `search` counterpart or an explicit non-goal note.
4. `search --scheme` describes the operation correctly (no stale text referencing the old command as preferred).
5. Agent instructions in `search.ts` (`whenToUse`, `scheme`) cover the use case without relying on the removed command.
6. `octocode-mcp-host/packages/octocode-core/src/resources/cli/index.ts` is updated only in the same PR that removes the executable command.
7. No regression on `yarn test` in `octocode` and `octocode-tools-core`.

### Equivalence table

Run each pair. Output must be structurally identical (same top-level keys, same row kinds).

#### grep → search (code/files)

```bash
# text search
grep "runCLI" packages/octocode/src --type ts --limit 5
search "runCLI" packages/octocode/src --lang ts --limit 5

# discovery (paths only)
grep "runCLI" packages/octocode/src --type ts --concise
search "runCLI" packages/octocode/src --lang ts --view discovery

# AST structural
grep packages/octocode/src --pattern 'eval($X)' --type ts
search --pattern 'eval($X)' packages/octocode/src --lang ts

# invert + context (requires Layer 1 schema)
grep "TODO" packages/octocode/src --type ts --invert-match --context-lines 2
search "TODO" packages/octocode/src --lang ts --invert-match --context-lines 2
```

Verify: `results[].kind === "code"`, `path`, `line`, `snippet` present; pagination fields match.

#### find → search (files/code)

```bash
# path/name discovery
find "command-help" packages/octocode/src --search path --ext ts --limit 5
search "command-help" packages/octocode/src --target files --search path --lang ts --limit 5

# content-backed discovery
find "runCLI" packages/octocode/src --search content --ext ts --limit 5
search "runCLI" packages/octocode/src --target files --search content --lang ts --limit 5

# both path and content
find "parser" packages/octocode/src --search both --ext ts --limit 5
search "parser" packages/octocode/src --target files --search both --lang ts --limit 5

# remote-as-local
find "useState" --repo facebook/react --path packages/react --search both --ext js
search "useState" packages/react --repo facebook/react --target files --search both --lang js
```

Verify: path mode returns file rows only; content mode preserves content snippets
or match metadata; `--repo` returns `location.localPath` for local follow-ups.

#### cat → search (content)

```bash
# standard read
cat packages/octocode/src/cli/index.ts
search packages/octocode/src/cli/index.ts

# exact text
cat packages/octocode/src/cli/index.ts --mode none
search packages/octocode/src/cli/index.ts --mode none

# match slice
cat packages/octocode/src/cli/index.ts --match-string "runCLI" --mode none
search packages/octocode/src/cli/index.ts --match-string "runCLI" --mode none

# line range
cat packages/octocode/src/cli/index.ts --start-line 1 --end-line 30 --mode none
search packages/octocode/src/cli/index.ts --start-line 1 --end-line 30 --mode none

# raw
cat packages/octocode/src/cli/index.ts --raw
search packages/octocode/src/cli/index.ts --raw
```

Verify: `results[].kind === "content"`, `content` field present; `--raw` prints bare body, no YAML.

#### ls → search (structure / semantics)

```bash
# directory tree
ls packages/octocode/src --depth 2
search packages/octocode/src --tree --depth 2

# symbol outline (file)
ls packages/octocode/src/cli/index.ts
search packages/octocode/src/cli/index.ts --op documentSymbols

# symbol outline (directory)
ls packages/octocode/src --symbols --ext ts --limit 5
search packages/octocode/src --symbols --lang ts --limit 5

# remote tree
ls src --repo facebook/react --depth 1
search src --repo facebook/react --tree --depth 1
```

Verify: tree → `results[].kind === "tree"`; outline → `recordType === "semantics"` with symbol names and lines.

#### lsp → search (semantics)

```bash
# document symbols (no --line needed)
lsp packages/octocode/src/cli/index.ts --type documentSymbols
search packages/octocode/src/cli/index.ts --op documentSymbols

# references (--line required)
lsp packages/octocode/src/cli/index.ts --type references --symbol runCLI --line 73
search packages/octocode/src/cli/index.ts --op references --symbol runCLI --line 73

# callers
lsp packages/octocode/src/cli/index.ts --type callers --symbol runCLI --line 73
search packages/octocode/src/cli/index.ts --op callers --symbol runCLI --line 73
```

Verify: `recordType === "semantics"`, location fields (`uri`, `line`, `name`) present and identical.

#### repo → search (repositories)

```bash
repo "mcp server" --language TypeScript --stars ">100" --limit 5
search "mcp server" --target repositories --lang TypeScript --stars ">100" --limit 5

repo --owner bgauryy --concise --limit 10
search --target repositories --owner bgauryy --concise --limit 10
```

Verify: `recordType === "repository"`, `id`, `stars`, `language` fields present.

#### pr → search (pullRequests)

```bash
pr facebook/react --state open --concise --limit 5
search facebook/react --target pullRequests --state open --concise --limit 5

pr bgauryy/octocode-mcp#1 --patches --comments
search bgauryy/octocode-mcp --target pullRequests --pr 1 --patches --comments
```

Verify: list → `recordType === "pullRequest"`, `id`, `state`, `title`; deep-read → patch and comment content present.

#### history → search (commits)

```bash
history facebook/react/packages/react/src --limit 5
search facebook/react/packages/react/src --target commits --limit 5

history bgauryy/octocode/README.md --diff
search bgauryy/octocode/README.md --target commits --diff
```

Verify: `recordType === "commit"`, `sha`, `title`, `author`, `date` fields present.

#### pkg → search (packages)

```bash
pkg zod
search zod --target packages

pkg "react state management"
search "react state management" --target packages
```

Verify: `recordType === "package"`, `id` (package name), `repository` present for exact-name queries.

#### binary → search (artifacts)

```bash
# inspect
binary packages/octocode-engine/octocode-engine.darwin-arm64.node
search packages/octocode-engine/octocode-engine.darwin-arm64.node --inspect

# list archive
binary packages/octocode/out/octocode.js --list
search packages/octocode/out/octocode.js --list

# strings
binary packages/octocode-engine/octocode-engine.darwin-arm64.node --strings --min-length 8
search packages/octocode-engine/octocode-engine.darwin-arm64.node --strings --min-length 8
```

Verify: `recordType === "artifact"`, mode-specific fields (`format`, `arch`, `entries`, `strings`) present.

#### diff → search (diff)

```bash
# Two local files (requires Phase 7 local lane)
diff packages/octocode/src/cli/commands/grep.ts packages/octocode/src/cli/commands/search.ts
search packages/octocode/src/cli/commands/grep.ts packages/octocode/src/cli/commands/search.ts
```

Verify: `recordType === "diff"`, `additions`, `deletions`, unified diff lines present.
⚠️ This check is **blocked** until Phase 7 (local diff lane) is implemented.

### Alignment check status

| Command | Equivalent runs | Fields match | All flags work | Spec updated | ✅ Done |
|---|:---:|:---:|:---:|:---:|:---:|
| `grep` | | | | | |
| `find` | | | | | |
| `cat` | | | | | |
| `ls` | | | | | |
| `lsp` | | | | | |
| `repo` | | | | | |
| `pr` | | | | | |
| `history` | | | | | |
| `pkg` | | | | | |
| `binary` | | | | | |
| `diff` | ⚠️ blocked | | | | |

**Gate:** all non-blocked rows must be ✅ before any `octocode-core` spec file is deleted.

---

## Implementation Order

Step 1 (alignment check) is the gate. Phases run after the parser/context
alignment in Phase 0. Phase 2 requires Phase 1. Phases 3–5 are independent of
each other after Phase 1.

### Phase 0 — Parser/context synchronization (first PR)
0. Register the target-state `search` flags in `packages/octocode/src/cli/commands/search.ts`
1. Add parse-only smokes for `--target`, `--op`, `--view`, `--mode`, `--raw`, `--limit`, `--page`, `--page-size`, `--search`, and representative target flags
2. Make built `search --help` and `octocode-core` `search.ts` agree before adding behavior
3. Keep all old quick commands and specs untouched

### Phase 1 — Design fixes (prerequisite for everything)
4. Resolve `--type` collision: rename language filter to `--lang`, introduce `--op` for LSP type
5. Resolve `--mode` overload: rename search-view flag to `--view`
6. Implement positional auto-routing (`file` → `content`, `--op` present → `semantics`, `--tree` → `structure`, `--target files` → file discovery)

### Phase 2 — Schema additions
7. Add `contextLines` + `invertMatch` to `controls.search` in `oql/types.ts`
8. Wire in `oql/adapters/local.ts:searchControls()`

### Phase 3 — Non-local target porting (no local schema deps)
9. `repo` · `pkg` · `history` · `pr`

### Phase 4 — Binary read modes
10. `binary` (inspect/list/strings/extract/decompress)

### Phase 5 — LSP + structure
11. `lsp` → `--op` + §lsp flags
12. `ls` → `--symbols` switch + §ls flags

### Phase 6 — Files + search + content (after Phase 1 + 2)
13. `find` → full §find flag set, including `--search path|content|both`
14. `grep` → full §grep flag set
15. `cat` → full §cat flag set + `--raw`

### Phase 7 — Diff blocker
16. Add `localFile` lane to `diffLanes.ts`
17. Remove `diff` command only after PR diff and GitHub direct-file diff still pass

---

## Files Changed

| File | Change |
|---|---|
| `packages/octocode-tools-core/src/oql/types.ts` | `controls.search.contextLines`, `controls.search.invertMatch` |
| `packages/octocode-tools-core/src/oql/adapters/local.ts` | `searchControls()`: wire two new fields |
| `packages/octocode-tools-core/src/oql/diffLanes.ts` | Add `localFile` lane while preserving `prPatch` and GitHub `directFile` |
| `packages/octocode-tools-core/src/oql/adapters/` | Local diff execution |
| `packages/octocode-tools-core/src/oql/schemeText.ts` | Document new fields |
| `packages/octocode-tools-core/src/oql/shorthand.ts` | `buildShorthandInput()`: `--lang` rename, auto-routing, `--target`, `--search`, all new fields |
| `packages/octocode/src/cli/commands/search.ts` | Options array, `buildSugar()`, `--raw` renderer |
| `octocode-mcp-host/packages/octocode-core/src/resources/cli/search.ts` | Keep agent-facing command docs synchronized with built CLI flags |
| `octocode-mcp-host/packages/octocode-core/src/resources/cli/index.ts` | Remove absorbed command specs only after each phase passes |
| `docs/OCTOCODE_QUERY_LANGUAGE.md` | Controls table, diff lanes, `--op` / `--lang` |

**Not changed:** `clone.ts` · `cache.ts` · `unzip.ts`  
**Deleted per phase:** `find.ts` · `grep.ts` · `cat.ts` · `ls.ts` · `lsp.ts` · `repo.ts` · `pr.ts` · `history.ts` · `pkg.ts` · `binary.ts` · `diff.ts`
