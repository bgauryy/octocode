/**
 * Scope -> tool-query helpers: translate an OQL `QueryScope` (include/exclude
 * globs, hidden/noIgnore, depth bounds, language) and query-level controls
 * into the field shapes each local tool runner accepts.
 */
import { LOCAL_MAX_LIMIT } from '../../../config.js';
import {
  toLocalFileLanguageGlobs,
  toLocalSearchLanguageParams,
} from '../../transformers/language.js';
import { firstScopeLanguage } from '../../transformers/github/common.js';
import type { OqlQuery, QueryScope } from '../../types.js';
import { normalizeGlobPath } from './types.js';
import type { LocalFindToolQuery } from './findToolType.js';
import type { LocalSearchToolQuery } from './searchToolType.js';

export const LOCAL_SCOPE_FILTER_CANDIDATE_LIMIT = LOCAL_MAX_LIMIT;

export function scopeToCommon(
  scope: QueryScope | undefined
): Partial<LocalSearchToolQuery> {
  const out: Partial<LocalSearchToolQuery> = {};
  if (scope?.include) out.include = scope.include;
  if (scope?.exclude) out.exclude = scope.exclude;
  if (scope?.excludeDir) out.excludeDir = scope.excludeDir;
  if (scope?.hidden !== undefined) out.hidden = scope.hidden;
  if (scope?.noIgnore !== undefined) out.noIgnore = scope.noIgnore;
  return out;
}

// Signals that a literal (fixedString) search value was probably meant as a
// regex: alternation, groups, character classes, anchors, quantifiers, or a
// backslash class escape. Lone `.` is excluded — it is far too common in real
// literal searches (paths, method calls) to treat as a regex intent.
const REGEX_METACHAR_RE = /[|()[\]{}^$*+?]|\\[bBdDwWsS]/;

export function looksLikeRegex(value: string): boolean {
  return REGEX_METACHAR_RE.test(value);
}

export function mergeStringArrays(
  left: unknown,
  right: readonly string[]
): string[] {
  const existing = Array.isArray(left)
    ? left.filter((value): value is string => typeof value === 'string')
    : [];
  return [...new Set([...existing, ...right])];
}

export function applyLocalSearchLanguage(
  toolQuery: Partial<LocalSearchToolQuery>,
  scope: QueryScope | undefined,
  explicitLangType: string | undefined
): void {
  if (explicitLangType) {
    toolQuery.langType = explicitLangType;
    return;
  }

  const languageParams = toLocalSearchLanguageParams(firstScopeLanguage(scope));
  if (languageParams.langType) toolQuery.langType = languageParams.langType;
  if (languageParams.include?.length) {
    toolQuery.include = mergeStringArrays(
      toolQuery.include,
      languageParams.include
    );
  }
}

function languageNameGlobs(scope: QueryScope | undefined): string[] {
  return toLocalFileLanguageGlobs(firstScopeLanguage(scope));
}

function scopeIncludeAsName(glob: string): string | undefined {
  const normalized = normalizeGlobPath(glob);
  const recursiveExtension = normalized.match(/^\*\*\/(\*\.[^/]+)$/);
  if (recursiveExtension) return recursiveExtension[1];
  if (!normalized.includes('/')) return normalized;
  return undefined;
}

export function findFilesNeedsScopePostFilter(
  scope: QueryScope | undefined
): boolean {
  return Boolean(scope?.include?.length || scope?.exclude?.length);
}

export function applyFindFilesScope(
  toolQuery: Partial<LocalFindToolQuery>,
  scope: QueryScope | undefined
): void {
  if (scope?.excludeDir) toolQuery.excludeDir = scope.excludeDir;
  if (scope?.minDepth !== undefined) toolQuery.minDepth = scope.minDepth;
  if (scope?.maxDepth !== undefined) toolQuery.maxDepth = scope.maxDepth;

  const includeNames = (scope?.include ?? [])
    .map(scopeIncludeAsName)
    .filter((value): value is string => Boolean(value));
  if (includeNames.length > 0 && !toolQuery.names) {
    toolQuery.names = mergeStringArrays(toolQuery.names, includeNames);
  }

  const pathIncludes = (scope?.include ?? []).filter(
    glob => !scopeIncludeAsName(glob)
  );
  if (pathIncludes.length === 1 && !toolQuery.pathPattern) {
    toolQuery.pathPattern = pathIncludes[0];
  }

  const languageNames = languageNameGlobs(scope);
  if (languageNames.length > 0 && !toolQuery.names && !toolQuery.pathPattern) {
    toolQuery.names = languageNames;
  }
}

export function applyFindFilesPagination(
  toolQuery: Partial<LocalFindToolQuery>,
  query: OqlQuery
): void {
  if (findFilesNeedsScopePostFilter(query.scope)) {
    applyUnpagedFindFilesWindow(toolQuery);
    toolQuery.page = 1;
    return;
  }
  if (query.itemsPerPage) toolQuery.itemsPerPage = query.itemsPerPage;
  if (query.page) toolQuery.page = query.page;
}

export function applyUnpagedFindFilesWindow(
  toolQuery: Partial<LocalFindToolQuery>
): void {
  toolQuery.limit = LOCAL_SCOPE_FILTER_CANDIDATE_LIMIT;
  toolQuery.itemsPerPage = LOCAL_SCOPE_FILTER_CANDIDATE_LIMIT;
}

// localFindFiles orders by sortBy modified|name|path|size; other
// controls.search.sort values are code-search sorts with no files-lane
// equivalent and are left to the default ordering.
export function applyFindFilesSort(
  toolQuery: Partial<LocalFindToolQuery>,
  query: OqlQuery
): void {
  const sort = query.controls?.search?.sort;
  if (
    sort === 'size' ||
    sort === 'name' ||
    sort === 'path' ||
    sort === 'modified'
  ) {
    toolQuery.sortBy = sort;
  }
}

export function searchControls(query: OqlQuery): Partial<LocalSearchToolQuery> {
  const out: Partial<LocalSearchToolQuery> = {};
  const s = query.controls?.search;
  if (s) {
    if (s.onlyMatching) out.onlyMatching = true;
    if (s.unique) out.unique = true;
    if (s.countUnique) out.countUnique = true;
    if (s.countMatchesPerFile) out.countMatchesPerFile = true;
    if (s.countLinesPerFile) out.countLinesPerFile = true;
    if (s.contextLines !== undefined) out.contextLines = s.contextLines;
    if (s.invertMatch) out.invertMatch = true;
    if (s.matchWindow !== undefined) out.matchWindow = s.matchWindow;
    if (s.matchContentLength !== undefined)
      out.matchContentLength = s.matchContentLength;
    if (s.maxMatchesPerFile !== undefined)
      out.maxMatchesPerFile = s.maxMatchesPerFile;
    if (s.matchPage !== undefined) out.matchPage = s.matchPage;
    // 'size'/'name' are files-lane (localFindFiles sortBy) sorts with no
    // localSearchCode equivalent — applyFindFilesSort handles them and
    // sortApplicabilityDiagnostics warns; only forward code-lane sorts here.
    if (s.sort && s.sort !== 'size' && s.sort !== 'name') out.sort = s.sort;
    if (s.sortReverse) out.sortReverse = true;
    // rankingProfile is a loose string in OQL controls; the localSearchCode
    // schema validates it against its language-profile enum at runtime.
    if (s.rankingProfile)
      out.rankingProfile =
        s.rankingProfile as LocalSearchToolQuery['rankingProfile'];
    if (s.debugRanking) out.debugRanking = true;
  }
  // budget.maxFiles is the search --max-files file cap; apply it even when
  // controls.search is absent (the old early-return dropped it silently).
  if (query.controls?.budget?.maxFiles !== undefined)
    out.maxFiles = query.controls.budget.maxFiles;
  return out;
}
