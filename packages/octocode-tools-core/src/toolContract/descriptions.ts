/** Canonical agent-facing descriptions for the public direct tools. */
export const PUBLIC_TOOL_DESCRIPTIONS = {
  ghSearch:
    'Choose operation:"code" for code contents or file paths, "repositories" to discover repositories, or "tree" to browse a known repository.\nEach operation has a strict input branch; fields from other operations are rejected. Use ghGetFileContent only after a path is known. Empty GitHub search results do not prove absence.',
  ghGetFileContent:
    'Read a known GitHub path after search or structure discovery; a complete search snippet needs no reread. A path-only call returns a bounded standard-minified view. For large files, get a symbols outline, then an exact range or match. Read small config files whole with minify:"none".\nOptionally choose fullContent, matchString, or startLine+endLine. Partial content cannot prove absence; run the returned continuation. matchedLines are LSP anchors; match ranges and offsets are not. type:"directory" returns localPath when local access is enabled; otherwise use ghSearch operation:"tree".',
  ghSearchHistory:
    'Search or list repository history. Choose operation:"pullRequests", "issues", or "commits"; fields from other operations are rejected. Pull-request search may be global, while issue and commit history require owner+repo. Search branches never accept detail identities, content selectors, or diff pagination. Follow a result with ghGetHistoryItem.',
  ghGetHistoryItem:
    'Retrieve exact GitHub history. Choose operation:"pullRequest" or "issue" with number, "commit" with ref, or "compare" with base+head. Every branch requires owner+repo and rejects fields from other branches. PR/issue content and commit/compare diffs keep their branch-specific page and character windows; copy returned continuations exactly.',
  npmSearch:
    'Resolve npm packages to source repositories. Set exactly one of packageName for exact lookup or keywords for discovery. Follow the repository into GitHub tools.',
  ghCloneRepo:
    'Best for repeated reads, local AST/regex, or LSP; use ghGetFileContent for one read. Discover a bounded sparsePath first; complete is relative to that requested scope. branch selects the ref and forceRefresh bypasses the cached clone. Pass results[].data.location.localPath to local tools.',
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
