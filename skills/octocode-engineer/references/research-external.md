# Research External — GitHub, npm, Cross-Repo Checks

Use this when code is not local yet, when starting from an npm package, when comparing implementations across repos, or when history/PR rationale matters.

## GitHub/npm CLI-first map

| Job | CLI | MCP | Notes |
|---|---|---|---|
| npm package → repo | `npx octocode search <package> --target packages` | `npmSearch` through OQL `target:"packages"` | Best start for packages; returns repo and monorepo directory |
| Discover repos | `npx octocode search <keywords> --target repositories` | `ghSearchRepos` | Use `--concise` first for lean owner/repo list |
| Map repo tree | `npx octocode search <owner/repo> --tree` | `ghViewRepoStructure` | Start shallow, then drill into source/subpackage |
| GitHub code/path search | `npx octocode search <kw> <owner/repo>` | `ghSearchCode` through OQL `target:"code"` | Discovery only; snippets are not proof |
| Fetch GitHub file | `npx octocode search <owner/repo/path> --content-view exact\|compact\|symbols` | `ghGetFileContent` | Use `--content-view symbols`, `--match-string`, line ranges |
| PR review/history | `npx octocode search <owner/repo[#N]> --target pullRequests` | `ghHistoryResearch` | List/search/deep-read PRs, comments, patches, reviews |
| Commit history | `npx octocode search <owner/repo[/path]> --target commits` | `ghHistoryResearch` | File/dir/repo archaeology; extract PR numbers |
| Clone for local proof | `npx octocode clone <owner/repo[/path]>` | `ghCloneRepo` | Required for AST/LSP/deep multi-file analysis |
| Cache remote proof locally | `npx octocode cache fetch <owner/repo> <path> --depth file\|tree\|clone` | `ghGetFileContent(type:"directory")` / `ghCloneRepo` | Reuse returned `localPath` with local tools |

## Starting points

### Package name

```text
npx octocode search <package> --target packages --json
→ take owner/repo and repository directory from metadata
→ npx octocode search <owner/repo[/dir]> --tree --json
→ npx octocode search <owner/repo/path> --content-view symbols --json
→ npx octocode search <owner/repo/path> --match-string ... --content-view exact --json for proof
→ clone/cache if >3 files or AST/LSP needed
```

### Concept / unknown repo

```text
npx octocode search <keywords> --target repositories --lang <lang> --stars '<range>' --concise --json
→ choose owner/repo
→ npx octocode search <owner/repo> --tree --depth 1 --json
→ npx octocode search <likely-file-or-symbol> <owner/repo> --view discovery --json
→ npx octocode search <owner/repo/path> --content-view symbols or --match-string ... --json
```

### Known owner/repo

```text
npx octocode search <owner/repo> --tree --depth 1 --json
→ drill into src/package dir
→ npx octocode search <symbol-or-path> <owner/repo> --view discovery --json
→ npx octocode search <owner/repo/path> --match-string ... --content-view exact --json
```

## GitHub search rules

- `npx octocode search` / `ghSearchCode` is discovery. Snippets are not proof.
- Use `--concise` for lean path lists.
- Use path-oriented search to confirm file existence without snippets when possible.
- Use content snippets, then re-anchor with `npx octocode search --match-string ... --content-view exact`.
- GitHub code search indexes default branch and has result caps; empty ≠ absent.
- Keywords are ANDed; alternatives belong in separate batched queries.

## GitHub fetch/read proof

Use `npx octocode search <owner/repo/path>` / `ghGetFileContent` as the remote proof tool:

- `--content-view symbols` — orient on source file skeleton.
- `--match-string` — returns real line anchors in JSON output.
- `--content-view exact` — exact quote/diff evidence.
- `--start-line/--end-line` — known range read.
- `fullContent` — small files only.

If the investigation needs semantic identity, clone and switch to local LSP. Remote GitHub fetch cannot do LSP by itself.

## Change Intent And PR Archaeology

### Find why code exists

```text
npx octocode search <owner/repo[/path]> --target commits --json
→ inspect commit headlines for (#PR)
→ npx octocode search <owner/repo#N> --target pullRequests --json
→ npx octocode search <owner/repo#N> --target pullRequests --patches --file <high-risk-file> --json
```

### Review a PR remotely

```text
npx octocode search <owner/repo> --target pullRequests --match <keywords> --state merged --concise --json
→ npx octocode search <owner/repo#N> --target pullRequests --json
→ npx octocode search <owner/repo#N> --target pullRequests --comments --json
→ npx octocode search <owner/repo#N> --target pullRequests --patches --file <highRiskFile> --json
→ paginate from JSON pagination/hints
```

Rules:
- Fetch changed file list before patches.
- Prefer selected patches over all patches.
- Fetch existing comments before producing findings to avoid duplicates.

## Cross-repo comparison

```text
npx octocode search --target repositories / npx octocode search --target packages for candidates
→ npx octocode search each repo/subdir --tree
→ npx octocode search <term> <repo-or-path> --view discovery for equivalent files
→ npx octocode search <file> --content-view symbols for outlines
→ npx octocode search <file> --match-string ... --content-view exact for exact implementations
→ compare with file:line citations
```

For serious comparison, clone candidates and use local tools:

```text
npx octocode clone owner/repo/path or npx octocode cache fetch owner/repo path --depth tree
→ npx octocode search <localPath> --tree
→ npx octocode search / structural search
→ npx octocode search <file> --content-view symbols / --match-string
→ npx octocode search --op ...
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
