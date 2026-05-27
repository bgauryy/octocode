import { describe, expect, it } from 'vitest';
import { z } from 'zod/v4';
import {
  FileContentBulkQueryLocalSchema,
  GitHubCodeSearchBulkQueryLocalSchema,
  GitHubPullRequestSearchBulkQueryLocalSchema,
  GitHubReposSearchBulkQueryLocalSchema,
  GitHubViewRepoStructureBulkQueryLocalSchema,
  PackageSearchBulkQueryLocalSchema,
} from '../../src/scheme/remoteSchemaOverlay.js';
import {
  BulkFetchContentQuerySchema,
  BulkFindFilesSchema,
  BulkRipgrepQuerySchema,
  BulkViewStructureSchema,
} from '../../src/scheme/localSchemaOverlay.js';
import {
  BulkLSPCallHierarchyQuerySchema,
  BulkLSPFindReferencesQuerySchema,
  BulkLSPGotoDefinitionQuerySchema,
} from '../../src/scheme/lspSchemaOverlay.js';
import { collectJsonSchemaDescriptions } from './fixtures.js';

const EXPECTED_TOOL_SCHEMA_DESCRIPTIONS: Record<
  string,
  Record<string, string>
> = {
  githubGetFileContent: {
    owner: 'Repo owner',
    repo: 'Repo name',
    branch: 'Branch/tag/SHA (defaults to repo default branch)',
    path: 'File path from root, no leading slash, exact case',
    startLine: 'Start line (with endLine)',
    endLine: 'End line (with startLine)',
    fullContent: 'Return entire file',
    matchString: 'Search pattern to extract',
    matchStringContextLines: 'Context lines around match',
    charOffset: 'Pagination offset',
    charLength: 'Max chars per page',
  },
  githubSearchCode: {
    keywordsToSearch:
      'Search terms (AND logic). match=file returns text_matches[]',
    owner: 'Repo owner (omit for cross-repo)',
    repo: 'Repo name (use with owner)',
    extension: 'Extension without dot (ts, js, py)',
    filename: 'Filename pattern (case-insensitive)',
    path: 'Directory path (strict prefix)',
    match: '"file" (content) | "path" (names). Omit for both',
    limit: 'Max results',
    page: 'Page number',
  },
  githubSearchPullRequests: {
    query: 'Search across title/body/comments (max 256 chars)',
    prNumber: 'Direct PR number (ignores other filters)',
    owner: 'Repo owner',
    repo: 'Repo name',
    state: '"open" | "closed" | "merged" (shorthand for closed + merged:true)',
    assignee: 'Assigned user',
    author: 'PR author',
    commenter: 'User who commented',
    involves: 'User involved',
    mentions: 'Mentions @user',
    'review-requested': 'Requested reviewer',
    'reviewed-by': 'Reviewer',
    label: 'Label filter',
    'no-label': 'No labels',
    'no-milestone': 'No milestone',
    'no-project': 'Not in project',
    'no-assignee': 'No assignee',
    head: 'Source branch',
    base: 'Target branch',
    created: 'Date: ">=YYYY-MM-DD" or "YYYY-MM-DD..YYYY-MM-DD"',
    updated: 'Same format as created',
    closed: 'Same format as created',
    'merged-at': 'Same format as created',
    comments: 'Count: ">5", "10..20"',
    reactions: 'Reaction count filter',
    interactions: 'Comments + reactions count',
    merged: 'Merged status (requires state=closed)',
    draft: 'Draft status',
    match: '["title"|"body"|"comments"]; default all three',
    sort: 'created | updated | best-match',
    order: 'desc | asc',
    limit: 'Max PRs',
    page: 'Page number',
    withComments: 'Include discussions (expensive)',
    withCommits: 'Include commit details',
    type: 'metadata | fullContent | partialContent',
  },
  githubSearchRepositories: {
    keywordsToSearch: 'Keywords (AND) across name/description/README',
    topicsToSearch: 'GitHub topic tags (self-reported, often sparse)',
    language:
      'Primary language ("TypeScript", "Python", "Go"). More reliable than topicsToSearch for language filtering',
    owner: 'Owner/org scope',
    stars: 'Stars: ">500", "100..500"',
    size: 'Repo size in KB with operators',
    created: 'Date: ">=YYYY-MM-DD" or "YYYY-MM-DD..YYYY-MM-DD"',
    updated: 'Last code push (pushed: qualifier, not metadata-only)',
    match: '["name"|"description"|"readme"] — restrictive',
    sort: 'stars | forks | updated | best-match',
    limit: 'Max repos',
    page: 'Page number',
  },
  githubViewRepoStructure: {
    owner: 'Repo owner',
    repo: 'Repo name',
    branch: 'Branch/tag/SHA (defaults to repo default branch)',
    path: 'Directory path (empty for root)',
    depth: '1 (current) | 2 (subdirs)',
    entriesPerPage: 'Entries per page',
    entryPageNumber: '1-based page',
  },
  packageSearch: {},
  localSearchCode: {
    pattern: 'Pattern/regex (required)',
    path: 'Root directory (required)',
    mode: '"discovery" (file list, cheapest) | "paginated" (default) | "detailed" (full context, costliest)',
    fixedString: 'Literal match, no regex',
    perlRegex: 'PCRE2 (lookahead, backrefs)',
    smartCase: 'Case-insensitive unless pattern has uppercase',
    caseInsensitive: 'Force case-insensitive',
    caseSensitive: 'Force case-sensitive',
    wholeWord: 'Whole words only',
    invertMatch: 'Return non-matching lines',
    type: 'Ripgrep language type ("ts", "js", "py", "go"...)',
    include: 'Include globs (["*.ts", "src/**"])',
    exclude: 'Exclude globs (["*.test.ts"])',
    excludeDir: 'Dir names to skip (["node_modules", "dist"])',
    noIgnore: 'Bypass .gitignore/.ignore',
    hidden: 'Include dotfiles',
    followSymlinks: 'Follow symlinks',
    filesOnly: 'Filenames only, no content',
    filesWithoutMatch: 'Files NOT containing the pattern',
    count: 'Matching-line count per file',
    countMatches: 'Total match count per file (multi-match aware)',
    contextLines: 'Symmetric context around match',
    beforeContext: 'Lines before (overrides contextLines on that side)',
    afterContext: 'Lines after (overrides contextLines on that side)',
    matchContentLength: 'Truncate each match line to N chars',
    maxMatchesPerFile: 'Cap matches per file',
    maxFiles: 'Cap total files scanned',
    filesPerPage: 'Files per page',
    filePageNumber: '1-indexed page',
    matchesPerPage: 'Matches per file in response',
    multiline: 'Patterns may span newlines (slower)',
    multilineDotall: "In multiline, '.' matches newlines",
    binaryFiles: '"skip" | "text" | "binary"',
    includeStats: 'Include scan stats in response',
    encoding: 'Force encoding ("utf-8", "latin1"); else auto',
    sort: 'path | modified | accessed | created',
    sortReverse: 'Reverse sort order',
    noMessages: 'Suppress non-fatal errors',
    lineRegexp: 'Pattern must match entire line',
    passthru: 'Print every line; highlight matches',
    debug: 'Emit debug diagnostics',
    showFileLastModified: 'Include lastModified timestamps',
  },
  localFindFiles: {
    path: 'Starting directory (required)',
    maxDepth: 'Max recursion depth',
    minDepth: 'Min depth from start',
    name: 'Glob name pattern (e.g. "*.js")',
    iname: 'Case-insensitive name glob',
    names: 'Glob array, OR-combined',
    pathPattern: 'Glob against full path, not basename',
    regex: 'Regex against name (or path with pathPattern semantics)',
    regexType: 'posix-egrep | posix-extended | posix-basic',
    type: 'f (file) | d (dir) | l (symlink) | b | c | p | s',
    empty: 'true = match only empty files/dirs',
    modifiedWithin: 'Within duration ("7d", "2h", "30m")',
    modifiedBefore: 'Before duration ("30d")',
    accessedWithin: 'Accessed within ("7d")',
    sizeGreater: '">" size ("10M", "500k", "1G")',
    sizeLess: '"<" size ("1M")',
    permissions: 'Octal ("755") or symbolic ("u=rwx")',
    executable: 'true = executable by current user',
    readable: 'true = readable by current user',
    writable: 'true = writable by current user',
    excludeDir: 'Dir names to skip (e.g. ["node_modules", ".git"])',
    limit: 'Hard cap before paging',
    details: 'Include perms/size/dates',
    filesPerPage: 'Results per page',
    filePageNumber: '1-indexed page',
    showFileLastModified: 'Include lastModified timestamps',
    charOffset: 'Char-level pagination offset',
    charLength: 'Max chars per payload page',
  },
  localGetFileContent: {
    path: 'File path (required)',
    fullContent: 'Return entire file',
    matchString: 'Search pattern with context',
    matchStringContextLines: 'Context lines around match',
    matchStringIsRegex: 'Treat matchString as regex',
    matchStringCaseSensitive: 'Case-sensitive',
    charOffset: 'Pagination offset',
    charLength: 'Max chars',
    startLine: 'Start line (1-indexed)',
    endLine: 'End line (1-indexed)',
  },
  localViewStructure: {
    path: 'Directory path (required)',
    details: 'Show perms/size/dates',
    hidden: 'Show hidden',
    humanReadable: 'Human sizes',
    sortBy: 'name | size | time | extension',
    reverse: 'Reverse sort',
    entriesPerPage: 'Entries per page',
    entryPageNumber: 'Page number',
    pattern: 'Name filter (glob/substring)',
    directoriesOnly: 'Dirs only',
    filesOnly: 'Files only',
    extension: 'Filter by extension',
    extensions: 'Multiple extensions',
    depth: 'Recursion depth',
    recursive: 'Recursive tree',
    limit: 'Max entries',
    charOffset: 'Pagination offset',
    charLength: 'Max chars',
    showFileLastModified: 'Show timestamps',
  },
  lspGotoDefinition: {
    uri: 'File path. Example: "src/utils.ts"',
    symbolName: 'EXACT symbol text, no parens, no partials',
    lineHint: '1-indexed line. Tool searches ±2 lines',
    orderHint: '0-indexed occurrence if multiple on line',
    contextLines: 'Context lines around match',
  },
  lspFindReferences: {
    uri: 'File path. Example: "src/api/client.ts"',
    symbolName: 'EXACT symbol text, no parens, no partials',
    lineHint: '1-indexed line. Tool searches ±2 lines',
    orderHint: '0-indexed occurrence if multiple on line',
    includeDeclaration: 'Include definition in results',
    contextLines: 'Context lines around match',
    referencesPerPage: 'Max refs per page',
    page: '1-indexed page',
    groupByFile:
      'Roll up references into per-file counts (cheaper, for impact analysis)',
    includePattern: 'Glob array — restrict search to these paths',
    excludePattern: 'Glob array — exclude these paths',
  },
  lspCallHierarchy: {
    uri: 'File path. Example: "src/api/handler.ts"',
    symbolName: 'EXACT function/method name, no parens',
    lineHint: '1-indexed line where function is defined or called',
    orderHint: '0-indexed occurrence if multiple on line',
    direction: '"incoming" (callers) | "outgoing" (callees)',
    depth: 'Recursion depth',
    contextLines: 'Context lines around match',
    callsPerPage: 'Max call sites per page',
    page: '1-indexed page',
  },
};

const TOOL_SCHEMAS: Record<string, z.ZodType> = {
  githubGetFileContent: FileContentBulkQueryLocalSchema,
  githubSearchCode: GitHubCodeSearchBulkQueryLocalSchema,
  githubSearchPullRequests: GitHubPullRequestSearchBulkQueryLocalSchema,
  githubSearchRepositories: GitHubReposSearchBulkQueryLocalSchema,
  githubViewRepoStructure: GitHubViewRepoStructureBulkQueryLocalSchema,
  packageSearch: PackageSearchBulkQueryLocalSchema,
  localSearchCode: BulkRipgrepQuerySchema,
  localFindFiles: BulkFindFilesSchema,
  localGetFileContent: BulkFetchContentQuerySchema,
  localViewStructure: BulkViewStructureSchema,
  lspGotoDefinition: BulkLSPGotoDefinitionQuerySchema,
  lspFindReferences: BulkLSPFindReferencesQuerySchema,
  lspCallHierarchy: BulkLSPCallHierarchyQuerySchema,
};

function topLevelFieldDescriptions(schema: z.ZodType): Record<string, string> {
  const jsonSchema = z.toJSONSchema(schema, { unrepresentable: 'any' });
  const descriptions = collectJsonSchemaDescriptions(jsonSchema);
  const result: Record<string, string> = {};

  for (const description of descriptions) {
    if (
      description.fieldName &&
      !(description.fieldName in result) &&
      description.fieldName !== 'queries'
    ) {
      result[description.fieldName] = description.description;
    }
  }

  return result;
}

describe('tool input schemas align with canonical resource metadata schema descriptions', () => {
  it.each(Object.keys(EXPECTED_TOOL_SCHEMA_DESCRIPTIONS))(
    '%s descriptions match the resource tool metadata',
    toolName => {
      const schema = TOOL_SCHEMAS[toolName];
      expect(
        schema,
        `${toolName} schema must be registered in test`
      ).toBeDefined();

      const actualDescriptions = topLevelFieldDescriptions(schema);
      const expectedDescriptions = EXPECTED_TOOL_SCHEMA_DESCRIPTIONS[toolName]!;

      for (const [fieldName, expectedDescription] of Object.entries(
        expectedDescriptions
      )) {
        expect(actualDescriptions[fieldName], `${toolName}.${fieldName}`).toBe(
          expectedDescription
        );
      }
    }
  );
});
