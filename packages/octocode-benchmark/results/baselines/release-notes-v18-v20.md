# Tool map: Octocode v18 → v20

Renames and what changed per tool, for comparing a tool across versions.

| v18 tool | v20 tool | Family | Change |
|---|---|---|---|
| ghSearchRepos | ghSearchRepo | GitHub | Renamed. Find candidate repositories by keyword, topic, owner, stars or date. |
| ghSearchCode | ghSearchCode | GitHub | Same job. Repo-scoped hits carry line numbers; ref reads hit lines at a tag, SHA or PR head. |
| ghViewRepoStructure | ghStructure | GitHub | Tree at any ref, plus refs (branches and tags with SHAs) and languages; materialize copies files for local tools. |
| ghGetFileContent | ghGetFileContent | GitHub | Same job. Region reads by matchString, line ranges or a symbols outline. |
| ghSearchPullRequests · ghSearchIssues · ghSearchCommits | ghSearchHistory | GitHub | Three searches merged into one, with operation = pullRequest, issue or commit. |
| — | ghGetHistoryItem | GitHub | New. Read one PR, issue, commit or compare range, picking sections such as files, patches or reviews. |
| npmSearch | artifactSearch | Packages | Package versions, dependencies and release source repository. |
| localSearchCode | localSearch | Local | Renamed. Text and regex search with ranked files. |
| localViewStructure | structureSearch | Local | Renamed. Tree view and find-by-name. |
| localGetFileContent | localFetch | Local | Renamed. Read a known file or region, with an rg -n style line gutter. |
| lspGetSemantics | lspSearch | Local | Renamed. Callers, references, types and diagnostics from a language server. |
| — | astSearch | Local | New. Declarations, symbol outlines and structural matches. |
| — | clasify | Semantic | New. Classifies large reads and wide search results with a provider model; listed only when its provider answers at startup. |
| ghCloneRepo | ghCloneRepo | CLI only | Kept in the CLI; not listed by the MCP server. |
| localFindDeadCode | — | Retired | No longer in the MCP tool list. |

Other notes in the old file:

```json
{
 "changes": [
  {
   "area": "Architecture",
   "title": "One native engine",
   "detail": "The CLI and the MCP server are thin interfaces over one Rust runtime, shipped as prebuilt binaries per platform."
  },
  {
   "area": "Output",
   "title": "Minimal responses by default",
   "detail": "Rows carry the answer, open pagination, next.* continuations, warnings and errors. debug:true adds scan stats, receipts and snapshots."
  },
  {
   "area": "Output",
   "title": "Leaner symbol outlines",
   "detail": "Members nest in their container and leaf symbols are one entry string. A/B: −35% tokens on 8 real outlines, 100% held-out replay accuracy (Sonnet and Haiku, 30 runs)."
  },
  {
   "area": "Output",
   "title": "Leaner text output",
   "detail": "Single-row YAML drops the results/index/data wrapper; path-only rows render as compact strings; localFetch numbers lines like rg -n."
  },
  {
   "area": "Output",
   "title": "Hints never clipped",
   "detail": "Every hint fits 120 characters. A complete single-hit answer carries one lead instead of up to two."
  },
  {
   "area": "Speed",
   "title": "Cached GitHub searches",
   "detail": "Repository, issue, pull-request and commit searches are cached for 60 seconds like code search; a repeat sends no request."
  },
  {
   "area": "Speed",
   "title": "Instant astSearch continuations",
   "detail": "Page 2 reuses page 1's scan while files are unchanged: 269 → 9 ms on a large tree."
  },
  {
   "area": "Speed",
   "title": "Warm lspSearch",
   "detail": "A private per-workspace language server stays up between CLI calls: TypeScript hover ~3 s → ~0.1 s."
  },
  {
   "area": "Research",
   "title": "PR review that fits",
   "detail": "Changed files come as compact rows grouped by directory; fileFilter narrows files and patches; matchString returns only matching hunks."
  },
  {
   "area": "Research",
   "title": "Importer pages for references",
   "detail": "TypeScript and JavaScript references and callers reach importers past the 24-file cap, one verified window at a time."
  },
  {
   "area": "Research",
   "title": "Declaring file ranks first",
   "detail": "Identifier searches rank the file that declares the symbol first."
  },
  {
   "area": "MCP",
   "title": "Small, stable instructions",
   "detail": "Server instructions stay within 2,000 characters; the grammar inventory moved to the CLI."
  },
  {
   "area": "CLI",
   "title": "schema replaces scheme",
   "detail": "octocode schema <tool> --view query prints the input contract. Output follows the terminal: text on a TTY, JSON on a pipe."
  },
  {
   "area": "CLI",
   "title": "One exit class per error code",
   "detail": "Scripts that read exit codes get one documented class per error code."
  },
  {
   "area": "Config",
   "title": "One name per setting",
   "detail": "Duplicate env aliases removed. GitHub token comes from GH_TOKEN, then GITHUB_TOKEN; lsp-servers.json replaces nine *_SERVER_PATH variables."
  }
 ]
}
```
