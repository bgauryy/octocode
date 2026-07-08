<tools>
Prefer Octocode-native tools over shell (`grep`/`find`/`cat`/`curl`). **Batch** independent calls in one `queries[]`. Follow `hasMore`/`isPartial` continuations exactly — never calculate offsets. Denied call = user declined; adjust, do not retry.

**When docs or skills say "use the octocode tools"** — use the **built-in Pi native tool functions** (`ghSearchCode`, `localSearchCode`, `lspGetSemantics`, etc.) directly. Do **not** shell out to `node $OCTOCODE_CLI tools <name>` for research; the CLI tool-runner is a last resort only when a native tool is unavailable or insufficient.

**Core** — `bash`, `edit`, `write` (new / rewrites)
- Use `edit` for targeted replacements in existing files — it requires exact current text so stale reads are caught before damage is done.
- Use `write` only when creating a new file or intentionally replacing all content; it overwrites without a match guard.

**Local — read & search**
- `localViewStructure` — cheapest orientation; directory tree before reading any file
- `localSearchCode` — text/regex/AST search; modes: `discovery` (paths) · `paginated` (snippets) · `detailed` (context) · `structural` (AST); use AST to understand code structure before reading bodies
- `localGetFileContent` — only after you know the target (search candidate, matchString, symbol, or line range); use `symbols`/`standard` first, `none` for edits/citations, whole file only when needed
- `localFindFiles` — find files by name/size/time/permissions; use when path is known, contents don't matter
- `localBinaryInspect` — inspect/list/extract archives, binaries, compressed streams; modes: `inspect` · `list` · `extract` · `decompress` · `strings` · `unpack`; for full archive unpack use `bash: node $OCTOCODE_CLI unzip <archive>`
- `lspGetSemantics` — symbol identity, definitions, references, callers, types, diagnostics; MUST use for code connections; `lineHint` MUST come from a prior search/AST/doc-symbol anchor, never guessed

**GitHub — remote research**
- `ghViewRepoStructure` — orient a repo tree before fetching files
- `ghSearchCode` — search code contents or paths across GitHub; `match:"path"` for filenames, `match:"file"` for snippets
- `ghGetFileContent` — read a file or region from a GitHub repo; `symbols` → anchor → `none` for edits
- `ghSearchRepos` — discover repos by name/topic/language/stars; start `concise:true`
- `ghHistoryResearch` — search PRs and commit history; `type:"prs"` or `type:"commits"`
- `ghCloneRepo` — clone repo/subtree locally for repeated reads or LSP; use `sparsePath` to bound checkout

**Package & web**
- `npmSearch` — repo/path resolution
- `web` — fetch / search

**Agents**
- `spawnAgent` — background worker; use for large independent work, long-running tasks, or parallel hypotheses; prompt must be self-contained
- `AgentMessage` — coordinate workers: `list` · `status` · `send` · `steer` · `followUp` · `wait` · `kill` · `abort`

**Route summary** — local code/files → local tools · symbol identity/callers/types → LSP · repos/PRs/history → GitHub · packages → npm · live docs/errors → web · builds/VCS/bulk edits → bash
</tools>
