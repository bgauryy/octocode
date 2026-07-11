/**
 * `target:"structure"` and `target:"content"` execution, plus the
 * scope-post-filter glob machinery shared by the files-lane callers
 * (localFindFiles has no include/exclude glob filter of its own, so results
 * are filtered and re-paginated here against the OQL scope globs).
 */
import path from 'node:path';
import type { LocalFindFilesToolResult } from '@octocodeai/octocode-core/extra-types';
import { viewStructure } from '../../../tools/local_view_structure/local_view_structure.js';
import { fetchContent } from '../../../tools/local_fetch_content/fetchContent.js';
import { mapContentResult, mapStructureResult } from '../resultMap.js';
import { findFilesNeedsScopePostFilter } from './scope.js';
import { resultDiagnostics, provenance } from './predicates.js';
import { normalizeGlobPath } from './types.js';
import type { OqlQuery, QueryScope, QuerySource } from '../../types.js';
import type {
  AdapterResult,
  LocalFetchToolQuery,
  LocalStructureToolQuery,
} from './types.js';

export async function executeStructure(
  query: OqlQuery,
  source: QuerySource,
  searchPath: string
): Promise<AdapterResult> {
  const toolQuery: Partial<LocalStructureToolQuery> = {
    path: searchPath,
    // details:true makes the tool emit per-entry rows (with depth/size) rather
    // than grouped files/folders lists, which map cleanly to tree rows.
    details: true,
    ...(query.fetch?.tree?.maxDepth !== undefined
      ? { maxDepth: query.fetch.tree.maxDepth, recursive: true }
      : {}),
    ...(query.fetch?.tree?.pattern
      ? { pattern: query.fetch.tree.pattern }
      : {}),
    ...(query.fetch?.tree?.includeSizes ? { includeSizes: true } : {}),
    ...(query.fetch?.tree?.extensions?.length
      ? { extensions: query.fetch.tree.extensions }
      : {}),
    ...(query.fetch?.tree?.filesOnly ? { filesOnly: true } : {}),
    ...(query.fetch?.tree?.directoriesOnly ? { directoriesOnly: true } : {}),
    ...(query.fetch?.tree?.sortBy ? { sortBy: query.fetch.tree.sortBy } : {}),
    ...(query.fetch?.tree?.reverse ? { reverse: true } : {}),
    ...(query.scope?.hidden !== undefined
      ? { hidden: query.scope.hidden }
      : {}),
    ...(query.limit ? { limit: query.limit } : {}),
    ...(query.itemsPerPage ? { itemsPerPage: query.itemsPerPage } : {}),
    ...(query.page ? { page: query.page } : {}),
  };
  const result = await viewStructure(toolQuery as LocalStructureToolQuery);
  const mapped = mapStructureResult(result, source);
  return {
    ...mapped,
    diagnostics: resultDiagnostics(result, 'localViewStructure'),
    provenance: [provenance('localViewStructure', source, undefined)],
  };
}

export async function executeContent(
  query: OqlQuery,
  source: QuerySource,
  searchPath: string
): Promise<AdapterResult> {
  const c = query.fetch?.content;
  // contentView and minify now share one vocabulary (none/standard/symbols),
  // so this is a direct passthrough rather than a translation.
  const minify = c?.contentView ?? 'standard';
  const range = normalizeContentRange(c?.range);
  const toolQuery: Partial<LocalFetchToolQuery> = {
    path: searchPath,
    minify,
    ...range,
    ...(c?.match?.text !== undefined ? { matchString: c.match.text } : {}),
    ...(c?.match?.regex ? { matchStringIsRegex: true } : {}),
    ...(c?.match?.caseSensitive ? { matchStringCaseSensitive: true } : {}),
    // Forward contextLines when startLine is absent (match-only fetch) so the
    // tool can expand context around the matched lines natively.
    ...(c?.range?.contextLines !== undefined &&
    c?.range?.startLine === undefined
      ? { contextLines: c.range.contextLines }
      : {}),
    ...(c?.charOffset !== undefined ? { charOffset: c.charOffset } : {}),
    ...(c?.charLength !== undefined ? { charLength: c.charLength } : {}),
    ...(c?.fullContent ? { fullContent: true } : {}),
  };
  const result = await fetchContent(toolQuery as LocalFetchToolQuery);
  const requestedView = minify;
  const mapped = mapContentResult(result, source, searchPath, requestedView);
  return {
    ...mapped,
    diagnostics: resultDiagnostics(result, 'localGetFileContent'),
    provenance: [provenance('localGetFileContent', source, undefined)],
  };
}

function normalizeContentRange(
  range: NonNullable<NonNullable<OqlQuery['fetch']>['content']>['range']
): { startLine?: number; endLine?: number } {
  if (range?.startLine === undefined) return {};
  const contextLines = range.contextLines ?? 0;
  const startLine = Math.max(1, range.startLine - contextLines);
  const endLine = (range.endLine ?? range.startLine) + contextLines;
  return { startLine, endLine };
}

/* ---------------------- findFiles scope post-filter ---------------------- */

export function filterFindFilesResultByScope(
  result: LocalFindFilesToolResult,
  query: OqlQuery,
  searchPath: string
): LocalFindFilesToolResult {
  if (!findFilesNeedsScopePostFilter(query.scope)) return result;
  if ((result as { status?: string }).status === 'error') return result;

  const filtered = (result.files ?? []).filter(entry =>
    pathMatchesScope(entry.path, searchPath, query.scope)
  );
  const page = Math.max(1, query.page ?? 1);
  const itemsPerPage = Math.max(
    1,
    (query.itemsPerPage ?? query.limit ?? filtered.length) || 1
  );
  const totalPages = Math.max(1, Math.ceil(filtered.length / itemsPerPage));
  const start = (page - 1) * itemsPerPage;
  const files = filtered.slice(start, start + itemsPerPage);
  const { status: _status, ...rest } = result as LocalFindFilesToolResult & {
    status?: string;
  };

  return {
    ...rest,
    ...(files.length === 0 ? { status: 'empty' as const } : {}),
    files,
    pagination: {
      currentPage: page,
      totalPages,
      filesPerPage: itemsPerPage,
      totalFiles: filtered.length,
      hasMore: page < totalPages,
      ...(page < totalPages ? { nextPage: page + 1 } : {}),
    },
  };
}

function pathMatchesScope(
  filePath: string,
  searchPath: string,
  scope: QueryScope | undefined
): boolean {
  const relativePath = normalizeGlobPath(
    relativeOrAbsolutePath(filePath, searchPath)
  );
  const includes = scope?.include ?? [];
  if (
    includes.length > 0 &&
    !includes.some(glob => matchesGlob(relativePath, glob))
  ) {
    return false;
  }
  return !(scope?.exclude ?? []).some(glob => matchesGlob(relativePath, glob));
}

function relativeOrAbsolutePath(filePath: string, searchPath: string): string {
  const rel = path.relative(searchPath, filePath);
  if (!rel || rel.startsWith('..') || path.isAbsolute(rel)) return filePath;
  return rel;
}

function matchesGlob(relativePath: string, glob: string): boolean {
  const normalizedGlob = normalizeGlobPath(glob);
  const target = normalizeGlobPath(relativePath);
  const re = globToRegExp(normalizedGlob);
  if (re.test(target)) return true;
  if (!normalizedGlob.includes('/')) {
    return re.test(path.posix.basename(target));
  }
  return false;
}

function globToRegExp(glob: string): RegExp {
  let pattern = '^';
  for (let i = 0; i < glob.length; i++) {
    const ch = glob[i]!;
    const next = glob[i + 1];
    const afterNext = glob[i + 2];
    if (ch === '*' && next === '*' && afterNext === '/') {
      pattern += '(?:.*/)?';
      i += 2;
      continue;
    }
    if (ch === '*' && next === '*') {
      pattern += '.*';
      i += 1;
      continue;
    }
    if (ch === '*') {
      pattern += '[^/]*';
      continue;
    }
    if (ch === '?') {
      pattern += '[^/]';
      continue;
    }
    pattern += escapeRegExp(ch);
  }
  return new RegExp(`${pattern}$`);
}

function escapeRegExp(ch: string): string {
  return /[|\\{}()[\]^$+?.]/.test(ch) ? `\\${ch}` : ch;
}
