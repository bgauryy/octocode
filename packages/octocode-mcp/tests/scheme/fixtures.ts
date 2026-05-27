/**
 * Shared test fixtures for schema and MCP protocol integration tests.
 *
 * Single source of truth for:
 *   - Mock field descriptions (all schema fields across all 14 tools)
 *   - Mock metadata builder (simulates octocodeai.com/api/mcpContent response)
 *   - Tool name mappings
 */

export const FIELD_DESCRIPTIONS: Record<string, string> = {
  mainResearchGoal: 'High-level research objective for context',
  researchGoal: 'Specific goal for this query',
  reasoning: 'Why this query is needed for the research',
  keywordsToSearch: 'Search keywords (1-5 strings)',
  owner: 'GitHub org or user (e.g. "facebook")',
  repo: 'Repository name (e.g. "react")',
  path: 'File or directory path',
  branch: 'Branch name',
  fullContent: 'When true, return entire file without truncation',
  startLine: 'Start line (1-indexed)',
  endLine: 'End line (1-indexed, inclusive)',
  matchString: 'Substring or regex to find within the file',
  matchStringContextLines: 'Lines of context around each match',
  charOffset: 'Character offset for output pagination',
  charLength: 'Character length for output pagination window',
  extension: 'File extension filter (e.g. "ts")',
  filename: 'Filename filter',
  match: 'Match scope (file, path, title, etc.)',
  limit: 'Maximum number of results to return',
  page: 'Page number for paginated results',
  depth: 'Tree traversal depth',
  entriesPerPage: 'Entries per page for pagination',
  entryPageNumber: 'Page number for entry pagination',
  query: 'Free-text search query string',
  state: 'PR state: open or closed',
  assignee: 'Filter by PR assignee username',
  author: 'Filter by PR author username',
  commenter: 'Filter by commenter username',
  involves: 'Filter by involved user',
  mentions: 'Filter by mentioned user',
  label: 'Filter by label name',
  sort: 'Sort field (stars, updated, etc.)',
  order: 'Sort direction: asc or desc',
  created: 'Filter by creation date range',
  updated: 'Filter by last-updated date range',
  closed: 'Filter by closed date',
  stars: 'Filter by star count range',
  size: 'Filter by repository size',
  topicsToSearch: 'GitHub topics to filter by',
  prNumber: 'Specific PR number to fetch',
  'review-requested': 'Filter by review-requested user',
  'reviewed-by': 'Filter by reviewer username',
  'no-label': 'Filter PRs with no labels',
  'no-milestone': 'Filter PRs with no milestone',
  'no-project': 'Filter PRs with no project',
  'no-assignee': 'Filter PRs with no assignee',
  head: 'Filter by head branch name',
  base: 'Filter by base branch name',
  'merged-at': 'Filter by merge date',
  comments: 'Filter by comment count',
  reactions: 'Filter by reaction count',
  interactions: 'Filter by interaction count',
  merged: 'Filter merged PRs',
  draft: 'Filter draft PRs',
  withComments: 'Include PR review comments in output',
  withCommits: 'Include PR commits in output',
  type: 'Output type selector',
  partialContentMetadata: 'Partial diff content specification',
  pattern: 'Search pattern (regex or fixed string)',
  mode: 'Workflow mode preset (discovery, paginated, detailed)',
  fixedString: 'Treat pattern as fixed string',
  perlRegex: 'Use PCRE2 regex engine',
  smartCase: 'Smart case: case-sensitive when pattern has uppercase',
  caseInsensitive: 'Force case-insensitive matching',
  caseSensitive: 'Force case-sensitive matching',
  wholeWord: 'Match whole words only',
  invertMatch: 'Invert match: show non-matching lines',
  include: 'Glob patterns to include',
  exclude: 'Glob patterns to exclude',
  excludeDir: 'Directory names to exclude',
  noIgnore: 'Ignore .gitignore rules',
  hidden: 'Include hidden files and directories',
  followSymlinks: 'Follow symbolic links',
  filesOnly: 'Return only file paths, not content',
  filesWithoutMatch: 'Return files that do NOT match',
  count: 'Return match count per file',
  countMatches: 'Return total match count',
  contextLines: 'Lines of context around each match',
  beforeContext: 'Lines of context before each match',
  afterContext: 'Lines of context after each match',
  matchContentLength: 'Maximum characters per match value',
  lineNumbers: 'Include line numbers in output',
  column: 'Include column numbers in output',
  maxMatchesPerFile: 'Maximum matches to return per file',
  maxFiles: 'Maximum files to return',
  filesPerPage: 'Files per page for pagination',
  filePageNumber: 'File-level page number',
  matchesPerPage: 'Matches per page for pagination',
  multiline: 'Enable multiline regex matching',
  multilineDotall: 'Dot matches newlines in multiline mode',
  binaryFiles: 'How to handle binary files',
  includeStats: 'Include search statistics in output',
  jsonOutput: 'Return results as JSON',
  vimgrepFormat: 'Return results in vimgrep format',
  includeDistribution: 'Include match distribution stats',
  threads: 'Number of search threads',
  mmap: 'Use memory-mapped I/O',
  noUnicode: 'Disable Unicode support',
  encoding: 'File encoding to use',
  sortReverse: 'Reverse sort order',
  noMessages: 'Suppress error messages',
  lineRegexp: 'Pattern must match entire line',
  passthru: 'Print all lines from matched files',
  debug: 'Enable debug output',
  showFileLastModified: 'Show file last-modified timestamp',
  name: 'Package name to search',
  searchLimit: 'Maximum number of search results',
  ecosystem: 'Package ecosystem: npm',
  npmFetchMetadata: 'Fetch extended npm metadata',
  uri: 'File URI for LSP operations',
  symbolName: 'Symbol name to look up',
  lineHint: 'Line number hint (1-indexed)',
  orderHint: 'Occurrence order on the hinted line (0-indexed)',
  direction: 'Call hierarchy direction: incoming or outgoing',
  callsPerPage: 'Call hierarchy entries per page',
  referencesPerPage: 'References per page',
  includeDeclaration: 'Include the declaration in references',
  includePattern: 'Glob patterns to include in results',
  excludePattern: 'Glob patterns to exclude from results',
  maxDepth: 'Maximum directory traversal depth',
  minDepth: 'Minimum directory depth',
  iname: 'Case-insensitive name pattern',
  names: 'Multiple name patterns to match',
  pathPattern: 'Path globbing pattern',
  regex: 'Regex pattern for file matching',
  regexType: 'Regex type (posix-egrep, posix-extended, posix-basic)',
  empty: 'Find empty files or directories',
  modifiedWithin: 'Find files modified within timeframe',
  modifiedBefore: 'Find files modified before date',
  accessedWithin: 'Find files accessed within timeframe',
  sizeGreater: 'Minimum file size filter',
  sizeLess: 'Maximum file size filter',
  permissions: 'File permission filter',
  executable: 'Filter executable files',
  readable: 'Filter readable files',
  writable: 'Filter writable files',
  sortBy: 'Sort results by field',
  details: 'Include detailed file information',
  summary: 'Include summary statistics',
  humanReadable: 'Human-readable file sizes',
  reverse: 'Reverse sort order',
  directoriesOnly: 'Return only directories',
  extensions: 'Multiple extension filters',
  recursive: 'Recurse into subdirectories',
  matchStringIsRegex: 'Treat matchString as regex',
  matchStringCaseSensitive: 'Case-sensitive matchString',
  sparse_path: 'Sparse checkout subdirectory path',
  forceRefresh: 'Bypass cache and force fresh fetch',
  parameterConflict: 'Parameter conflict validation message',
  lineRangeMismatch: 'Line range mismatch validation message',
};

export const TOOL_NAMES_MAP = {
  GITHUB_FETCH_CONTENT: 'githubGetFileContent',
  GITHUB_SEARCH_CODE: 'githubSearchCode',
  GITHUB_SEARCH_PULL_REQUESTS: 'githubSearchPullRequests',
  GITHUB_SEARCH_REPOSITORIES: 'githubSearchRepositories',
  GITHUB_VIEW_REPO_STRUCTURE: 'githubViewRepoStructure',
  PACKAGE_SEARCH: 'packageSearch',
  LOCAL_RIPGREP: 'localSearchCode',
  LOCAL_FETCH_CONTENT: 'localGetFileContent',
  LOCAL_FIND_FILES: 'localFindFiles',
  LOCAL_VIEW_STRUCTURE: 'localViewStructure',
  LSP_GOTO_DEFINITION: 'lspGotoDefinition',
  LSP_FIND_REFERENCES: 'lspFindReferences',
  LSP_CALL_HIERARCHY: 'lspCallHierarchy',
  GITHUB_CLONE_REPO: 'githubCloneRepo',
} as const;

export const ALL_TOOL_NAMES = [
  'githubGetFileContent',
  'githubSearchCode',
  'githubSearchPullRequests',
  'githubSearchRepositories',
  'githubViewRepoStructure',
  'packageSearch',
  'localSearchCode',
  'localGetFileContent',
  'localFindFiles',
  'localViewStructure',
  'lspGotoDefinition',
  'lspFindReferences',
  'lspCallHierarchy',
] as const;

/**
 * Builds a mock metadata object that mirrors the shape returned by
 * `https://octocodeai.com/api/mcpContent`.
 *
 * Every field description is non-empty so that tests can assert
 * Zod `.describe()` propagation without false positives.
 */
export function buildMockMetadata(overrides?: {
  instructions?: string;
  prompts?: Record<string, unknown>;
}) {
  return {
    instructions: overrides?.instructions ?? 'Test instructions',
    prompts: overrides?.prompts ?? {},
    toolNames: { ...TOOL_NAMES_MAP },
    baseSchema: {
      mainResearchGoal: FIELD_DESCRIPTIONS.mainResearchGoal,
      researchGoal: FIELD_DESCRIPTIONS.researchGoal,
      reasoning: FIELD_DESCRIPTIONS.reasoning,
      bulkQuery: (toolName: string) =>
        'Research queries for ' + toolName + ' (1-3 per call)',
    },
    tools: Object.fromEntries(
      ALL_TOOL_NAMES.map(name => [
        name,
        {
          name,
          description: `Full description for ${name}`,
          schema: { ...FIELD_DESCRIPTIONS },
          hints: { hasResults: ['hint'], empty: ['hint'] },
        },
      ])
    ),
    baseHints: { hasResults: ['base hint'], empty: ['base hint'] },
    genericErrorHints: ['error hint'],
    bulkOperations: {},
  };
}

/**
 * Recursively collects all `description` fields from a JSON Schema object.
 * Used to verify that Zod `.describe()` values are populated after metadata init.
 */
export interface FieldDesc {
  path: string;
  fieldName: string;
  description: string;
}

export function collectJsonSchemaDescriptions(
  obj: unknown,
  path = ''
): FieldDesc[] {
  const results: FieldDesc[] = [];
  if (!obj || typeof obj !== 'object') return results;
  const o = obj as Record<string, unknown>;

  if (typeof o.description === 'string') {
    const fieldName = path.split('.').pop() ?? path;
    results.push({ path, fieldName, description: o.description });
  }

  if (o.properties && typeof o.properties === 'object') {
    for (const [key, value] of Object.entries(
      o.properties as Record<string, unknown>
    )) {
      results.push(...collectJsonSchemaDescriptions(value, `${path}.${key}`));
    }
  }

  if (o.items && typeof o.items === 'object') {
    results.push(...collectJsonSchemaDescriptions(o.items, `${path}[]`));
  }

  for (const key of ['anyOf', 'oneOf', 'allOf'] as const) {
    if (Array.isArray(o[key])) {
      for (const item of o[key] as unknown[]) {
        results.push(...collectJsonSchemaDescriptions(item, path));
      }
    }
  }

  return results;
}
