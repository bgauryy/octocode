# Octocode arm primer

Inject as the `octocode` runner's only primer. Every research call is
`npx octocode tools <tool> --queries '<json>'` (no MCP, no gh). `--queries` takes one JSON
object or an array of objects (batch). This primer is fixed setup — it is **not** counted;
use it instead of paying for schema discovery.

## Tools — what each is for and when to STOP

| Tool | Use it for — and when NOT to |
|---|---|
| `ghSearch` | Remote discovery through strict operations: `code` for code/file paths, `repositories` for repository discovery, and `tree` for a known repository. For code, use `match:"path"` when only filenames matter. For repositories, start with `concise:true`. Skip discovery when you already know the exact file path. |
| `ghGetFileContent` | Read a file or a **region** once you know the path — not for discovery. A code-search snippet can narrow the candidate; fetch exact source when the answer depends on source text, line identity, or a fixed revision. |
| `ghSearchHistory` | Search or list history with `operation:"pullRequests"`, `"issues"`, or `"commits"`. Use it for discovery, not exact item reads. |
| `ghGetHistoryItem` | Read one PR or issue by `number`, one commit by `ref`, or compare `base`+`head`. Select only the content or diff you need. |
| `ghCloneRepo` | Materialize a repo/sparse subtree **only** for repeated reads, structural (AST) matching, or LSP semantics. |
| `artifactSearch` | Resolve dependencies/source or discover packages by capability. Require ecosystem `type` and exact `packageName` or `keywords`; PyPI is exact-only. Skip when source is known or installed behavior needs local evidence. |
| `localSearch` | Lexical text and regex occurrences with file+line anchors. Choose a result view and follow returned continuations. |
| `astSearch` | Filesystem, syntax tree, symbols, structural matches, and bounded topology. Use `operation:"topology"` with `analysis` for dependencies, dependents, paths, reachability, cycles, or dead-code candidates. |
| `localFetch` | Read exact local file bytes or an anchored region. Use `minify:"symbols"` only for an outline; preserve `minify:"none"` for exact content. |
| `lspSearch` | Definitions, references, callers/callees, symbols, types, and diagnostics — **after** search/read gives a real file+line. `documentSymbols`/`diagnostic` need `uri`; `workspaceSymbol` needs `symbolName` plus `uri` or `workspaceRoot`; anchored operations need `uri`+`symbolName`+`lineHint`. |

## Lean path

- **Use snippets as leads.** A `ghSearch(operation:"code")` result can identify a candidate, but fetch the file when exact source, line identity, or revision matters.
- **Choose the needed source view.** Read a known region directly with `matchString` or `startLine`+`endLine`. For an unfamiliar large file, use `minify:"symbols"` only when its outline can select the decisive region. Path-only reads are valid and exact by default; selectors are optional and mutually exclusive with `fullContent`. Both readers use `chunkType:"lines"|"bytes"`, `offset`, and `limit`; copy `next.continue` unchanged. Match context defaults to five lines or 256 UTF-8 bytes according to chunk type; explicit `contextLines` and `contextBytes` are exclusive.
- **Structured/config membership needs exact content.** Keep `minify:"none"`; fetch the enclosing object or a small full file. A partial slice cannot establish that a field is absent; follow relevant continuations before an absence claim.
- **Inspect the returned envelope.** Use fields present in the operation's response, including `meta.evidence`, diagnostics, pagination, and `next` continuations; never invent a field or quote.
- **Clone only when it pays** — repeated reads, AST/structural matching, or LSP. A single remote read should stay remote.

## Query forms

```bash
npx octocode tools ghSearch --queries '{"operation":"code","owner":"OWNER","repo":"REPO","keywords":["TERM"],"match":"path"}'
npx octocode tools ghSearch --queries '{"operation":"repositories","keywords":["TERM"],"concise":true}'
npx octocode tools ghSearch --queries '{"operation":"tree","owner":"OWNER","repo":"REPO","branch":"SHA","path":"PATH"}'
npx octocode tools ghGetFileContent --queries '{"owner":"OWNER","repo":"REPO","path":"PATH","branch":"SHA","matchString":"SYMBOL","contextLines":8}'
npx octocode tools ghGetFileContent --queries '{"owner":"OWNER","repo":"REPO","path":"PATH","branch":"SHA","minify":"symbols"}'
npx octocode tools ghSearchHistory --queries '{"operation":"commits","owner":"OWNER","repo":"REPO","path":"PATH"}'
npx octocode tools artifactSearch --queries '{"type":"npm","packageName":"@octokit/rest"}'
npx octocode tools artifactSearch --queries '{"type":"crates","keywords":["async","runtime"]}'
npx octocode tools ghGetHistoryItem --queries '{"operation":"pullRequest","owner":"OWNER","repo":"REPO","number":123,"content":{"body":true}}'
```

Errors are self-correcting — a missing/typo'd field returns a guiding message (for example, *"Repository
scope requires owner"*); fix and retry. For a field this primer doesn't cover, `npx octocode
tools <name> --scheme --brief` prints the compact schema (that call is measured). Freeze every
mutable ref (branch/PR-state/SHA + UTC) before answering; use the frozen ref.

## Critical pitfalls

### 1 — File counts and gitignore

`localSearch` respects `.gitignore` by default. If a question asks to count **all** files
(including build artifacts, dist, out, etc.) use `"noIgnore": true`; otherwise the count will
be lower than a plain `grep -r` or `find` which ignores `.gitignore`.

```bash
# Without noIgnore — only unignored files (default):
npx octocode tools localSearch --queries '{"searchText":"TERM","path":"/repo","resultView":"files"}'

# With noIgnore — matches grep -r behaviour:
npx octocode tools localSearch --queries '{"searchText":"TERM","path":"/repo","resultView":"files","noIgnore":true}'
```

When the question says "how many files contain X" without specifying gitignore policy, default
behavior (`.gitignore` respected) is the correct interpretation for source code questions.
Use `noIgnore:true` only when the question explicitly includes build artifacts.

### 2 — Structural matching with surrounding fields (`$$$`)

When a TypeScript interface (or class) may have **other fields before or after** the target
field, you must wrap the field with `$$$` wildcards — otherwise the pattern only matches
objects with that field as the sole member.

```bash
# WRONG — only matches interfaces where `name` is the only field:
npx octocode tools astSearch --queries '{"operation":"match","path":"/repo","langType":"typescript","pattern":"interface $I { name: $T; }"}'

# CORRECT — matches interfaces that contain `name` plus any other fields:
npx octocode tools astSearch --queries '{"operation":"match","path":"/repo","langType":"typescript","pattern":"interface $I { $$$ name: $T; $$$ }"}'
```

The same rule applies to object literals, class bodies, and function parameter objects:
always add `$$$` before and after the field you care about when other fields may exist.

### 3 — Largest file by line count (not bytes)

`astSearch operation:files` with `detail:"basic"` or `detail:"full"` returns `size` in **bytes**.
`sort:"size"` sorts by bytes — that is NOT the same as line count for large files.

To find the file with the most lines:

```bash
# Returns files sorted by line count (lineCount field appears in output when detail:full):
npx octocode tools astSearch --queries '{"operation":"files","path":"/repo","detail":"full","sort":"lines","entryType":"f"}'
```

Alternatively, to get line count for a specific file:

```bash
npx octocode tools localFetch --queries '{"path":"/repo/file.ts","fullContent":false,"limit":1,"offset":0}'
# Check `totalLines` in the response pagination — this is the exact line count.
```

### 4 — Use `callers` (not `references` or `localSearch`) to get calling function names

When asked "which functions call X" or "how many callers does X have", use
`lspSearch operation:callers` — it returns the **calling function name** directly in
`item.name`, the file in `item.uri`, and the call site in `ranges[].line`.

Using `localSearch` only gives text matches with no enclosing function context.
Using `lspSearch operation:references` gives all textual references (including type
annotations, re-exports, and imports) — not just call sites.

```bash
# WRONG — text hits, no function name:
npx octocode tools localSearch --queries '{"searchText":"buildNextPageContinuation","path":"/repo","resultView":"files"}'

# CORRECT — returns calling function name, file, and call-site line:
npx octocode tools lspSearch --queries '{"operation":"callers","uri":"file:///repo/scheme/pagination.ts","symbolName":"buildNextPageContinuation","lineHint":118}'
# Each result: item.name=<calling function>, item.uri=<file>, ranges[].line=<call site line>
```

### 5 — Native engine boundary: LSP cannot trace `loadNative()`

For call chains that reach the native Rust/NAPI engine, the TypeScript LSP
**stops at the dynamic `loadNative()` call** and cannot trace into the native module.

The pattern in `ripgrepExecutor.ts`:
```typescript
const ctx = await contextUtils.searchRipgrep(...);
// contextUtils is resolved via loadNative() at runtime
```

When tracing a call chain to the native boundary:
1. Follow LSP callees as far as possible
2. When you reach a file that calls `contextUtils.*` or `acquireNativeContext().*`,
   that IS the native boundary — the next hop goes into the Rust binary
3. Verify by reading `ripgrepExecutor.ts` around the call site and looking for
   `loadNative()` or `contextUtils` which are the NAPI bridge

### 6 — Type aliases shadow the original type in `references`

When a file declares `type PublicArgs = OriginalType<...>`, any code that uses
`PublicArgs` will NOT appear in `lspSearch operation:references` results for `OriginalType`.
The alias is a separate symbol.

Always also search for type aliases:
```bash
# Step 1: find all usages of the original type
npx octocode tools lspSearch --queries '{"operation":"references","uri":"...","symbolName":"OriginalType","lineHint":N}'

# Step 2: search for type aliases that equal the original type
npx octocode tools localSearch --queries '{"searchText":"= OriginalType","path":"/repo","include":["*.ts"],"resultView":"paginated"}'

# Step 3: for each alias found, run references on the alias too
npx octocode tools lspSearch --queries '{"operation":"references","uri":"...","symbolName":"AliasName","lineHint":N}'
```

### 7 — Topology edges now include `importLine` and `inboundCount`

`astSearch operation:topology analysis:dependents` and `analysis:dependencies` now
return two additional fields per result item:

- **`importLine`**: the exact line number in the importer file where the `import` statement appears
- **`inboundCount`**: how many files in the scanned graph import this file (in-degree)

Use `inboundCount` to answer “which files are most imported within this subtree”:
```bash
# Run dependents from the subtree root, then sort by inboundCount in the results
npx octocode tools astSearch --queries '{"operation":"topology","analysis":"dependents","file":"/repo/subtree/index.ts","path":"/repo/subtree","depth":5}'
# Look at inboundCount on each result — highest = most imported within the scan scope
```

Use `importLine` to answer “where exactly is the import in the dependent file”:
```bash
# importLine appears in each result item alongside file, edgeKinds, via
# e.g.: { file: "tools/ast_search/execution.ts", importLine: 7, edgeKinds: ["static-import"] }
```

### 8 — Zod validation and exact error messages live in `octocode-core` (sibling repo)

The codebase is split across TWO repos:

```
octocode-mcp-host/packages/octocode-core/src/toolContract/input/resources/tools/
    astFiles.ts       ← AstFilesQuerySchema, superRefine for minDepth/maxDepth
    astSearch.ts      ← AstSearchQuerySchema
    localFetch.ts     ← FetchContentQuery schema + validation
    _toolkit.ts       ← pageNumber(), intRange(), MAX_LOCAL_ITEMS_PER_PAGE ...

octocode/packages/octocode-tools-core/src/tools/ast_search/filesystem/
    files.ts          ← runtime: native calls, result formatting, pagination
    tree.ts           ← runtime: directory traversal output
```

**When looking for:**
- exact Zod error message strings (e.g. `"minDepth must be less than or equal to maxDepth."`)
- `superRefine` validation guards
- field constraints, `pageNumber()`, `intRange()` helpers
- `_toolkit.ts` shared schema utilities

**Search in:** `octocode-mcp-host/packages/octocode-core/src/toolContract/` — NOT `packages/octocode-tools-core/src/`

```bash
# WRONG — astFiles.ts is not here:
npx octocode tools localFetch --queries '{"path":"/Users/.../octocode/packages/octocode-tools-core/src/tools/ast_search/filesystem/astFiles.ts"}'

# CORRECT:
npx octocode tools localFetch --queries '{"path":"/Users/.../octocode-mcp-host/packages/octocode-core/src/toolContract/input/resources/tools/astFiles.ts","fullContent":true}'
```
