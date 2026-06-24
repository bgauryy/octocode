# Research External — GitHub, npm, Cross-Repo Checks

Use this when code is not local yet, when starting from an npm package, when comparing implementations across repos, or when history/PR rationale matters.

## GitHub/npm CLI-first map

| Job | CLI | MCP | Notes |
|---|---|---|---|
| npm package → repo | `octocode search <package> --target packages` | `npmSearch` through OQL `target:"packages"` | Best start for packages; returns repo and monorepo directory |
| Discover repos | `octocode search <keywords> --target repositories` | `ghSearchRepos` | Use `--concise` first for lean owner/repo list |
| Map repo tree | `octocode search <owner/repo> --tree` | `ghViewRepoStructure` | Start shallow, then drill into source/subpackage |
| GitHub code/path search | `octocode search <kw> <owner/repo>` | `ghSearchCode` through OQL `target:"code"` | Discovery only; snippets are not proof |
| Fetch GitHub file | `octocode search <owner/repo/path> --content-view exact\|compact\|symbols` | `ghGetFileContent` | Use `--content-view symbols`, `--match-string`, line ranges |
| PR review/history | `octocode pr <owner/repo[#N]>` | `ghHistoryResearch` | List/search/deep-read PRs, comments, patches, reviews |
| Commit history | `octocode search <owner/repo[/path]> --target commits` | `ghHistoryResearch` | File/dir/repo archaeology; extract PR numbers |
| Clone for local proof | `octocode clone <owner/repo[/path]>` | `ghCloneRepo` | Required for AST/LSP/deep multi-file analysis |
| Cache remote proof locally | `octocode cache fetch <owner/repo> <path> --depth file\|tree\|clone` | `ghGetFileContent(type:"directory")` / `ghCloneRepo` | Reuse returned `localPath` with local tools |

## Starting points

### Package name

```text
octocode search <package> --target packages --json
→ take owner/repo and repository directory from metadata
→ octocode search <owner/repo[/dir]> --tree --json
→ octocode search <owner/repo/path> --content-view symbols --json
→ octocode search <owner/repo/path> --match-string ... --content-view exact --json for proof
→ clone/cache if >3 files or AST/LSP needed
```

### Concept / unknown repo

```text
octocode search <keywords> --target repositories --lang <lang> --stars '<range>' --concise --json
→ choose owner/repo
→ octocode search <owner/repo> --tree --depth 1 --json
→ octocode search <likely-file-or-symbol> <owner/repo> --view discovery --json
→ octocode search <owner/repo/path> --content-view symbols or --match-string ... --json
```

### Known owner/repo

```text
octocode search <owner/repo> --tree --depth 1 --json
→ drill into src/package dir
→ octocode search <symbol-or-path> <owner/repo> --view discovery --json
→ octocode search <owner/repo/path> --match-string ... --content-view exact --json
```

## GitHub search rules

- `octocode search` / `ghSearchCode` is discovery. Snippets are not proof.
- Use `--concise` for lean path lists.
- Use path-oriented search to confirm file existence without snippets when possible.
- Use content snippets, then re-anchor with `octocode search --match-string ... --content-view exact`.
- GitHub code search indexes default branch and has result caps; empty ≠ absent.
- Keywords are ANDed; alternatives belong in separate batched queries.

## GitHub fetch/read proof

Use `octocode search <owner/repo/path>` / `ghGetFileContent` as the remote proof tool:

- `--content-view symbols` — orient on source file skeleton.
- `--match-string` — returns real line anchors in JSON output.
- `--content-view exact` — exact quote/diff evidence.
- `--start-line/--end-line` — known range read.
- `fullContent` — small files only.

If the investigation needs semantic identity, clone and switch to local LSP. Remote GitHub fetch cannot do LSP by itself.

## History and PR archaeology

### Find why code exists

```text
octocode search <owner/repo[/path]> --target commits --json
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
octocode search --target repositories / octocode search --target packages for candidates
→ octocode search each repo/subdir --tree
→ octocode search <term> <repo-or-path> --view discovery for equivalent files
→ octocode search <file> --content-view symbols for outlines
→ octocode search <file> --match-string ... --content-view exact for exact implementations
→ compare with file:line citations
```

For serious comparison, clone candidates and use local tools:

```text
octocode clone owner/repo/path or octocode cache fetch owner/repo path --depth tree
→ octocode search <localPath> --tree
→ octocode search / structural search
→ octocode search <file> --content-view symbols / --match-string
→ octocode search --op ...
```

## Remote → local handoff triggers

Clone when:
- analysis spans more than ~3 files in one repo;
- AST structural search is needed;
- LSP definition/references/call hierarchy is needed;
- you need to inspect generated/local package relationships;
- GitHub search is capped/noisy/empty but structure suggests code exists.

---

## Docs

- [GitHub Tools Reference](https://github.com/bgauryy/octocode/blob/main/docs/mcp/tools/GITHUB_TOOLS.md)
- [Clone & Local Tools Workflow](https://github.com/bgauryy/octocode/blob/main/docs/mcp/CLONE_WORKFLOW.md)
- [Tool Behavior Guide](https://github.com/bgauryy/octocode/blob/main/docs/mcp/tools/TOOL_BEHAVIOR.md)
