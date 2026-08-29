/**
 * Conditional and mutually-exclusive input rules that a flattened field list
 * cannot represent. These are deliberately concise and executable-minded:
 * clients can show them beside either the compact or full schema.
 */
const RELATIONS: Readonly<Record<string, readonly string[]>> = {
  localSearchCode: [
    'mode:paginated | discovery | detailed -> requires searchText; pattern and rule are ignored.',
    'mode:structural -> requires exactly one of pattern | rule; searchText is ignored.',
    'unique | matchWindow -> requires output:matchOnly.',
  ],
  localAnalyzeGraph: [
    'operation:dependencies | dependents -> requires file.',
    'operation:path -> requires file + target.',
    'operation:deadCode | reachability -> entrypoints and includeTests select roots.',
    'depth applies only to dependencies | dependents.',
  ],
  localGetFileContent: [
    'Choose one extraction mode: fullContent | line range | matchString | minify:symbols.',
    'startLine and endLine form one line-range mode; endLine requires startLine.',
    'matchStringIsRegex and matchStringCaseSensitive apply only with matchString.',
  ],
  lspGetSemantics: [
    'type:workspaceSymbol -> requires symbolName; use workspaceRoot instead of uri.',
    'type:documentSymbols | diagnostic -> requires uri only.',
    'type:definition | references | hover | callers | callees | implementation | typeDefinition -> requires uri + symbolName; lineHint strongly recommended.',
  ],
  ghSearchPullRequests: [
    'detail mode -> requires owner + repo + prNumber; content and reviewMode apply only here.',
    'list mode -> omit prNumber and use keywordsToSearch and filters.',
  ],
  ghSearchIssues: [
    'detail mode -> requires owner + repo + issueNumber; content selectors apply only here.',
    'list mode -> omit issueNumber and use keywordsToSearch and filters.',
  ],
  ghSearchCommits: [
    'compare mode -> requires base + head together; omit both for history/search mode.',
  ],
  ghGetFileContent: [
    'Choose one extraction mode: fullContent | line range | matchString | minify:symbols.',
    'matchStringIsRegex and matchStringCaseSensitive apply only with matchString.',
    'type:directory materializes a directory; extraction fields apply to file reads.',
  ],
  ghSearchDiscussions: [
    'after is an opaque cursor returned by a previous page; omit it on the first request.',
  ],
};

export function getDirectToolSchemaRelations(toolName: string): string[] {
  return [...(RELATIONS[toolName] ?? [])];
}
