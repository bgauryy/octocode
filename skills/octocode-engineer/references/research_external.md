# Research External — GitHub, npm, Cross-Repo Checks

Use this when code is not local yet, when starting from an npm package, when comparing implementations across repos, or when history/PR rationale matters.

## GitHub/npm CLI-first map

| Job | CLI | MCP | Notes |
|---|---|---|---|
| npm package → repo | `octocode pkg <package>` | `npmSearch` | Best start for packages; returns repo and monorepo directory |
| Discover repos | `octocode repo <keywords>` | `ghSearchRepos` | Use `--concise` first for lean owner/repo list |
| Map repo tree | `octocode ls <owner/repo>` | `ghViewRepoStructure` | Start shallow, then drill into source/subpackage |
| GitHub code/path search | `octocode grep <kw> <owner/repo>` | `ghSearchCode` | Discovery only; snippets are not proof |
| Fetch GitHub file | `octocode cat <owner/repo/path>` | `ghGetFileContent` | Use `--mode symbols`, `--match-string`, line ranges |
| PR review/history | `octocode pr <owner/repo[#N]>` | `ghHistoryResearch` | List/search/deep-read PRs, comments, patches, reviews |
| Commit history | `octocode history <owner/repo[/path]>` | `ghHistoryResearch` | File/dir/repo archaeology; extract PR numbers |
| Clone for local proof | `octocode clone <owner/repo[/path]>` | `ghCloneRepo` | Required for AST/LSP/deep multi-file analysis |
| Cache remote proof locally | `octocode cache fetch <owner/repo> <path> --depth file\|tree\|clone` | `ghGetFileContent(type:"directory")` / `ghCloneRepo` | Reuse returned `localPath` with local tools |

## Starting points

### Package name

```text
octocode pkg <package> --json
→ take owner/repo and repository directory from metadata
→ octocode ls <owner/repo[/dir]> --json
→ octocode cat <owner/repo/path> --mode symbols --json
→ octocode cat <owner/repo/path> --match-string ... --mode none --json for proof
→ clone/cache if >3 files or AST/LSP needed
```

### Concept / unknown repo

```text
octocode repo <keywords> --language <lang> --stars '<range>' --concise --json
→ choose owner/repo
→ octocode ls <owner/repo> --depth 1 --json
→ octocode grep <likely-file-or-symbol> <owner/repo> --concise --json
→ octocode cat <owner/repo/path> --mode symbols or --match-string ... --json
```

### Known owner/repo

```text
octocode ls <owner/repo> --depth 1 --json
→ drill into src/package dir
→ octocode grep <symbol-or-path> <owner/repo> --concise --json
→ octocode cat <owner/repo/path> --match-string ... --mode none --json
```

## GitHub search rules

- `octocode grep` / `ghSearchCode` is discovery. Snippets are not proof.
- Use `--concise` for lean path lists.
- Use path-oriented search to confirm file existence without snippets when possible.
- Use content snippets, then re-anchor with `octocode cat --match-string ... --mode none`.
- GitHub code search indexes default branch and has result caps; empty ≠ absent.
- Keywords are ANDed; alternatives belong in separate batched queries.

## GitHub fetch/read proof

Use `octocode cat` / `ghGetFileContent` as the remote proof tool:

- `--mode symbols` — orient on source file skeleton.
- `--match-string` — returns real line anchors in JSON output.
- `--mode none` — exact quote/diff evidence.
- `--start-line/--end-line` — known range read.
- `fullContent` — small files only.

If the investigation needs semantic identity, clone and switch to local LSP. Remote GitHub fetch cannot do LSP by itself.

## History and PR archaeology

### Find why code exists

```text
octocode history <owner/repo[/path]> --json
→ inspect commit headlines for (#PR)
→ octocode pr <owner/repo#N> --json
→ octocode pr <owner/repo#N> --patches --file <high-risk-file> --json
```

### Review a PR remotely

```text
octocode pr <owner/repo> --query <keywords> --state merged --concise --json
→ octocode pr <owner/repo#N> --json
→ octocode pr <owner/repo#N> --comments --json
→ octocode pr <owner/repo#N> --patches --file <highRiskFile> --json
→ paginate from JSON pagination/hints
```

Rules:
- Fetch changed file list before patches.
- Prefer selected patches over all patches.
- Fetch existing comments before producing findings to avoid duplicates.

## Cross-repo comparison

```text
octocode repo / octocode pkg for candidates
→ octocode ls each repo/subdir
→ octocode grep --concise for equivalent files
→ octocode cat --mode symbols for outlines
→ octocode cat --match-string ... --mode none for exact implementations
→ compare with file:line citations
```

For serious comparison, clone candidates and use local tools:

```text
octocode clone owner/repo/path or octocode cache fetch owner/repo path --depth tree
→ octocode ls <localPath>
→ octocode grep / structural grep
→ octocode cat --mode symbols / --match-string
→ octocode lsp
```

## Remote → local handoff triggers

Clone when:
- analysis spans more than ~3 files in one repo;
- AST structural search is needed;
- LSP definition/references/call hierarchy is needed;
- you need to inspect generated/local package relationships;
- GitHub search is capped/noisy/empty but structure suggests code exists.
