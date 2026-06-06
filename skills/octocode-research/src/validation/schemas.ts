import { z } from 'zod';

import {
  toArray,
  safePath,
  numericString,
  requiredNumber,
  booleanString,
  stringArray,
  withResearchDefaults,
} from './httpPreprocess.js';


export const localSearchSchema = z
  .object({
    pattern: z.string().min(1, 'Pattern is required'),
    path: safePath,

    mode: z.enum(['discovery', 'paginated', 'detailed']).optional(),

    fixedString: booleanString,
    perlRegex: booleanString,

    smartCase: booleanString,
    caseInsensitive: booleanString,
    caseSensitive: booleanString,

    wholeWord: booleanString,
    invertMatch: booleanString,
    multiline: booleanString,
    multilineDotall: booleanString,
    lineRegexp: booleanString,

    type: z.string().optional(),
    include: stringArray.optional(),
    exclude: stringArray.optional(),
    excludeDir: stringArray.optional(),
    binaryFiles: z.enum(['text', 'without-match', 'binary']).optional(),

    noIgnore: booleanString,
    hidden: booleanString,
    followSymlinks: booleanString,

    filesOnly: booleanString,
    filesWithoutMatch: booleanString,
    count: booleanString,
    countMatches: booleanString,
    lineNumbers: booleanString,
    column: booleanString,

    contextLines: numericString,
    beforeContext: numericString,
    afterContext: numericString,
    context: numericString,
    matchContentLength: numericString,

    maxMatchesPerFile: numericString,
    maxFiles: numericString,
    maxResults: numericString,

    limit: numericString,
    filesPerPage: numericString,
    filePageNumber: numericString,
    matchesPerPage: numericString,

    includeStats: booleanString,
    includeDistribution: booleanString,
    jsonOutput: booleanString,
    vimgrepFormat: booleanString,

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


export const localContentSchema = z
  .object({
    path: safePath,

    startLine: numericString,
    endLine: numericString,
    fullContent: booleanString,

    matchString: z.string().optional(),
    matchStringContextLines: numericString,
    matchStringIsRegex: booleanString,
    matchStringCaseSensitive: booleanString,

    charOffset: numericString,
    charLength: numericString,

    mainResearchGoal: z.string().optional(),
    researchGoal: z.string().optional(),
    reasoning: z.string().optional(),
  })
  .transform(withResearchDefaults);


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

