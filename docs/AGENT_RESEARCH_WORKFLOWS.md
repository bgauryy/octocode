# Agent Research Workflows

> Product guidance for agent-facing Octocode workflows: combine package search,
> GitHub code/repo search, fetch, clone, local ripgrep, AST structural search,
> and LSP semantics into one evidence-first research loop.

## Core Rule

Use layered retrieval, not one magic search path:

```text
orient -> search -> fetch exact evidence -> prove -> act
```

Carry anchors forward at every step: package names, owner/repo, branch, path,
line, match text, PR number, localPath, symbol name, and LSP lineHint.

## Product Guidance For Octocode

Octocode should teach agents this decision ladder:

| Need | Start with | Prove with | Avoid |
|------|------------|------------|-------|
| Known package | `npmSearch` / CLI `pkg` | source repo + exact file reads | guessing GitHub repo names |
| Unknown repo | `ghSearchRepos` / CLI `repo` | `ghViewRepoStructure`, README, examples | cloning before scope is known |
| Known remote symbol/string | `ghSearchCode` | `ghGetFileContent` line range or `matchString` | treating snippets as proof |
| One remote file | `ghGetFileContent(type:"file")` | exact minify mode, line range, or match slice | full clone |
| One remote directory | `ghGetFileContent(type:"directory")` | returned `localPath` + local tools | many one-file API reads |
| Deep remote analysis | `ghCloneRepo` | local ripgrep + AST + LSP | GitHub-only semantic claims |
| Local lexical search | `localSearchCode` | `localGetFileContent` exact slice | reading whole files first |
| Code shape | `localSearchCode(mode:"structural")` | AST matches plus exact slices | regex over comments/strings |
| Symbol identity | `lspGetSemantics` | definition/references/callers/callees | LSP without a search anchor |
| Why code changed | `ghHistoryResearch` | direct `prNumber` metadata, files, patches | broad PR comment search first |
| Unified multi-domain query | CLI `search` / OQL | routed backing tool evidence | guessing raw tool fields |
| Remote content cache | CLI `cache fetch` | returned `localPath` + local tools | repeated remote file reads |
| Repository tree shape | `ghViewRepoStructure` / CLI `ls` | targeted file reads | path guessing |
| Local metadata/path search | `localFindFiles` / CLI `find` | exact file slices | content search for filenames only |
| Local directory shape | `localViewStructure` / CLI `ls` | find/search/read follow-ups | recursive whole-file reads |
| Archive or binary artifact | `localBinaryInspect` / CLI `binary` or `unzip` | archive listing, unpacked `localPath`, nested binary inspection, local searches | treating opaque assets as plain text |
| Diff or selected patch | CLI `diff` or `ghHistoryResearch` selected patches | exact patch/file slice | full PR dumps |

## CLI Command Map

Quick commands are the fastest path for humans and agents. Raw `tools` calls are
schema-exact and should be used when the quick command cannot express the
needed field.

| CLI command | Backing workflow | Use when |
|-------------|------------------|----------|
| `search` | OQL router across local, GitHub, npm, PRs, commits, artifacts, diff | One typed query should choose the backing tool and return continuations |
| `pkg` | `npmSearch` | Starting from an npm package or package keyword |
| `repo` | `ghSearchRepos` | Discovering candidate repositories |
| `ls` | `localViewStructure` or `ghViewRepoStructure` | Orienting by tree shape or symbol outline |
| `grep` | `localSearchCode` or `ghSearchCode` | Searching text, regex, paths, or local AST patterns |
| `cat` | `localGetFileContent` or `ghGetFileContent` | Fetching exact file evidence |
| `find` | `localFindFiles` or remote file discovery | Locating files by name, path, metadata, or content |
| `lsp` | `lspGetSemantics` | Definitions, references, call hierarchy, hover, type info |
| `pr` | `ghHistoryResearch` | PR search or direct PR inspection |
| `history` | `ghHistoryResearch(type:"commits")` | Commit history for a repo, directory, or file |
| `clone` | `ghCloneRepo` | Full repo or sparse subtree materialization |
| `cache fetch` | clone/file/tree materialization lane | Saving remote content locally for local tools |
| `binary` | `localBinaryInspect` | Inspecting binaries, listing/extracting archives, decompressing streams, or reading strings |
| `unzip` | `localBinaryInspect(mode:"unpack")` | Unpacking archives, then continuing with `ls`/`find`/`grep`/`cat`/`lsp` and `binary` for nested artifacts |
| `diff` | direct file diff or PR patch workflow | Comparing files or inspecting selected changes |

## Best Workflows

### 1. Package To Source To Evidence

Use this when the user names a library, npm package, or framework feature.

```text
npmSearch(packageName)
-> take repository owner/repo from package metadata
-> ghViewRepoStructure(owner, repo, path="", depth=1)
-> ghSearchCode(owner, repo, keywordsToSearch=[distinctive symbol])
-> ghGetFileContent(owner, repo, path, matchString/startLine/endLine)
-> ghCloneRepo only if local AST/LSP proof is needed
```

CLI equivalent:

```bash
octocode pkg <package>
octocode ls <owner/repo>
octocode grep <symbol> <owner/repo>
octocode cat <owner/repo/path>
octocode clone <owner/repo[/subpath]>
```

### 2. Repo Discovery To Pattern Examples

Use this when the agent needs prior art, examples, or comparable
implementations across GitHub.

```text
ghSearchRepos(keywordsToSearch, topicsToSearch?, language?, stars?)
-> shortlist active/relevant repositories
-> ghSearchCode(owner, repo, keywordsToSearch, extension/path filters)
-> ghGetFileContent for exact examples
-> ghHistoryResearch when the reason or evolution matters
```

Start with `ghSearchRepos`; use `ghSearchCode` after you know which repositories
or owners are worth reading. This keeps broad GitHub search from becoming noisy.

### 3. GitHub Code Search To Fetch

Use this when a remote repo is known and the agent needs exact evidence.

```text
ghSearchCode(owner, repo, keywordsToSearch, extension?, path?)
-> read result status and pagination
-> ghGetFileContent(owner, repo, path, matchString or line range)
-> cite the fetched file slice, not the search snippet
```

Rules:

- Use distinctive identifiers and scope by owner/repo as soon as possible.
- If search is empty, verify spelling, branch, path, extension, and filters before
  calling it absence.
- Fetch one file or slice before cloning.

### 4. Remote To Local Bridge

Use this when remote research needs local-only power: ripgrep controls,
structural search, file metadata, binary inspection, or LSP.

```text
ghViewRepoStructure(owner, repo, path, depth)
-> choose materialization:
   - ghGetFileContent(type:"directory") for one small directory
   - ghCloneRepo for full project, sparse subtree, AST, or LSP
-> localViewStructure(localPath)
-> localSearchCode(localPath)
-> localSearchCode(mode:"structural") when shape matters
-> lspGetSemantics(uri, symbolName, lineHint) when identity matters
```

Clone only when the clone cost buys something: project context, many files,
AST/LSP proof, call tracing, or repeated local searches.

### 5. Local Repo Investigation

Use this inside a checked-out workspace or cloned repo.

```text
localViewStructure(path, recursive=true, maxDepth=1)
-> localFindFiles(path, names/pathPattern/modifiedWithin)
-> localSearchCode(path, keywords, mode:"discovery" or filesOnly=true)
-> localGetFileContent(path, matchString/startLine/endLine)
-> localSearchCode(mode:"structural", pattern/rule) for syntax-aware proof
-> lspGetSemantics(type, uri, symbolName, lineHint) for semantic proof
```

Use ripgrep-style search for cheap anchors. Use AST when the claim is about code
shape. Use LSP after a line anchor exists.

### 6. Change History And Intent

Use this when the user asks why a behavior exists or when a regression may come
from a prior PR.

```text
ghSearchCode(owner, repo, keywordsToSearch)
-> ghHistoryResearch(owner, repo, query, matchScope=["title","body"])
-> ghHistoryResearch(owner, repo, prNumber, content={metadata, changedFiles})
-> selected patches/comments only when needed
-> clone + local tools if current code needs semantic follow-up
```

Prefer title/body PR search before broad comment search. When a PR number is
known, jump directly to `prNumber`.

### 7. Unified Search Router

Use this when an agent should express the research target once and let Octocode
route to the right backing tool.

```text
search(target:"packages", from:{kind:"npm"}, params:{packageName})
-> search(target:"repositories", from:{kind:"github"}, params:{keywords})
-> search(target:"code", from:{kind:"github" or "local"}, where:{kind:"text"|"regex"|"structural"})
-> search(target:"content", fetch:{content:{...}})
-> follow next.* continuations or rerun with --explain when routing is unclear
```

Use raw tools instead when the agent needs a tool-specific field, exact pagination
control, or a schema feature not yet modeled by OQL.

### 8. Cache Fetch To Local Proof

Use this when remote code should become local evidence without committing to a
full manual clone workflow. This is a CLI-only convenience surface over the
underlying GitHub materialization tools: it checks existing local tmp/cache
materialization first and uses `--force-refresh` only when the cache must be
bypassed.

```text
cache fetch owner/repo path --depth file|tree|clone
-> cache hit? reuse localPath; otherwise materialize into tmp/tree or tmp/clone
-> capture localPath, repoRoot, cached, complete, and resolvedBranch
-> localViewStructure(localPath)
-> localFindFiles(localPath)
-> localSearchCode(localPath)
-> localGetFileContent(localPath + file)
-> lspGetSemantics(uri under repoRoot) when project context is present
```

Use `--depth file` for a single file, `--depth tree` for a directory, and
`--depth clone` for project-wide local search or LSP. With no explicit depth,
the CLI defaults to `file` when a path is supplied and `clone` when no path is
supplied.

### 9. Artifacts, Archives, And Binaries

Use this when useful code or evidence is inside a compressed artifact, archive,
or native binary.

```text
localBinaryInspect(mode:"inspect") for native metadata
-> localBinaryInspect(mode:"list") before extracting archives
-> localBinaryInspect(mode:"strings") for printable identifiers, URLs, symbols
-> localBinaryInspect(mode:"extract") for one archive entry
-> localBinaryInspect(mode:"unpack") or CLI unzip for multi-file archive research
-> localViewStructure/localFindFiles/localSearchCode over returned localPath
-> localGetFileContent for exact text evidence
-> localBinaryInspect again for nested .node/.so/.dll/.wasm/.zip artifacts
```

List before extract. Use `strings` to find anchors. Use `unzip` when the next
step is normal local research across many unpacked files; use `binary --extract`
when only one archive member matters.

### 10. Unzip To Binary And Local Search

Use this when an archive may contain source, generated bundles, nested archives,
or native binaries.

```text
unzip artifact.zip
-> capture localPath
-> ls localPath
-> find localPath for likely source, manifests, nested archives, and binaries
-> grep localPath for identifiers, secrets, URLs, errors, imports, or config keys
-> cat exact matching files or slices
-> binary nested-artifact for inspect/list/strings/decompress/extract
-> lsp localPath/file when unpacked source has project context
```

This turns opaque release artifacts into normal local research surfaces. Search
the unpacked tree first, then inspect nested binary/archive files only when a
path or string anchor makes them relevant.

### 11. Diff And Patch Review

Use this when the question is about what changed between two files, refs, or PR
states.

```text
diff(left, right) for direct file comparison
-> ghHistoryResearch(prNumber, content={metadata, changedFiles})
-> ghHistoryResearch(prNumber, content={patches:{mode:"selected", files:[...]}})
-> ghGetFileContent or localGetFileContent for current exact source
```

Prefer selected patch files or ranges over full PR patches. Use the current file
content to separate "what changed" from "what exists now".

## Evidence Gates

- Search snippets are discovery, not proof. Fetch exact source before making a
  claim.
- Empty status is not absence until scope, spelling, branch, filters, and
  pagination are checked.
- AST proves syntax shape, not runtime behavior or types.
- LSP proves semantic identity when the server is available and configured; an
  empty or unavailable server is inconclusive.
- Semantic or embedding search is useful for conceptual discovery, but every hit
  must be proved through exact file content, AST, LSP, PR history, or tests.
- Use pagination and match windows before expanding scope.
- Batch independent queries; serialize dependent steps that rely on returned
  anchors.

## Workflow Defaults For Agents

| Phase | Default behavior |
|-------|------------------|
| First pass | `concise:true`, path-only, `mode:"discovery"`, shallow tree depth |
| Reading | `matchString`, line ranges, or `minify:"symbols"` before whole files |
| Local search | literal/fixed-string search before broad regex |
| Structural search | `pattern` for simple shapes; YAML `rule` for relational logic |
| LSP | search first, then pass `uri`, `symbolName`, and `lineHint` |
| Remote research | package/repo/code search first; clone only for local proof |
| Materialization | `cache fetch` or `ghCloneRepo`; capture `localPath` and continue locally |
| Artifacts | inspect/list/strings first; `unzip`/unpack before local search; run `binary` again on nested artifacts |
| Diffs | direct `diff` for file comparison; selected PR patches for review history |
| Reporting | cite fetched files, PRs, package metadata, or exact local paths/lines |

## References

Internal Octocode references:

- [GitHub Tools Reference](https://github.com/bgauryy/octocode/blob/main/docs/mcp/tools/GITHUB_TOOLS.md)
- [Local Tools Reference](https://github.com/bgauryy/octocode/blob/main/docs/mcp/tools/LOCAL_TOOLS.md)
- [LSP Tools Reference](https://github.com/bgauryy/octocode/blob/main/docs/mcp/tools/LSP_TOOLS.md)
- [Clone & Local Tools Workflow](https://github.com/bgauryy/octocode/blob/main/docs/mcp/CLONE_WORKFLOW.md)
- [Tool Behavior Guide](https://github.com/bgauryy/octocode/blob/main/docs/mcp/tools/TOOL_BEHAVIOR.md)
- [CLI Reference](https://github.com/bgauryy/octocode/blob/main/docs/cli/REFERENCE.md)
- [Binary Tools Reference](https://github.com/bgauryy/octocode/blob/main/docs/mcp/tools/BINARY_TOOLS.md)
- [Octocode Query Language](https://github.com/bgauryy/octocode/blob/main/docs/octocode-language/OCTOCODE_QUERY_LANGUAGE.md)

External references:

- [GitHub Code Search syntax](https://docs.github.com/en/search-github/github-code-search/understanding-github-code-search-syntax)
- [GitHub repository search](https://docs.github.com/en/search-github/searching-on-github/searching-for-repositories)
- [npm package search guidance](https://docs.npmjs.com/searching-for-and-choosing-packages-to-download/)
- [ripgrep](https://github.com/BurntSushi/ripgrep)
- [ast-grep AI prompting](https://ast-grep.github.io/advanced/prompting.html)
- [Tree-sitter introduction](https://tree-sitter.github.io/tree-sitter/)
- [Language Server Protocol](https://microsoft.github.io/language-server-protocol/)
