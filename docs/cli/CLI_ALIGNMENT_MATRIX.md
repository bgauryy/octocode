# Octocode CLI — Alignment Matrix

> 15 command surfaces total: 11 read-only quick commands targeted for `search`
> absorption, 3 write/materialization commands that stay separate, plus `search`
> itself.
> Generated 2026-06-23 from `--help` output and live CLI inspection.  
> §9 documents what `search` still needs before any quick command can be removed.
>
> **Current-vs-target warning:** `octocode-core` already documents the target
> `search` surface, but the built CLI still rejects several target-state flags
> (`--target`, `--op`, `--mode`, `--raw`, `--limit`). Treat this matrix as a
> migration guide, not proof that removal is safe today.
>
> **OQL source of truth:** full OQL scheme, language-selector logic,
> transformer/adaptor inventory, evidence semantics, diagnostics, pagination,
> and continuation rules live in
> [`docs/OCTOCODE_QUERY_LANGUAGE.md`](https://github.com/bgauryy/octocode/blob/main/docs/OCTOCODE_QUERY_LANGUAGE.md).
> This matrix maps CLI commands to that language; it does not redefine it.

---

## 1. Command Identity

| Command   | One-liner purpose                                        | Category           | Underlying tool(s)                              |
|-----------|----------------------------------------------------------|--------------------|-------------------------------------------------|
| `ls`      | Directory tree **or** semantic symbol outline            | Orient / Structure | `localViewStructure` · `ghViewRepoStructure` · `lspGetSemantics(documentSymbols)` |
| `cat`     | Read + minify a single file (local or GitHub)            | Read               | `localGetFileContent` · `ghGetFileContent`      |
| `grep`    | Text / regex / AST-pattern search                        | Search             | `localSearchCode` · `ghSearchCode`              |
| `find`    | Locate files by name, path fragment, or content term     | Discover           | `localFindFiles` · `localSearchCode` · `ghSearchCode` |
| `diff`    | Compare two explicit file refs side-by-side              | Compare            | `localGetFileContent` · `ghGetFileContent` (mode:none) |
| `lsp`     | Symbol-identity semantics (defs, refs, callers, …)       | Semantics          | `lspGetSemantics`                               |
| `search`  | Typed OQL research across all targets                    | All / Orchestrate  | All tools via OQL compiler                      |
| `repo`    | Discover GitHub repositories                             | Discover (GitHub)  | `ghSearchRepos`                                 |
| `pr`      | List or deep-read GitHub pull requests                   | History (GitHub)   | `ghHistoryResearch`                             |
| `history` | Commit history for a repo / path                         | History (GitHub)   | `ghHistoryResearch(type:commits)`               |
| `pkg`     | Research an npm package + its source repo                | Discover (npm)     | `npmSearch`                                     |
| `binary`  | Inspect, list, extract, decompress, or string-scan files | Binary / Archive   | `localBinaryInspect`                            |
| `unzip`   | Unpack an archive to disk, then research with local tools| Archive            | `localBinaryInspect(mode:unpack)`               |
| `clone`   | Git-clone a GitHub repo / subtree to local disk          | Materialize        | `ghCloneRepo`                                   |
| `cache`   | Fetch / inspect / clear the local materialization cache  | Materialize        | `ghCloneRepo` (fetch) + cache ops               |

---

## 2. Data Source

| Command   | Local | GitHub | npm | Both local+GH | Notes |
|-----------|:-----:|:------:|:---:|:-------------:|-------|
| `ls`      | ✅    | ✅     |     | ✅            | `--repo` materializes remote first |
| `cat`     | ✅    | ✅     |     | ✅            | auto-routes by ref shape |
| `grep`    | ✅    | ✅     |     | ✅            | `--repo` clones then searches local |
| `find`    | ✅    | ✅     |     | ✅            | `--source auto|local|github` |
| `diff`    | ✅    | ✅     |     | ✅            | each side can be different source |
| `lsp`     | ✅    |        |     |               | `--repo` materializes, then runs local LSP |
| `search`  | ✅    | ✅     |     | ✅            | `from.kind: local|github|materialized|npm` |
| `repo`    |       | ✅     |     |               | GitHub only |
| `pr`      |       | ✅     |     |               | GitHub only |
| `history` |       | ✅     |     |               | GitHub only (rejects local paths) |
| `pkg`     |       |        | ✅  |               | npm only |
| `binary`  | ✅    |        |     |               | local file only |
| `unzip`   | ✅    |        |     |               | local file only |
| `clone`   |       | ✅     |     |               | clones to local, reports path |
| `cache`   |       | ✅     |     |               | materializes GitHub → local |

---

## 3. Output Grain

| Command   | Files | Lines/Snippets | Symbols | Metadata | Tree | Diff | Semantics |
|-----------|:-----:|:--------------:|:-------:|:--------:|:----:|:----:|:---------:|
| `ls`      | ✅    |                | ✅ (--symbols) | ✅  | ✅   |      |           |
| `cat`     |       | ✅             | ✅ (--mode symbols) |  |   |      |           |
| `grep`    | ✅    | ✅             |         |          |      |      |           |
| `find`    | ✅    | ✅ (content)   |         | ✅       |      |      |           |
| `diff`    |       | ✅             |         |          |      | ✅   |           |
| `lsp`     |       | ✅             | ✅       |          |      |      | ✅        |
| `search`  | ✅    | ✅             | ✅       | ✅       | ✅   | ✅   | ✅        |
| `repo`    |       |                |         | ✅       |      |      |           |
| `pr`      |       | ✅ (patches)   |         | ✅       |      | ✅   |           |
| `history` |       | ✅ (commits)   |         | ✅       |      | ✅ (--diff) |      |
| `pkg`     |       |                |         | ✅       |      |      |           |
| `binary`  |       | ✅ (strings)   |         | ✅       | ✅ (list) |   |           |
| `unzip`   |       |                |         | ✅       |      |      |           |
| `clone`   |       |                |         | ✅ (path)|      |      |           |
| `cache`   |       |                |         | ✅ (path)|      |      |           |

---

## 4. Key Flags / Capabilities

| Command  | Minification modes | AST/structural | Pagination | `--repo` remote-as-local | `--concise` | `--json` |
|----------|--------------------|:--------------:|:----------:|:------------------------:|:-----------:|:--------:|
| `ls`     | — (tree/outline)   |                | ✅         | ✅                       |             | ✅       |
| `cat`    | `none` `standard` `symbols` | —     | ✅ (char/page) | ✅                  |             | ✅       |
| `grep`   | `paginated` `discovery` `detailed` | ✅ (`--pattern` `--rule`) | ✅ | ✅    | ✅ (paths only) | ✅  |
| `find`   | —                  |                | ✅         | ✅                       | ✅          | ✅       |
| `diff`   | —                  |                |            |                          |             | ✅       |
| `lsp`    | `structured` `compact` | —          | ✅         | ✅                       |             | ✅       |
| `search` | all (per target)   | ✅             | ✅         | via `materialize`        | target: `--concise`; current built CLI: `--compact` renderer only | ✅       |
| `repo`   | —                  |                | ✅         |                          | ✅          | ✅       |
| `pr`     | —                  |                | ✅         |                          | ✅          | ✅       |
| `history`| —                  |                | ✅         |                          |             | ✅       |
| `pkg`    | `lean` `full`      |                | ✅         |                          |             | ✅       |
| `binary` | —                  |                | ✅ (scan/char) |                       |             | ✅       |
| `unzip`  | —                  |                |            |                          |             | ✅       |
| `clone`  | —                  |                |            |                          |             | ✅       |
| `cache`  | —                  |                |            |                          |             | ✅       |

---

## 5. `search` — OQL Target Coverage

`search` (OQL) is the only command that cross-cuts every other command's domain.

| OQL `target`    | Equivalent quick command(s)      | What it adds over the quick cmd |
|-----------------|----------------------------------|---------------------------------|
| `code`          | `grep`                           | Typed routing, `--explain`, materialization policy, provenance |
| `content`       | `cat`                            | Typed file reads, slices, minification, continuations |
| `structure`     | `ls`                             | Typed query, depth controls     |
| `files`         | `find`                           | Typed field predicates          |
| `semantics`     | `lsp`                            | Typed, composable with `where`  |
| `repositories`  | `repo`                           | Typed, composable               |
| `packages`      | `pkg`                            | Typed                           |
| `pullRequests`  | `pr`                             | Typed, composable               |
| `commits`       | `history`                        | Typed, date/author filters      |
| `artifacts`     | `binary`                         | Typed                           |
| `diff`          | `diff`                           | Typed                           |
| `research`      | *(no direct equivalent)*         | Dead-code / reachability triage; data.summary + packets |
| `graph`         | *(no direct equivalent)*         | LSP proof per symbol (confirmed-by-lsp / conflicting / needs-framework) |
| `materialize`   | `clone` / `cache`                | Typed checkpoint with follow-up continuations |

---

## 6. Cheapest-Tool Decision Tree

```
Question                         → Use
─────────────────────────────────────────────────────────────────────
Unknown repo layout?             → ls <owner/repo> (--depth 1..3)
What does this file define?      → ls <file> --symbols  OR  lsp --type documentSymbols
Read an exact file region?       → cat <path> --match-string / --start-line
Find all sites of a text term?   → grep <term> <path>
Find a CODE SHAPE (no regex)?    → grep <path> --pattern '<ast>' --type <lang>
Locate a file by name/fragment?  → find <name> <path>
Who changed this file + when?    → history owner/repo/path
Which PR introduced a change?    → history (headline #NNN) → pr owner/repo#NNN
Read a PR diff / comments?       → pr owner/repo#NNN --patches --comments
Compare two known file versions? → diff <left> <right>
Symbol definition / callers?     → lsp <file> --type definition|callers --symbol X --line N
npm package + source repo?       → pkg <name>
Discover repos for a topic?      → repo <keywords> --language <lang>
Inspect a .so/.node/.wasm?       → binary <file>
Multi-file work inside a .zip?   → unzip <archive> → ls/grep/cat on localPath
Research a remote repo locally?  → clone|cache --then ls/grep/cat/lsp on returned localPath
                                    OR any local cmd with --repo owner/repo
Typed, provenance-rich research? → search (run --scheme first)
Dead-code / unused exports?      → search --query '{"target":"research",...}' (two-step)
```

---

## 7. `--repo` Remote-as-Local Bridge

Commands that accept `--repo owner/repo[@branch]` to transparently materialize a GitHub repo and run locally:

| Command  | `--repo` supported | Effect |
|----------|--------------------|--------|
| `ls`     | ✅                 | Clones, lists local tree; `location.localPath` returned |
| `cat`    | ✅                 | Clones, reads local file |
| `grep`   | ✅                 | Clones, runs `localSearchCode` on local path |
| `find`   | ✅                 | Clones, runs local find tools |
| `lsp`    | ✅                 | Clones, runs LSP on local file |
| `diff`   | ❌                 | Each side is an explicit path, not a repo |
| `search` | via `materialize` | `materialize:{mode:'auto|required'}` clones bounded subtree |
| others   | ❌                 | Command-specific: `clone`, `cache` materialze directly |

Cache TTL: **24 h**. Use `--force-refresh` or `cache clear --clone` to invalidate.

---

## 8. Flow Recipes (commands in sequence)

| Goal                                    | Sequence |
|-----------------------------------------|----------|
| Orient → read code                      | `ls <dir>` → `cat <file>` |
| Find usage → get exact evidence         | `grep <term> <path>` → `cat <file> --match-string <term>` |
| Find file → navigate symbols            | `find <name> <path>` → `lsp <file> --type documentSymbols` → `lsp --type callers --symbol X --line N` |
| Remote file → AST search                | `clone owner/repo` → `grep <localPath> --pattern 'eval($X)' --type js` |
| Trace a change to a PR                  | `history owner/repo/file` → `pr owner/repo#NNN --patches --comments` |
| npm package → source code               | `pkg zod` → `repo zod --owner colinhacks` → `cat/grep/ls` |
| Dead-code audit                         | `search --query '{"target":"research",...,"page":1,"itemsPerPage":1}'` → packets → `search --query <packet.next.graph>` |
| Inspect a native addon                  | `binary packages/octocode-engine/octocode-engine.linux-x64-gnu.node` |
| Multi-file archive research             | `unzip release.zip` → `ls <localPath>` → `grep <term> <localPath>` |

---

## 9. `search` Gaps Before Command Absorption

Goal: make `search` the single read-only CLI entry-point, removing `find`,
`grep`, `cat`, `ls`, `lsp`, `repo`, `pr`, `history`, `pkg`, `binary`, and
eventually `diff`. `clone`, `cache`, and `unzip` stay separate because they write
to disk / materialize state.

### Layer 0 — parser/context synchronization

The first implementation PR must make the executable CLI parser, built
`search --help`, `search --scheme`, and `octocode-core` `search.ts` describe the
same command surface. Today the built CLI only accepts the small shorthand set
(`text`, `regex`, `pcre2`, `pattern`, `rule`, `lang`, `type`, `corpus`,
`materialize`) while the target context already mentions broader flags.

| Must parse before removal | Why |
|---|---|
| `--target` | Needed for repositories, pullRequests, commits, packages, artifacts, diff, files |
| `--op` | Needed to replace `lsp --type` without colliding with language filters |
| `--view` / `--mode` | Separates grep-style result density from cat-style minification |
| `--raw` | Required for `cat --raw` parity |
| `--search path\|content\|both` | Required for `find` parity |
| `--limit`, `--page`, `--page-size` | Shared pagination parity across old commands |

### Layer 1 — OQL schema (2 missing fields)

`local.ts:searchControls()` is the only bridge from `controls.search.*` to `localSearchCode`. Two `grep` fields are passed directly to `localSearchCode` but have no OQL schema entry and no `searchControls()` mapping:

| Missing OQL field | `localSearchCode` field | `grep` flag |
|---|---|---|
| `controls.search.contextLines` | `contextLines` | `--context-lines` |
| `controls.search.invertMatch` | `invertMatch` | `--invert-match` |

All other predicate-level fields (`caseSensitive`, `caseInsensitive`, `wholeWord`, `fixedString`, `perlRegex`, `multiline`, `multilineDotall`) already exist via `compileWhere()` — no schema change needed there.

---

### Layer 2 — CLI shorthand (flags missing from `search`)

`search.ts:buildSugar()` passes only 9 fields to `buildShorthandInput()`: `text`, `regex`, `pcre2`, `pattern`, `rule`, `lang`, `type`, `corpus`, `materialize`. Every flag below is absent from the `search` option list.

#### 2a — grep compatibility flags

| Flag | OQL field | Layer 1 prereq? |
|---|---|:---:|
| `--case-insensitive` | `where.text.case:"insensitive"` | |
| `--case-sensitive` | `where.text.case:"sensitive"` | |
| `--whole-word` | `where.text.wholeWord:true` | |
| `--fixed` / `--fixed-string` | default text predicate (already literal) | |
| `--multiline` | `where.regex.multiline:true` | |
| `--multiline-dotall` | `where.regex.dotAll:true` | |
| `--files-only` | `filesOnly:true` | |
| `--files-without-match` | `filesWithoutMatch:true` | |
| `--concise` | `view:"discovery"` | |
| `--mode paginated\|discovery\|detailed` | `view` | ⚠️ see naming conflict |
| `--count-lines` | `controls.search.countLinesPerFile:true` | |
| `--count-matches` | `controls.search.countMatchesPerFile:true` | |
| `--only-matching` | `controls.search.onlyMatching:true` | |
| `--unique` | `controls.search.unique:true` | |
| `--count` | `controls.search.countUnique:true` | |
| `--match-window <n>` | `controls.search.matchWindow` | |
| `--match-length <n>` | `controls.search.matchContentLength` | |
| `--max-matches <n>` | `controls.search.maxMatchesPerFile` | |
| `--match-page <n>` | `controls.search.matchPage` | |
| `--max-files <n>` | `controls.budget.maxFiles` | |
| `--hidden` | `scope.hidden:true` | |
| `--no-ignore` | `scope.noIgnore:true` | |
| `--include <glob>` | `scope.include:[glob]` | |
| `--exclude <glob>` | `scope.exclude:[glob]` | |
| `--page <n>` | `page` | |
| `--page-size <n>` | `itemsPerPage` | |
| `--limit <n>` | `limit` | |
| `--branch <ref>` | `from.ref` (GitHub corpus) | |
| `--force-refresh` | `materialize.forceRefresh:true` | |
| `--context-lines <n>` | `controls.search.contextLines` | ✅ Layer 1 |
| `--invert-match` | `controls.search.invertMatch` | ✅ Layer 1 |

#### 2b — find compatibility flags

`find` is in scope because it is read-only research. Its path/name branch maps to
`target:"files"`; its content branch maps to content-backed file discovery.

| Flag | OQL field / behavior |
|---|---|
| `--target files` | explicit file-discovery replacement |
| `--search path\|content\|both` | field predicate, text predicate, or batch of both |
| `--source auto\|local\|github` | CLI source-routing policy over `from.kind` |
| `--ext <list>` | extension/language scope |
| `--path <subpath>` | `scope.path` |
| `--filename <name>` / `--name <pattern>` | basename field predicate / GitHub filename filter |
| `--path-pattern <pattern>` | path field predicate |
| `--regex <pattern>` | local path regex predicate |
| `--entry f\|d`, `--min-depth`, `--max-depth` | files-target params / field predicates |
| size/time/permission flags | files-target field predicates or explicit unsupported notes |
| local content flags | same mappings as grep compatibility branch |

#### 2c — cat compatibility flags (route to `target:"content"`)

| Flag | OQL field |
|---|---|
| `--mode none\|standard\|symbols` | `fetch.content.contentView:"exact"\|"compact"\|"symbols"` |
| `--match-string <s>` | `fetch.content.match.text` |
| `--match-regex` | `fetch.content.match.regex:true` |
| `--match-case-sensitive` | `fetch.content.match.caseSensitive:true` |
| `--start-line <n>` | `fetch.content.range.startLine` |
| `--end-line <n>` | `fetch.content.range.endLine` |
| `--context-lines <n>` | `fetch.content.range.contextLines` |
| `--char-offset <n>` | `fetch.content.charOffset` |
| `--char-length <n>` | `fetch.content.charLength` |
| `--full-content` | `fetch.content.fullContent:true` |
| *(auto-route)* | `target:"content"` when positional is a file path — today shorthand always emits `target:"code"` |

---

### Layer 3 — Render layer (no OQL backing)

| Gap | Description |
|---|---|
| `--raw` | Body-only stdout, no YAML envelope. `cat` supports it; `search` has no equivalent at any layer. Needs a render flag in `search.ts`. |

---

### Naming conflict: `--mode`

| Command | `--mode` maps to |
|---|---|
| `grep` | search view — `paginated\|discovery\|detailed` → OQL `view` |
| `cat` | content minification — `none\|standard\|symbols` → OQL `fetch.content.contentView` |

Both flag values are disjoint sets, so `search` can resolve it by value (no `paginated\|discovery\|detailed` value is a valid minification mode and vice versa), or rename one (e.g. `--view` for search mode, keep `--mode` for content).

---

### Summary

| Layer | Item | Count |
|---|---|---|
| Parser/context sync | Built parser, `search --help`, `search --scheme`, and `octocode-core` `search.ts` agree | **gate** |
| OQL schema | `controls.search.contextLines` + `controls.search.invertMatch` | **2** |
| CLI flags — grep compat | ripgrep control/scope/output flags missing from `buildSugar()` | **31** |
| CLI flags — find compat | path/name discovery, content discovery, metadata filters, and `--search path\|content\|both` | **broad; must enumerate before implementation** |
| CLI flags — cat compat | content-read flags + file-path auto-routing | **11 + routing** |
| Render layer | `--raw` body-only output | **1** |
| Design decision | `--mode` naming conflict between grep view and cat minification | **1** |

---

## 10. Exit Codes (all commands)

| Code | Meaning          |
|------|------------------|
| `0`  | OK               |
| `2`  | Bad input        |
| `3`  | Not found        |
| `4`  | Auth error       |
| `5`  | Tool error       |
| `7`  | Rate limited     |
