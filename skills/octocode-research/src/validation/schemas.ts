/**
 * HTTP-compatible Zod schemas for octocode-research routes.
 *
 * These schemas wrap the authoritative schemas from octocode-mcp/public
 * with HTTP query string preprocessing (string → number/boolean/array).
 *
 * @module validation/schemas
 */

import { z } from 'zod';

// =============================================================================
// Import HTTP preprocessing utilities
// =============================================================================
import {
  toArray,
  safePath,
  numericString,
  requiredNumber,
  booleanString,
  stringArray,
  withResearchDefaults,
} from './httpPreprocess.js';

// =============================================================================
// Local Route Schemas
// =============================================================================

/**
 * HTTP schema for localSearchCode (ripgrep)
 * Wraps RipgrepQuerySchema with HTTP preprocessing
 */
export const localSearchSchema = z
  .object({
    // Required
    pattern: z.string().min(1, 'Pattern is required'),
    path: safePath,

    // Workflow mode preset
    mode: z.enum(['discovery', 'paginated', 'detailed']).optional(),

    // Pattern interpretation
    fixedString: booleanString,
    perlRegex: booleanString,

    // Case sensitivity
    smartCase: booleanString,
    caseInsensitive: booleanString,
    caseSensitive: booleanString,

    // Match behavior
    wholeWord: booleanString,
    invertMatch: booleanString,
    multiline: booleanString,
    multilineDotall: booleanString,
    lineRegexp: booleanString,

    // File filtering
    type: z.string().optional(),
    include: stringArray.optional(),
    exclude: stringArray.optional(),
    excludeDir: stringArray.optional(),
    binaryFiles: z.enum(['text', 'without-match', 'binary']).optional(),

    // Ignore control
    noIgnore: booleanString,
    hidden: booleanString,
    followSymlinks: booleanString,

    // Output control
    filesOnly: booleanString,
    filesWithoutMatch: booleanString,
    count: booleanString,
    countMatches: booleanString,
    lineNumbers: booleanString,
    column: booleanString,

    // Context control
    contextLines: numericString,
    beforeContext: numericString,
    afterContext: numericString,
    context: numericString, // deprecated alias
    matchContentLength: numericString,

    // Match limiting
    maxMatchesPerFile: numericString,
    maxFiles: numericString,
    maxResults: numericString, // deprecated alias

    // Pagination
    limit: numericString,
    filesPerPage: numericString,
    filePageNumber: numericString,
    matchesPerPage: numericString,

    // Stats & output format
    includeStats: booleanString,
    includeDistribution: booleanString,
    jsonOutput: booleanString,
    vimgrepFormat: booleanString,

    // Advanced options
    threads: numericString,
    mmap: booleanString,
    noUnicode: booleanString,
    encoding: z.string().optional(),
    sort: z.enum(['path', 'modified', 'accessed', 'created']).optional(),
    sortReverse: booleanString,
    noMessages: booleanString,
    passthru: booleanString,
    debug: booleanString,
    showFileLastModified: booleanString,

    // Research context (optional for HTTP)
    mainResearchGoal: z.string().optional(),
    researchGoal: z.string().optional(),
    reasoning: z.string().optional(),
  })
  .transform((data) => {
    const result = withResearchDefaults(data);
    if (result.contextLines === undefined && data.context !== undefined) {
      result.contextLines = data.context;
    }
    if (result.limit === undefined && data.maxResults !== undefined) {
      result.limit = data.maxResults;
    }
    return result;
  });

/**
 * HTTP schema for localGetFileContent
 * Wraps FetchContentQuerySchema with HTTP preprocessing
 */
export const localContentSchema = z
  .object({
    path: safePath,

    // Line-based pagination
    startLine: numericString,
    endLine: numericString,
    fullContent: booleanString,

    // Pattern matching within file
    matchString: z.string().optional(),
    matchStringContextLines: numericString,
    matchStringIsRegex: booleanString,
    matchStringCaseSensitive: booleanString,

    // Character-based pagination
    charOffset: numericString,
    charLength: numericString,

    // Research context
    mainResearchGoal: z.string().optional(),
    researchGoal: z.string().optional(),
    reasoning: z.string().optional(),
  })
  .transform(withResearchDefaults);

/**
 * Transform human-readable file type to MCP's Unix-style type codes
 */
const fileTypeTransform = (val: string | undefined) => {
  if (!val) return undefined;
  const typeMap: Record<string, string | undefined> = {
    file: 'f',
    directory: 'd',
    symlink: 'l',
    block: 'b',
    character: 'c',
    pipe: 'p',
    socket: 's',
    all: undefined,
    f: 'f',
    d: 'd',
    l: 'l',
    b: 'b',
    c: 'c',
    p: 'p',
    s: 's',
  };
  return typeMap[val] ?? val;
};

/**
 * HTTP schema for localFindFiles
 * Wraps FindFilesQuerySchema with HTTP preprocessing
 */
export const localFindSchema = z
  .object({
    path: safePath,
    pattern: z.string().optional(),
    name: z.string().optional(),
    names: stringArray.optional(),
    iname: z.string().optional(),
    pathPattern: z.string().optional(),
    regex: z.string().optional(),
    regexType: z.enum(['posix-egrep', 'posix-extended', 'posix-basic']).optional(),
    type: z
      .enum([
        'file', 'directory', 'symlink', 'block', 'character', 'pipe', 'socket', 'all',
        'f', 'd', 'l', 'b', 'c', 'p', 's',
      ])
      .optional()
      .transform(fileTypeTransform),
    empty: booleanString,
    executable: booleanString,
    readable: booleanString,
    writable: booleanString,
    permissions: z.string().optional(),
    maxDepth: numericString,
    minDepth: numericString,
    modifiedWithin: z.string().optional(),
    modifiedBefore: z.string().optional(),
    accessedWithin: z.string().optional(),
    sizeGreater: z.string().optional(),
    sizeLess: z.string().optional(),
    excludeDir: stringArray.optional(),
    limit: numericString,
    maxResults: numericString,
    filesPerPage: numericString,
    filePageNumber: numericString,
    charOffset: numericString,
    charLength: numericString,
    details: booleanString,
    showFileLastModified: booleanString,
    mainResearchGoal: z.string().optional(),
    researchGoal: z.string().optional(),
    reasoning: z.string().optional(),
  })
  .transform((data) => {
    const result = withResearchDefaults(data);
    if (result.name === undefined && data.pattern !== undefined) {
      result.name = data.pattern;
    }
    if (result.limit === undefined && data.maxResults !== undefined) {
      result.limit = data.maxResults;
    }
    return result;
  });

/**
 * HTTP schema for localViewStructure
 * Wraps ViewStructureQuerySchema with HTTP preprocessing
 */
export const localStructureSchema = z
  .object({
    path: safePath,
    pattern: z.string().optional(),
    directoriesOnly: booleanString,
    filesOnly: booleanString,
    extension: z.string().optional(),
    extensions: z.preprocess(
      (val) => (val === undefined || val === null) ? undefined : toArray(val),
      z.array(z.string()).optional()
    ),
    hidden: booleanString,
    showHidden: booleanString,
    depth: numericString,
    recursive: booleanString,
    details: booleanString,
    humanReadable: booleanString,
    summary: booleanString,
    showFileLastModified: booleanString,
    sortBy: z.enum(['name', 'size', 'time', 'extension']).optional(),
    reverse: booleanString,
    limit: numericString,
    entriesPerPage: numericString,
    entryPageNumber: numericString,
    charOffset: numericString,
    charLength: numericString,
    mainResearchGoal: z.string().optional(),
    researchGoal: z.string().optional(),
    reasoning: z.string().optional(),
  })
  .transform((data) => {
    const result = withResearchDefaults(data);
    if (result.hidden === undefined && data.showHidden !== undefined) {
      result.hidden = data.showHidden;
    }
    return result;
  });

// =============================================================================
// LSP Route Schemas
// =============================================================================

/**
 * HTTP schema for lspGotoDefinition
 * Wraps LSPGotoDefinitionQuerySchema with HTTP preprocessing
 */
export const lspDefinitionSchema = z
  .object({
    uri: safePath,
    symbolName: z.string().min(1, 'Symbol name is required'),
    lineHint: requiredNumber.refine((n) => n >= 1, 'Line hint must be at least 1'),
    orderHint: numericString.default(0),
    contextLines: numericString.default(5),
    mainResearchGoal: z.string().optional(),
    researchGoal: z.string().optional(),
    reasoning: z.string().optional(),
  })
  .transform(withResearchDefaults);

/**
 * HTTP schema for lspFindReferences
 * Wraps LSPFindReferencesQuerySchema with HTTP preprocessing
 */
export const lspReferencesSchema = z
  .object({
    uri: safePath,
    symbolName: z.string().min(1, 'Symbol name is required'),
    lineHint: requiredNumber.refine((n) => n >= 1, 'Line hint must be at least 1'),
    orderHint: numericString.default(0),
    includeDeclaration: booleanString.default(true),
    contextLines: numericString.default(2),
    referencesPerPage: numericString.default(20),
    page: numericString.default(1),
    mainResearchGoal: z.string().optional(),
    researchGoal: z.string().optional(),
    reasoning: z.string().optional(),
  })
  .transform(withResearchDefaults);

/**
 * HTTP schema for lspCallHierarchy
 * Wraps LSPCallHierarchyQuerySchema with HTTP preprocessing
 */
export const lspCallsSchema = z
  .object({
    uri: safePath,
    symbolName: z.string().min(1, 'Symbol name is required'),
    lineHint: requiredNumber.refine((n) => n >= 1, 'Line hint must be at least 1'),
    orderHint: numericString.default(0),
    direction: z.enum(['incoming', 'outgoing'], {
      error: "Direction must be 'incoming' or 'outgoing'",
    }),
    depth: numericString.default(1),
    contextLines: numericString.default(2),
    callsPerPage: numericString.default(15),
    page: numericString.default(1),
    mainResearchGoal: z.string().optional(),
    researchGoal: z.string().optional(),
    reasoning: z.string().optional(),
  })
  .transform(withResearchDefaults);

// =============================================================================
// GitHub Route Schemas
// =============================================================================

/**
 * HTTP schema for githubSearchCode
 * Wraps GitHubCodeSearchQuerySchema with HTTP preprocessing
 */
export const githubSearchSchema = z
  .object({
    keywordsToSearch: stringArray,
    owner: z.string().optional(),
    repo: z.string().optional(),
    path: z.string().optional(),
    extension: z.string().optional(),
    filename: z.string().optional(),
    match: z.enum(['file', 'path']).optional(),
    limit: numericString,
    page: numericString,
    mainResearchGoal: z.string().optional(),
    researchGoal: z.string().optional(),
    reasoning: z.string().optional(),
  })
  .transform(withResearchDefaults);

/**
 * HTTP schema for githubGetFileContent
 * Wraps FileContentQuerySchema with HTTP preprocessing
 */
export const githubContentSchema = z
  .object({
    owner: z.string().min(1, 'Owner is required'),
    repo: z.string().min(1, 'Repo is required'),
    path: z.string().min(1, 'Path is required'),
    branch: z.string().optional(),
    fullContent: booleanString,
    startLine: numericString,
    endLine: numericString,
    matchString: z.string().optional(),
    matchStringContextLines: numericString,
    charOffset: numericString,
    charLength: numericString,
    mainResearchGoal: z.string().optional(),
    researchGoal: z.string().optional(),
    reasoning: z.string().optional(),
  })
  .transform(withResearchDefaults);

/**
 * HTTP schema for githubSearchRepositories
 * Wraps GitHubReposSearchSingleQuerySchema with HTTP preprocessing
 */
export const githubReposSchema = z
  .object({
    keywordsToSearch: stringArray.optional(),
    topicsToSearch: stringArray.optional(),
    owner: z.string().optional(),
    stars: z.string().optional(),
    size: z.string().optional(),
    created: z.string().optional(),
    updated: z.string().optional(),
    match: z.preprocess(toArray, z.array(z.enum(['name', 'description', 'readme'])).optional()),
    sort: z.enum(['stars', 'forks', 'updated', 'best-match']).optional(),
    limit: numericString,
    page: numericString,
    mainResearchGoal: z.string().optional(),
    researchGoal: z.string().optional(),
    reasoning: z.string().optional(),
  })
  .refine(
    (data) =>
      (data.keywordsToSearch && data.keywordsToSearch.length > 0) ||
      (data.topicsToSearch && data.topicsToSearch.length > 0),
    {
      message: "At least one of 'keywordsToSearch' or 'topicsToSearch' is required",
      path: ['keywordsToSearch'],
    }
  )
  .transform(withResearchDefaults);

/**
 * HTTP schema for githubViewRepoStructure
 * Wraps GitHubViewRepoStructureQuerySchema with HTTP preprocessing
 */
export const githubStructureSchema = z
  .object({
    owner: z.string().min(1, 'Owner is required'),
    repo: z.string().min(1, 'Repo is required'),
    branch: z.string().min(1, 'Branch is required'),
    path: z.string().optional(),
    depth: numericString,
    entriesPerPage: numericString,
    entryPageNumber: numericString,
    mainResearchGoal: z.string().optional(),
    researchGoal: z.string().optional(),
    reasoning: z.string().optional(),
  })
  .transform(withResearchDefaults);

/**
 * HTTP schema for githubSearchPullRequests
 * Wraps GitHubPullRequestSearchQuerySchema with HTTP preprocessing
 */
export const githubPRsSchema = z
  .object({
    query: z.string().optional(),
    owner: z.string().optional(),
    repo: z.string().optional(),
    prNumber: numericString,
    match: z.preprocess(toArray, z.array(z.enum(['title', 'body', 'comments'])).optional()),
    author: z.string().optional(),
    assignee: z.string().optional(),
    commenter: z.string().optional(),
    involves: z.string().optional(),
    mentions: z.string().optional(),
    'review-requested': z.string().optional(),
    'reviewed-by': z.string().optional(),
    label: z.preprocess(toArray, z.union([z.string(), z.array(z.string())]).optional()),
    'no-label': booleanString,
    'no-milestone': booleanString,
    'no-project': booleanString,
    'no-assignee': booleanString,
    base: z.string().optional(),
    head: z.string().optional(),
    state: z.enum(['open', 'closed']).optional(),
    created: z.string().optional(),
    updated: z.string().optional(),
    closed: z.string().optional(),
    'merged-at': z.string().optional(),
    comments: z.union([numericString, z.string()]).optional(),
    reactions: z.union([numericString, z.string()]).optional(),
    interactions: z.union([numericString, z.string()]).optional(),
    merged: booleanString,
    draft: booleanString,
    withComments: booleanString,
    withCommits: booleanString,
    type: z.enum(['metadata', 'fullContent', 'partialContent']).optional(),
    sort: z.enum(['created', 'updated', 'best-match']).optional(),
    order: z.enum(['asc', 'desc']).optional(),
    limit: numericString,
    page: numericString,
    mainResearchGoal: z.string().optional(),
    researchGoal: z.string().optional(),
    reasoning: z.string().optional(),
  })
  .transform(withResearchDefaults);

// =============================================================================
// Package Route Schemas
// =============================================================================

/**
 * HTTP schema for packageSearch
 * Wraps PackageSearchQuerySchema with HTTP preprocessing
 */
export const packageSearchSchema = z
  .object({
    name: z.string().min(1, 'Package name is required'),
    ecosystem: z.enum(['npm', 'python']).optional().default('npm'),
    searchLimit: numericString,
    npmFetchMetadata: booleanString,
    pythonFetchMetadata: booleanString,
    mainResearchGoal: z.string().optional(),
    researchGoal: z.string().optional(),
    reasoning: z.string().optional(),
  })
  .transform(withResearchDefaults);

