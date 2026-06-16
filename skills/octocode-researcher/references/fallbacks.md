# Fallback Tiers — When Octocode MCP Is Not Available

Use this reference when Octocode MCP tools are not detected. See [SKILL.md §2](../SKILL.md) for detection logic.

---

## Tier 2: `gh` CLI + Linux Commands

**Detection**: Run `gh --version` and `gh auth status`.

### External (GitHub via `gh` CLI)

| Octocode Tool | `gh` CLI Equivalent |
|---------------|---------------------|
| `ghSearchCode` | `gh search code "PATTERN" --repo OWNER/REPO --language LANG` |
| `ghSearchRepos` | `gh search repos "QUERY" --language LANG --sort stars` |
| `ghViewRepoStructure` | `gh api repos/OWNER/REPO/git/trees/BRANCH?recursive=1 --jq '.tree[].path'` |
| `ghGetFileContent` | `gh api repos/OWNER/REPO/contents/PATH --jq '.content' \| base64 -d` |
| `ghSearchPRs` | `gh search prs "QUERY" --repo OWNER/REPO --state merged` |
| `npmSearch` | `npm search KEYWORD --json` |
| `ghCloneRepo` | `git clone --depth 1 https://github.com/OWNER/REPO.git /tmp/research-REPO` |
| `ghCloneRepo` (sparse) | `git clone --depth 1 --filter=blob:none --sparse URL /tmp/research-REPO && cd /tmp/research-REPO && git sparse-checkout set TARGET_DIR` |

**`gh` CLI tips**:
- `gh search code` qualifiers: `--filename`, `--extension`, `--language`, `--repo`
- `gh api` returns JSON — use `--jq` for filtering (e.g., `--jq '.[].name'`)
- `gh pr view NUMBER --repo OWNER/REPO` for details, `--json files` for changed files
- `gh pr diff NUMBER --repo OWNER/REPO` to see diffs directly
- Pagination: `--limit N` for search, `--paginate` for api

### Local (Linux commands / agent tools)

| Octocode Tool | Linux / Agent Equivalent |
|---------------|--------------------------|
| `localViewStructure` | `ls -la PATH` or `tree -L DEPTH PATH` or agent `Glob` |
| `localSearchCode` | `rg "PATTERN" PATH --type TYPE -n` or agent `Grep` |
| `localFindFiles` | `find PATH -name "PATTERN" -mtime -DAYS -size +SIZE` or agent `Glob` |
| `localGetFileContent` | `head -n N PATH` / `sed -n 'START,ENDp' PATH` or agent `Read` |

**Linux tips**:
- `rg` (ripgrep) preferred — faster, respects `.gitignore`, better regex
- `rg -l "PATTERN" PATH` = files only (like `filesOnly: true`)
- `rg -c "PATTERN" PATH` = match counts per file
- `find PATH -name "*.ts" -newer FILE` = files modified after reference
- `tree -I 'node_modules|dist' -L 2` = structure excluding dirs

### Clone (Tier 2)

```bash
# Full shallow clone
git clone --depth 1 https://github.com/OWNER/REPO.git /tmp/research-REPO

# Sparse shallow clone (monorepo)
git clone --depth 1 --filter=blob:none --sparse https://github.com/OWNER/REPO.git /tmp/research-REPO
cd /tmp/research-REPO && git sparse-checkout set src/target-dir
```

Then use `rg`, `Grep`, `Read` on the cloned path. Clean up `/tmp/` clones after research.

### Compensating for Missing Octocode Features

| Missing | Compensation |
|---------|-------------|
| Hints | Manually plan next steps using the Transition Matrix in SKILL.md §5 |
| Pagination | Use `--limit`, `head`, `tail` to manage output size |
| LSP (defs, refs, calls) | `rg` pattern matching to trace manually — **no true equivalent** |
| Research context | Track goals in task management tools |
| Structured results | Parse raw output manually |

> **LSP semantic analysis has no Linux equivalent.** Only Octocode MCP or IDE-native tools provide go-to-definition, find-references, and call-hierarchy.

---

## Tier 3: Agent Default Tools Only

If neither Octocode MCP nor `gh` CLI is available:

| Need | Agent Tool |
|------|------------|
| Search code content | `Grep` (ripgrep-based) |
| Find files by pattern | `Glob` |
| Read file content | `Read` |
| Fetch web content | `WebFetch` |
| Run commands | `Shell` (`npm search`, `git log`, etc.) |

---

## Tier 2/3 Verification Checklist

**Local research**:
- [ ] Used `rg` / `Grep` for search before reading files
- [ ] Used `Read` / `sed -n` LAST (after search)
- [ ] Managed output size with `head`, `tail`, `--limit`

**External research**:
- [ ] Used `gh search repos` or `npm search` to find repos (not guessed)
- [ ] Used `gh api .../trees` to explore structure before reading
- [ ] Used `gh api .../contents` LAST (after exploration)
- [ ] References include full GitHub URLs with line numbers

---

## Why Octocode Over Alternatives

| Instead of... | Octocode | Why Better |
|---------------|----------|------------|
| `grep`, `rg` | `localSearchCode` | Structured results, pagination, hints, byte offsets |
| `ls`, `tree` | `localViewStructure` | Filtering, sorting, depth control, summaries |
| `find` | `localFindFiles` | Time/size/permission filters, pagination |
| `cat`, `head` | `localGetFileContent` | matchString targeting, context lines, pagination |
| `gh api` | `ghSearchCode` | Structured results, hints, pagination |
| `npm search` | `npmSearch` | npm registry, structured metadata |
| `WebFetch` (GitHub) | `ghViewRepoStructure` | Tree view, no HTML parsing |

**Exclusive Octocode benefits** (unavailable in Tier 2/3):
- Structured JSON with hints for next steps
- Automatic pagination for token management
- `.gitignore` respect with `noIgnore` option
- Research params (`mainResearchGoal`, `researchGoal`, `reasoning`)
- LSP semantic analysis (go-to-definition, find-references, call-hierarchy)
