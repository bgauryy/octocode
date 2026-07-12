/**
 * `controls`/`fetch`/`materialize`/`view` lowering for `octocode search`.
 *
 * These are the non-predicate parts of an OQL query: search-result shaping
 * (counting, dedup, context, ranking), content/tree fetch instructions, the
 * materialize policy, and the discovery/paginated/detailed view mode.
 */
import type { OqlInputQuery, QueryControls, QueryView } from '../types.js';
import type { SearchShorthand } from './types.js';
import { isTreeSort, listFromComma } from './utils.js';

export function queryControls(
  parts: SearchShorthand
): QueryControls | undefined {
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

export function fetchInstructions(
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

export function materializePolicy(
  parts: SearchShorthand
): OqlInputQuery['materialize'] | undefined {
  if (!parts.materialize && !parts.forceRefresh) return undefined;
  const mode = parts.materialize ?? 'auto';
  if (!parts.forceRefresh) return mode;
  return { mode, forceRefresh: true };
}

export function resolveView(
  value: string | undefined
): { view?: QueryView } | { error: string } {
  if (!value) return {};
  if (value === 'discovery' || value === 'paginated' || value === 'detailed')
    return { view: value };
  return {
    error: `--view must be discovery, paginated, or detailed (got "${value}").`,
  };
}

// 'exact'/'compact' are deprecated pre-rename aliases for 'none'/'standard',
// accepted so an already-written --query JSON or external doc example
// doesn't silently start failing.
const CONTENT_VIEW_ALIASES: Record<string, 'none' | 'standard' | 'symbols'> = {
  none: 'none',
  exact: 'none',
  standard: 'standard',
  compact: 'standard',
  symbols: 'symbols',
};

function contentViewMode(
  value: string | undefined
): 'none' | 'standard' | 'symbols' | undefined {
  return value !== undefined ? CONTENT_VIEW_ALIASES[value] : undefined;
}

export function hasContentFetch(parts: SearchShorthand): boolean {
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
