/** Canonical agent-facing descriptions for the public direct tools. */
export const PUBLIC_TOOL_DESCRIPTIONS = {
  ghSearch:
    'Discover GitHub code with operation:"code", repositories with "repositories", or a known repository tree with "tree". Code search covers the default branch; tree accepts branch. Read known paths with ghGetFileContent. Empty or incomplete search results do not prove absence.',
  ghGetFileContent:
    'Read a known GitHub path. branch is honored when supplied; omission uses the default branch. A path-only call returns bounded, standard-minified content; optionally select fullContent, matchString, or startLine+endLine. minify:"none" preserves text; "symbols" returns an outline. Run returned continuations for partial content. matchedLines are exact source-line anchors. type:"directory" materializes localPath with persistent local access; use ghSearch operation:"tree" to browse remotely.',
  ghSearchHistory:
    'Discover history metadata with operation:"pullRequests", "issues", or "commits". Pull requests may be searched globally; issues and commits require owner+repo. Commit keywords search messages on the default branch; omit keywords to walk history by path/ref/date. Retrieve a known number or commit ref with ghGetHistoryItem.',
  ghGetHistoryItem:
    'Read known history with owner+repo: operation:"pullRequest" or "issue" takes number, "commit" takes ref, and "compare" takes base+head. PR/issue content selectors request bodies, comments, or review/change details; commit/compare includeDiff requests patches. Follow returned continuations for each partial surface.',
  npmSearch:
    'Look up npm package metadata with packageName, or discover npm packages with keywords. Uses npm configuration and registry-scoped credentials; exact scoped names honor scope mappings. Optional registry overrides routing. Keyword search targets one registry and supports page; follow returned continuations. Use returned repository links for source-code research.',
  ghCloneRepo:
    'Create a cached, shallow checkout for repeated reads, local AST/regex, or LSP. Optional sparsePath scopes a file/subtree; complete refers to that scope. branch selects a branch, tag, or full commit SHA; omission uses the default branch. location.commitSha identifies HEAD; cached working-tree contents are not reverified. forceRefresh renews the checkout. Requires git and persistent local storage. Pass results[].data.location.localPath to local tools; ghGetFileContent also supports individual remote reads.',
  localSearch:
    'Choose operation:"text" for lexical anchors, "structural" for AST matches, "files" for path or metadata discovery, and "tree" for directory orientation. Each operation has a strict branch, so do not mix fields across operations. Files/tree prune common generated and vendor directories by default; pass excludeDir:[] to prune nothing. Read exact hits with localGetFileContent and prove symbol identity with lspGetSemantics.',
  localAnalyzeGraph:
    'Map file topology, not symbol identity. dependencies/dependents need file; path needs file+target; deadCode/reachability accept roots; cycles needs no selector. Import edges and dead-code results are candidates—verify changes and deletions with exact reads plus LSP.',
  localGetFileContent:
    'Read a known local path after search, find, or structure discovery. A path-only call returns a bounded standard-minified view. For large files, get a symbols outline, then an exact range or match. Read small config files whole with minify:"none".\nOptionally choose fullContent, matchString, or startLine+endLine. Partial content cannot prove absence; run the returned continuation. matchedLines are LSP anchors; match ranges and character offsets are not. Report fetched bytes only.',
  lspGetSemantics:
    'Use a real search/read line anchor for semantic proof. references finds usages; callers incoming calls; callees outgoing calls; callHierarchy both. Fall back to references if call hierarchy is unavailable.\ndocumentSymbols/diagnostic need uri; workspaceSymbol needs symbolName; other operations need uri+symbolName+lineHint. orderHint resolves same-line names. format:"compact" saves tokens. Empty/unavailable results call for a new anchor or text search.',
} as const satisfies Record<string, string>;

export type PublicToolName = keyof typeof PUBLIC_TOOL_DESCRIPTIONS;
