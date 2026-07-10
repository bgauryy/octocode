/**
 * Shorthand lowering for `octocode search`.
 *
 * The CLI reads argv and resolves a target string to a corpus (local path vs
 * GitHub ref — the only step that needs the filesystem). Everything else —
 * which predicate the flags select, dialect, lang requirements, and assembling
 * the sugar object the normalizer accepts — lives here so it is owned once in
 * tools-core (not re-implemented in the interface) and is unit-testable without
 * argv or a terminal.
 */
import {
  ACTIVE_TARGETS,
  CORPUS_OPTIONAL_TARGETS,
  type OqlActiveTarget,
  type OqlInputBatch,
  type OqlInputQuery,
  type Predicate,
  type QueryControls,
  type QueryScope,
  type QuerySource,
  type QueryView,
  type StructuralRuleInput,
} from './types.js';

/** Corpus already classified by the caller (local path vs GitHub ref). */
export type ShorthandCorpus =
  | { kind: 'local'; path: string }
  | { kind: 'github'; repo: string; path?: string; ref?: string }
  | { kind: 'npm' };

export interface SearchShorthand {
  target?: string;
  view?: string;
  contentView?: string;
  search?: string;
  /** literal text term (used when no pattern/rule/regex flag is set) */
  text?: string;
  /** regex pattern (rust dialect unless pcre2) */
  regex?: string;
  pcre2?: boolean;
  /** structural AST pattern (requires lang) */
  pattern?: string;
  /** structural rule, already parsed from JSON (requires lang) */
  rule?: unknown;
  lang?: string;
  corpus: ShorthandCorpus;
  materialize?: 'never' | 'auto' | 'required';
  branch?: string;
  forceRefresh?: boolean;
  limit?: number;
  page?: number;
  itemsPerPage?: number;
  include?: string[];
  exclude?: string[];
  excludeDir?: string[];
  extension?: string;
  filename?: string;
  pathPattern?: string;
  hidden?: boolean;
  noIgnore?: boolean;
  minDepth?: number;
  maxDepth?: number;
  empty?: boolean;
  modifiedWithin?: string;
  modifiedBefore?: string;
  accessedWithin?: string;
  sizeGreater?: string;
  sizeLess?: string;
  permissions?: string;
  executable?: boolean;
  readable?: boolean;
  writable?: boolean;
  details?: boolean;
  showModified?: boolean;
  caseInsensitive?: boolean;
  caseSensitive?: boolean;
  wholeWord?: boolean;
  fixedString?: boolean;
  multiline?: boolean;
  multilineDotall?: boolean;
  filesOnly?: boolean;
  directoriesOnly?: boolean;
  filesWithoutMatch?: boolean;
  entryType?: 'file' | 'directory';
  countLinesPerFile?: boolean;
  countMatchesPerFile?: boolean;
  onlyMatching?: boolean;
  unique?: boolean;
  countUnique?: boolean;
  contextLines?: number;
  invertMatch?: boolean;
  matchWindow?: number;
  matchContentLength?: number;
  maxMatchesPerFile?: number;
  matchPage?: number;
  maxFiles?: number;
  sort?: string;
  sortReverse?: boolean;
  rankingProfile?: string;
  debugRanking?: boolean;
  matchString?: string;
  matchRegex?: boolean;
  matchCaseSensitive?: boolean;
  startLine?: number;
  endLine?: number;
  charOffset?: number;
  charLength?: number;
  fullContent?: boolean;
  tree?: boolean;
  includeSizes?: boolean;
  op?: string;
  symbol?: string;
  symbolKind?: string;
  uri?: string;
  line?: number;
  order?: number;
  orderDirection?: string;
  depth?: number;
  workspaceRoot?: string;
  format?: string;
  owner?: string;
  topic?: string[];
  stars?: string;
  forks?: string;
  goodFirstIssues?: string;
  license?: string;
  created?: string;
  updated?: string;
  closed?: string;
  mergedAt?: string;
  size?: string;
  match?: string[];
  archived?: boolean;
  visibility?: string;
  concise?: boolean;
  state?: string;
  author?: string;
  label?: string;
  prNumber?: number;
  base?: string;
  head?: string;
  draft?: boolean;
  commentsContent?: boolean;
  commitsContent?: boolean;
  deep?: boolean;
  patchFile?: string;
  reviewMode?: string;
  filePage?: number;
  commentPage?: number;
  commitPage?: number;
  since?: string;
  until?: string;
  patches?: boolean;
  baseRef?: string;
  headRef?: string;
  diffPath?: string;
  artifactMode?: string;
  detailed?: boolean;
  verbose?: boolean;
  maxEntries?: number;
  minLength?: number;
  entryPageNumber?: number;
  scanOffset?: number;
  includeOffsets?: boolean;
  archiveFile?: string;
  intent?: string;
  facets?: string[];
  proof?: string;
  proofLimit?: number;
  includePackets?: boolean;
  includeFacts?: boolean;
  includeEdges?: boolean;
}

export type ShorthandResult =
  { input: OqlInputQuery | OqlInputBatch } | { error: string };

/**
 * Lower shorthand parts into the OQL sugar object. Predicate precedence:
 * pattern > rule > regex > text. Returns a typed error for invalid combos
 * (e.g. structural without `lang`) instead of throwing.
 */
export function buildShorthandInput(parts: SearchShorthand): ShorthandResult {
  const targetResult = resolveTarget(parts);
  if ('error' in targetResult) return targetResult;
  const target = targetResult.target;

  if (parts.search === 'both') {
    const pathQuery = buildSingleQuery({
      ...parts,
      target: 'files',
      search: 'path',
    });
    if ('error' in pathQuery) return pathQuery;
    const contentQuery = buildSingleQuery({
      ...parts,
      target: target === 'files' ? 'code' : target,
      search: 'content',
    });
    if ('error' in contentQuery) return contentQuery;
    return {
      input: {
        schema: 'oql',
        queries: [pathQuery.query, contentQuery.query],
        combine: 'independent',
        ...(parts.limit !== undefined ? { limit: parts.limit } : {}),
        ...(parts.page !== undefined ? { page: parts.page } : {}),
        ...(parts.itemsPerPage !== undefined
          ? { itemsPerPage: parts.itemsPerPage }
          : {}),
      },
    };
  }

  const built = buildSingleQuery({ ...parts, target });
  return 'error' in built ? built : { input: built.query };
}

function buildSingleQuery(
  parts: SearchShorthand & { target: OqlActiveTarget }
): { query: OqlInputQuery } | { error: string } {
  const view = resolveView(parts.view);
  if ('error' in view) return view;

  const sourceScope = sourceAndScope(parts);
  const targetUsesWhere = parts.target === 'code' || parts.target === 'files';
  const whereResult = targetUsesWhere ? buildPredicate(parts) : {};
  if ('error' in whereResult) return whereResult;

  const targetNeedsPredicate = ['code', 'files'].includes(parts.target);
  if (
    targetNeedsPredicate &&
    whereResult.where === undefined &&
    !parts.filesOnly &&
    !parts.filesWithoutMatch
  ) {
    return {
      error: 'No search term: provide text, --regex, --pattern, or --rule.',
    };
  }

  const query: OqlInputQuery = {
    schema: 'oql',
    target: parts.target,
    ...sourceScope,
  };

  if (whereResult.where) query.where = whereResult.where;
  if (view.view) query.view = view.view;
  if (parts.limit !== undefined) query.limit = parts.limit;
  if (parts.page !== undefined) query.page = parts.page;
  if (parts.itemsPerPage !== undefined) query.itemsPerPage = parts.itemsPerPage;

  const materialize = materializePolicy(parts);
  if (materialize) query.materialize = materialize;

  const fetch = fetchInstructions(parts);
  if (fetch) query.fetch = fetch;

  const controls = queryControls(parts);
  if (controls) query.controls = controls;

  const params = targetParams(parts);
  if (Object.keys(params).length > 0) query.params = params;

  return { query };
}

function resolveTarget(
  parts: SearchShorthand
): { target: OqlActiveTarget } | { error: string } {
  const explicit = parts.target;
  if (explicit) {
    if (isActiveTarget(explicit)) return { target: explicit };
    return {
      error: `--target must be one of: ${ACTIVE_TARGETS.join(', ')}.`,
    };
  }

  if (parts.op || parts.symbol || parts.uri || parts.workspaceRoot)
    return { target: 'semantics' };
  if (parts.tree) return { target: 'structure' };
  if (hasContentFetch(parts)) return { target: 'content' };
  if (parts.search === 'path') return { target: 'files' };
  if (parts.corpus.kind === 'npm') return { target: 'packages' };

  return { target: 'code' };
}

function isActiveTarget(value: string): value is OqlActiveTarget {
  return (ACTIVE_TARGETS as readonly string[]).includes(value);
}

function sourceAndScope(parts: SearchShorthand): {
  from?: QuerySource;
  scope?: QueryScope;
} {
  const scope: QueryScope = {};
  if (parts.lang) scope.language = parts.lang;
  if (parts.include?.length) scope.include = parts.include;
  if (parts.excludeDir?.length) scope.excludeDir = parts.excludeDir;
  if (parts.extension) {
    const ext = parts.extension.replace(/^\./, '');
    scope.include = [...(scope.include ?? []), `**/*.${ext}`];
  }
  if (parts.exclude?.length) scope.exclude = parts.exclude;
  if (parts.hidden) scope.hidden = true;
  if (parts.noIgnore) scope.noIgnore = true;
  if (parts.minDepth !== undefined) scope.minDepth = parts.minDepth;
  if (parts.maxDepth !== undefined) scope.maxDepth = parts.maxDepth;

  if (
    CORPUS_OPTIONAL_TARGETS.includes(parts.target as OqlActiveTarget) &&
    parts.corpus.kind === 'local'
  ) {
    return Object.keys(scope).length > 0 ? { scope } : {};
  }

  if (parts.corpus.kind === 'npm') {
    return {
      from: { kind: 'npm' },
      ...(Object.keys(scope).length > 0 ? { scope } : {}),
    };
  }

  if (parts.corpus.kind === 'github') {
    const from: QuerySource = {
      kind: 'github',
      repo: parts.corpus.repo,
      ...(parts.branch || parts.corpus.ref
        ? { ref: parts.branch ?? parts.corpus.ref }
        : {}),
    };
    if (parts.corpus.path) scope.path = parts.corpus.path;
    return {
      from,
      ...(Object.keys(scope).length > 0 ? { scope } : {}),
    };
  }

  return {
    from: { kind: 'local', path: parts.corpus.path },
    ...(Object.keys(scope).length > 0 ? { scope } : {}),
  };
}

function buildPredicate(
  parts: SearchShorthand
): { where?: Predicate } | { error: string } {
  const fieldPredicates = buildFieldPredicates(parts);

  if (parts.filesWithoutMatch) {
    const inner = predicateFromSearchTerm(parts);
    if ('error' in inner || !inner.where) return inner;
    const negated: Predicate = { kind: 'not', predicate: inner.where };
    return {
      where: fieldPredicates.length
        ? { kind: 'all', of: [...fieldPredicates, negated] }
        : negated,
    };
  }

  const base =
    parts.search === 'path'
      ? pathPredicate(parts)
      : predicateFromSearchTerm(parts);
  if ('error' in base) return base;
  if (fieldPredicates.length && base.where)
    return { where: { kind: 'all', of: [...fieldPredicates, base.where] } };
  if (fieldPredicates.length === 1) return { where: fieldPredicates[0] };
  if (fieldPredicates.length > 1)
    return { where: { kind: 'all', of: fieldPredicates } };
  return base;
}

function buildFieldPredicates(parts: SearchShorthand): Predicate[] {
  const predicates: Predicate[] = [];
  if (parts.filesOnly) {
    predicates.push({
      kind: 'field',
      field: 'entryType',
      op: '=',
      value: 'file',
    });
  } else if (parts.directoriesOnly || parts.entryType) {
    predicates.push({
      kind: 'field',
      field: 'entryType',
      op: '=',
      value: parts.directoriesOnly ? 'directory' : parts.entryType,
    });
  }
  if (parts.extension) {
    predicates.push({
      kind: 'field',
      field: 'extension',
      op: '=',
      value: parts.extension.replace(/^\./, ''),
    });
  }
  if (parts.filename) {
    predicates.push({
      kind: 'field',
      field: 'basename',
      op: 'glob',
      value: globValue(parts.filename),
    });
  }
  if (parts.pathPattern) {
    predicates.push({
      kind: 'field',
      field: 'path',
      op: 'glob',
      value: globValue(parts.pathPattern),
    });
  }
  if (parts.sizeGreater) {
    predicates.push({
      kind: 'field',
      field: 'size',
      op: '>',
      value: parts.sizeGreater,
    });
  }
  if (parts.sizeLess) {
    predicates.push({
      kind: 'field',
      field: 'size',
      op: '<',
      value: parts.sizeLess,
    });
  }
  if (parts.modifiedWithin) {
    predicates.push({
      kind: 'field',
      field: 'modified',
      op: 'within',
      value: parts.modifiedWithin,
    });
  }
  if (parts.modifiedBefore) {
    predicates.push({
      kind: 'field',
      field: 'modified',
      op: 'before',
      value: parts.modifiedBefore,
    });
  }
  if (parts.accessedWithin) {
    predicates.push({
      kind: 'field',
      field: 'accessed',
      op: 'within',
      value: parts.accessedWithin,
    });
  }
  if (parts.empty) {
    predicates.push({ kind: 'field', field: 'empty', op: '=', value: true });
  }
  if (parts.permissions) {
    predicates.push({
      kind: 'field',
      field: 'permissions',
      op: '=',
      value: parts.permissions,
    });
  }
  for (const [field, enabled] of [
    ['executable', parts.executable],
    ['readable', parts.readable],
    ['writable', parts.writable],
  ] as const) {
    if (enabled)
      predicates.push({ kind: 'field', field, op: '=', value: true });
  }
  return predicates;
}

function predicateFromSearchTerm(
  parts: SearchShorthand
): { where?: Predicate } | { error: string } {
  const structuralLang = parts.lang;
  if (parts.pattern !== undefined) {
    if (!structuralLang)
      return { error: '--pattern requires --lang (e.g. --lang ts).' };
    return {
      where: {
        kind: 'structural',
        lang: structuralLang,
        pattern: parts.pattern,
      },
    };
  }
  if (parts.rule !== undefined) {
    if (!structuralLang)
      return { error: '--rule requires --lang (e.g. --lang ts).' };
    return {
      where: {
        kind: 'structural',
        lang: structuralLang,
        // parts.rule is JSON-parsed upstream (SearchShorthand.rule: unknown);
        // it is the pre-parsed rule object/string the structural predicate
        // accepts. Downstream compile validates it before use.
        rule: parts.rule as StructuralRuleInput,
      },
    };
  }
  if (parts.regex !== undefined) {
    if (parts.fixedString) {
      // --fixed wins over --regex: treat the pattern as a literal text term
      return {
        where: {
          kind: 'text',
          value: parts.regex,
          ...caseControl(parts),
          ...(parts.wholeWord ? { wholeWord: true } : {}),
        },
      };
    }
    return {
      where: {
        kind: 'regex',
        value: parts.regex,
        ...(parts.pcre2 ? { dialect: 'pcre2' } : {}),
        ...caseControl(parts),
        ...(parts.wholeWord ? { wholeWord: true } : {}),
        ...(parts.multiline ? { multiline: true } : {}),
        ...(parts.multilineDotall ? { dotAll: true } : {}),
      },
    };
  }
  if (parts.text !== undefined && parts.text !== '') {
    return {
      where: {
        kind: 'text',
        value: parts.text,
        ...caseControl(parts),
        ...(parts.wholeWord ? { wholeWord: true } : {}),
      },
    };
  }
  return {};
}

function pathPredicate(
  parts: SearchShorthand
): { where?: Predicate } | { error: string } {
  if (parts.regex !== undefined) {
    return {
      where: {
        kind: 'field',
        field: 'path',
        op: 'regex',
        value: parts.regex,
      },
    };
  }
  if (parts.text !== undefined && parts.text !== '') {
    const hasPathShape = /[/*?[\]]/.test(parts.text);
    return {
      where: {
        kind: 'field',
        field: hasPathShape ? 'path' : 'basename',
        op: 'glob',
        value: hasPathShape ? parts.text : `*${parts.text}*`,
      },
    };
  }
  return {};
}

function globValue(value: string): string {
  return /[*?[\]]/.test(value) ? value : `*${value}*`;
}

function caseControl(parts: SearchShorthand): {
  case?: 'sensitive' | 'insensitive';
} {
  if (parts.caseSensitive) return { case: 'sensitive' };
  if (parts.caseInsensitive) return { case: 'insensitive' };
  return {};
}

function queryControls(parts: SearchShorthand): QueryControls | undefined {
  const search: NonNullable<QueryControls['search']> = {};
  if (parts.countLinesPerFile) search.countLinesPerFile = true;
  if (parts.countMatchesPerFile) search.countMatchesPerFile = true;
  if (parts.onlyMatching) search.onlyMatching = true;
  if (parts.unique) search.unique = true;
  if (parts.countUnique) search.countUnique = true;
  if (parts.contextLines !== undefined)
    search.contextLines = parts.contextLines;
  if (parts.invertMatch) search.invertMatch = true;
  if (parts.matchWindow !== undefined) search.matchWindow = parts.matchWindow;
  if (parts.matchContentLength !== undefined)
    search.matchContentLength = parts.matchContentLength;
  if (parts.maxMatchesPerFile !== undefined)
    search.maxMatchesPerFile = parts.maxMatchesPerFile;
  if (parts.matchPage !== undefined) search.matchPage = parts.matchPage;
  // Forward the sort as-is for search-sort targets: an unknown value fails
  // loudly at normalize (schema enum) and a valid-but-inapplicable value gets
  // a planner warning (sortApplicabilityDiagnostics) — never a silent drop.
  if (usesSearchSortControls(parts.target) && parts.sort) {
    search.sort = parts.sort as NonNullable<typeof search.sort>;
  }
  if (usesSearchSortControls(parts.target) && parts.sortReverse) {
    search.sortReverse = true;
  }
  if (parts.rankingProfile) search.rankingProfile = parts.rankingProfile;
  if (parts.debugRanking) search.debugRanking = true;

  const budget: NonNullable<QueryControls['budget']> = {};
  if (parts.maxFiles !== undefined) budget.maxFiles = parts.maxFiles;

  const controls: QueryControls = {};
  if (Object.keys(search).length > 0) controls.search = search;
  if (Object.keys(budget).length > 0) controls.budget = budget;
  return Object.keys(controls).length > 0 ? controls : undefined;
}

function usesSearchSortControls(target: string | undefined): boolean {
  return target === 'code' || target === 'files';
}

function fetchInstructions(
  parts: SearchShorthand
): OqlInputQuery['fetch'] | undefined {
  const content: NonNullable<OqlInputQuery['fetch']>['content'] = {};
  const contentView = contentViewMode(parts.contentView);
  if (contentView) content.contentView = contentView;
  if (
    parts.startLine !== undefined ||
    parts.endLine !== undefined ||
    parts.contextLines !== undefined
  ) {
    content.range = {
      ...(parts.startLine !== undefined ? { startLine: parts.startLine } : {}),
      ...(parts.endLine !== undefined ? { endLine: parts.endLine } : {}),
      ...(parts.contextLines !== undefined
        ? { contextLines: parts.contextLines }
        : {}),
    };
  }
  if (parts.matchString) {
    content.match = {
      text: parts.matchString,
      ...(parts.matchRegex ? { regex: true } : {}),
      ...(parts.matchCaseSensitive ? { caseSensitive: true } : {}),
    };
  }
  if (parts.charOffset !== undefined) content.charOffset = parts.charOffset;
  if (parts.charLength !== undefined) content.charLength = parts.charLength;
  if (parts.fullContent) content.fullContent = true;

  const tree: NonNullable<OqlInputQuery['fetch']>['tree'] = {};
  // `--tree --depth N` lowers --depth to parts.depth; map it to the tree's
  // maxDepth (parts.maxDepth comes from the file-discovery `--max-depth` flag).
  if (parts.maxDepth !== undefined) tree.maxDepth = parts.maxDepth;
  else if (parts.tree && parts.depth !== undefined) tree.maxDepth = parts.depth;
  if (parts.filename) {
    tree.pattern = parts.filename;
  } else if (parts.pattern && (parts.tree || parts.target === 'structure')) {
    // --pattern doubles as the AST shape for code search; only a tree/structure
    // query treats it as a name filter. Copying an AST pattern here would leak
    // a stray fetch.tree.pattern into code-target queries and continuations.
    tree.pattern = parts.pattern;
  }
  if (parts.includeSizes) tree.includeSizes = true;
  if (parts.extension) tree.extensions = listFromComma(parts.extension);
  if (parts.filesOnly) tree.filesOnly = true;
  if (parts.directoriesOnly || parts.entryType === 'directory')
    tree.directoriesOnly = true;
  if (isTreeSort(parts.sort)) tree.sortBy = parts.sort;
  if (parts.sortReverse) tree.reverse = true;

  const fetch: NonNullable<OqlInputQuery['fetch']> = {};
  if (Object.keys(content).length > 0) fetch.content = content;
  if (Object.keys(tree).length > 0) fetch.tree = tree;
  return Object.keys(fetch).length > 0 ? fetch : undefined;
}

function targetParams(parts: SearchShorthand): Record<string, unknown> {
  switch (parts.target) {
    case 'code':
      return clean({
        concise: parts.concise,
        extension: parts.extension,
        filename: parts.filename,
      });
    case 'semantics':
      return clean({
        type: parts.op ?? 'documentSymbols',
        uri: parts.uri,
        symbolName: parts.symbol,
        symbolKind: parts.symbolKind,
        lineHint: parts.line,
        orderHint: parts.order,
        depth: parts.depth,
        workspaceRoot: parts.workspaceRoot,
        format: parts.format,
      });
    case 'repositories':
      return clean({
        keywords: parts.text
          ? parts.text.includes(' ')
            ? parts.text.split(/\s+/).filter(Boolean)
            : [parts.text]
          : undefined,
        topicsToSearch: parts.topic?.length ? parts.topic : undefined,
        language: parts.lang,
        owner: parts.owner,
        stars: parts.stars,
        forks: parts.forks,
        goodFirstIssues: parts.goodFirstIssues,
        license: parts.license,
        created: parts.created,
        updated: parts.updated,
        size: parts.size,
        match: parts.match,
        archived: parts.archived,
        visibility: parts.visibility,
        concise: parts.concise,
        sort: parts.sort,
        limit: parts.limit,
        page: parts.page,
      });
    case 'packages':
      return clean({
        packageName: parts.text,
        page: parts.page,
      });
    case 'pullRequests':
      return clean({
        keywordsToSearch: parts.text ? [parts.text] : undefined,
        query: undefined,
        concise: parts.concise,
        state: parts.state,
        author: parts.author,
        label: parts.label,
        prNumber: parts.prNumber,
        head: parts.head,
        base: parts.base,
        created: parts.created,
        updated: parts.updated,
        closed: parts.closed,
        'merged-at': parts.mergedAt,
        draft: parts.draft,
        archived: parts.archived,
        sort: parts.sort,
        order: parts.orderDirection,
        reviewMode: parts.reviewMode,
        filePage: parts.filePage,
        commentPage: parts.commentPage,
        commitPage: parts.commitPage,
        charOffset: parts.charOffset,
        charLength: parts.charLength,
        content: prContent(parts),
        limit: parts.limit,
        page: parts.page,
        matchString: parts.matchString,
        matchScope: parts.matchString ? 'all' : undefined,
      });
    case 'commits':
      return clean({
        path: parts.corpus.kind === 'github' ? parts.corpus.path : undefined,
        branch: parts.branch,
        since: parts.since,
        until: parts.until,
        author: parts.author,
        includeDiff: parts.patches,
        limit: parts.limit,
        page: parts.page,
        filePage: parts.filePage,
        itemsPerPage: parts.itemsPerPage,
      });
    case 'artifacts':
      return clean({
        mode: parts.artifactMode ?? 'inspect',
        detailed: parts.detailed,
        verbose: parts.verbose,
        maxEntries: parts.maxEntries,
        entriesPerPage: parts.itemsPerPage,
        minLength: parts.minLength,
        entryPageNumber: parts.entryPageNumber,
        scanOffset: parts.scanOffset,
        charOffset: parts.charOffset,
        charLength: parts.charLength,
        matchString: parts.matchString,
        format: parts.format,
        includeOffsets: parts.includeOffsets,
        archiveFile: parts.archiveFile,
      });
    case 'diff': {
      const localTwoFileDiff =
        parts.corpus.kind === 'local' && parts.diffPath !== undefined;
      return clean({
        prNumber: parts.prNumber,
        baseRef: parts.baseRef ?? (localTwoFileDiff ? 'base' : undefined),
        headRef: parts.headRef ?? (localTwoFileDiff ? 'head' : undefined),
        path:
          parts.diffPath ??
          (parts.corpus.kind === 'github' ? parts.corpus.path : undefined),
      });
    }
    case 'research':
    case 'graph':
      return clean({
        goal: parts.text,
        intent: parts.intent,
        facets: parts.facets,
        proof: parts.proof,
        proofLimit: parts.proofLimit,
        includePackets: parts.includePackets,
        includeFacts: parts.includeFacts,
        includeEdges: parts.includeEdges,
        maxFiles: parts.maxFiles,
      });
    default:
      return {};
  }
}

function prContent(
  parts: SearchShorthand
): Record<string, unknown> | undefined {
  if (
    !parts.patches &&
    !parts.patchFile &&
    !parts.commentsContent &&
    !parts.commitsContent &&
    !parts.deep
  )
    return undefined;
  return clean({
    metadata: true,
    body: parts.deep ? true : undefined,
    changedFiles:
      parts.deep || parts.patches || parts.patchFile ? true : undefined,
    patches: parts.patchFile
      ? { mode: 'selected' as const, files: [parts.patchFile] }
      : parts.deep || parts.patches
        ? { mode: 'all' as const }
        : undefined,
    comments:
      parts.deep || parts.commentsContent
        ? { discussion: true, reviewInline: true }
        : undefined,
    reviews: parts.deep ? true : undefined,
    commits: parts.deep || parts.commitsContent ? { list: true } : undefined,
  });
}

function materializePolicy(
  parts: SearchShorthand
): OqlInputQuery['materialize'] | undefined {
  if (!parts.materialize && !parts.forceRefresh) return undefined;
  const mode = parts.materialize ?? 'auto';
  if (!parts.forceRefresh) return mode;
  return { mode, forceRefresh: true };
}

function resolveView(
  value: string | undefined
): { view?: QueryView } | { error: string } {
  if (!value) return {};
  if (value === 'discovery' || value === 'paginated' || value === 'detailed')
    return { view: value };
  return {
    error: `--view must be discovery, paginated, or detailed (got "${value}").`,
  };
}

function contentViewMode(
  value: string | undefined
): 'exact' | 'compact' | 'symbols' | undefined {
  switch (value) {
    case 'exact':
      return 'exact';
    case 'compact':
      return 'compact';
    case 'symbols':
      return 'symbols';
    default:
      return undefined;
  }
}

function hasContentFetch(parts: SearchShorthand): boolean {
  return Boolean(
    contentViewMode(parts.contentView) ||
    parts.matchString ||
    parts.startLine !== undefined ||
    parts.endLine !== undefined ||
    parts.charOffset !== undefined ||
    parts.charLength !== undefined ||
    parts.fullContent
  );
}

function clean(input: Record<string, unknown>): Record<string, unknown> {
  const out: Record<string, unknown> = {};
  for (const [key, value] of Object.entries(input)) {
    if (value !== undefined) out[key] = value;
  }
  return out;
}

function listFromComma(value: string): string[] {
  return value
    .split(',')
    .map(v => v.trim().replace(/^\./, ''))
    .filter(Boolean);
}

function isTreeSort(
  value: string | undefined
): value is 'name' | 'size' | 'time' | 'extension' {
  return (
    value === 'name' ||
    value === 'size' ||
    value === 'time' ||
    value === 'extension'
  );
}
