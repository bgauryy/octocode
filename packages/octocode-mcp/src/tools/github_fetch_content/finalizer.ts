import type { z } from 'zod/v4';
import type { FileContentQuerySchema } from '@octocodeai/octocode-core/schemas';

type FileContentQuery = z.infer<typeof FileContentQuerySchema>;
import type { BulkFinalizer } from '../../types/bulk.js';
import type {
  FlatQueryResult,
  PaginationInfo,
} from '../../types/toolResults.js';
import {
  applyBulkCharWindow,
  collectFlatErrors,
  dedupeHints,
  formatFinalizedResponse,
  type CharPagination,
  type QueryWithPagination,
} from '../../utils/response/groupedFinalizer.js';
import type {
  GitHubFetchContentOutputLocal,
  GroupedToolWarning,
} from '../../scheme/remoteSchemaOverlay.js';
import {
  isConcise,
  isCompact,
  compactTrimHints,
  makeAdvisoryPredicate,
} from '../../scheme/verbosity.js';
import { applyMinification } from '../../utils/minifier/applyMinification.js';
import { buildEvidenceMetadata } from '../evidence.js';
import type { WithVerbosity } from '../../scheme/localSchemaOverlay.js';
import type { WithOptionalMeta } from '../../types/execution.js';

/** Advisory hints githubGetFileContent emits; stripped under compact.
 * Substring-OR, case-insensitive. */
const isAdvisoryFetchContentHint = makeAdvisoryPredicate([
  'file_too_large',
  'too large',
]);

type PartialFileContentQuery = WithOptionalMeta<FileContentQuery> &
  QueryWithPagination;

type FileEntry = {
  path: string;
  content: string;
  totalLines?: number;
  resolvedBranch?: string;
  pagination?: PaginationInfo;
  isPartial?: boolean;
  startLine?: number;
  endLine?: number;
  lastModified?: string;
  lastModifiedBy?: string;
  warnings?: string[];
};

type DirectoryEntry = {
  path: string;
  localPath: string;
  fileCount: number;
  totalSize: number;
  files?: Array<{ path: string; size: number; type: string }>;
  cached?: boolean;
  resolvedBranch?: string;
};

type RepoGroup = {
  id: string;
  owner: string;
  repo: string;
  files?: FileEntry[];
  directories?: DirectoryEntry[];
};

// Structurally identical to `GitHubFetchContentOutputLocal` now that the
// schema's pagination fields are tightened (file pagination →
// PaginationInfoSchema; responsePagination → CharPaginationSchema).  Kept as
// a local alias for readability; no cast needed at formatFinalizedResponse.
type FileContentResponse = GitHubFetchContentOutputLocal;

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null;
}

function readString(value: unknown): string | undefined {
  return typeof value === 'string' && value.length > 0 ? value : undefined;
}

function readNumber(value: unknown): number | undefined {
  return typeof value === 'number' && Number.isFinite(value)
    ? value
    : undefined;
}

function readStringArray(value: unknown): string[] | undefined {
  if (!Array.isArray(value)) return undefined;
  const strings = value.filter(
    (item): item is string => typeof item === 'string'
  );
  return strings.length > 0 ? strings : undefined;
}

function collectPeerHints(results: readonly FlatQueryResult[]): string[] {
  return dedupeHints(
    results.flatMap(result => {
      const raw = result.data.hints;
      return Array.isArray(raw)
        ? raw.filter((hint): hint is string => typeof hint === 'string')
        : [];
    })
  );
}

function buildFetchEvidence(
  groups: readonly RepoGroup[],
  responsePagination: CharPagination | undefined,
  errors: ReadonlyArray<NonNullable<FileContentResponse['errors']>[number]>
): NonNullable<GitHubFetchContentOutputLocal['evidence']> {
  const fileCount = groups.reduce(
    (sum, group) => sum + (group.files?.length ?? 0),
    0
  );
  const directoryCount = groups.reduce(
    (sum, group) => sum + (group.directories?.length ?? 0),
    0
  );
  const partialFiles = groups.reduce(
    (sum, group) =>
      sum + (group.files ?? []).filter(file => file.isPartial).length,
    0
  );
  const paginatedFiles = groups.reduce(
    (sum, group) =>
      sum + (group.files ?? []).filter(file => file.pagination?.hasMore).length,
    0
  );
  const reasons: string[] = [];

  if (partialFiles > 0) {
    reasons.push(`${partialFiles} file slice(s) are partial.`);
  }
  if (paginatedFiles > 0) {
    reasons.push(`${paginatedFiles} file content page(s) have more data.`);
  }
  for (const group of groups) {
    for (const file of group.files ?? []) {
      if (
        file.pagination?.hasMore &&
        typeof file.pagination.charOffset === 'number'
      ) {
        const nextOffset =
          file.pagination.charOffset + (file.pagination.charLength ?? 0);
        reasons.push(
          `Use charOffset=${nextOffset} for ${group.id}:${file.path}.`
        );
      }
      if (
        file.isPartial &&
        typeof file.endLine === 'number' &&
        typeof file.totalLines === 'number' &&
        file.endLine < file.totalLines
      ) {
        reasons.push(
          `Use startLine=${file.endLine + 1} with an endLine up to ${file.totalLines} for ${group.id}:${file.path}.`
        );
      }
    }
  }
  if (responsePagination?.hasMore) {
    reasons.push('Bulk response pagination has more data.');
  }
  if (errors.length > 0) {
    reasons.push(`${errors.length} query result(s) failed.`);
  }

  const hasContent = fileCount + directoryCount > 0;
  return buildEvidenceMetadata({
    kind: 'content',
    answerReady: hasContent,
    incompleteReasons: reasons,
    emptyReason: 'No file or directory content was returned.',
  });
}

const OPTIONAL_PAGINATION_NUMERIC_FIELDS = [
  'byteOffset',
  'byteLength',
  'totalBytes',
  'charOffset',
  'charLength',
  'totalChars',
  'filesPerPage',
  'totalFiles',
  'entriesPerPage',
  'totalEntries',
  'matchesPerPage',
  'totalMatches',
] as const satisfies ReadonlyArray<keyof PaginationInfo>;

/**
 * Narrow an opaque per-query data slot into a `PaginationInfo`.
 *
 * The bulk runner types `FlatQueryResult.data` as `Record<string, unknown>`
 * so each tool is free to define its own per-query payload.  We validate
 * the three required fields, then copy the optional numeric counters
 * field-by-field so the returned value is structurally typed without an
 * `as unknown as` boundary cast.
 */
function readPagination(value: unknown): PaginationInfo | undefined {
  if (!isRecord(value)) return undefined;
  const { currentPage, totalPages, hasMore } = value;
  if (
    typeof currentPage !== 'number' ||
    typeof totalPages !== 'number' ||
    typeof hasMore !== 'boolean'
  ) {
    return undefined;
  }
  const result: PaginationInfo = { currentPage, totalPages, hasMore };
  for (const field of OPTIONAL_PAGINATION_NUMERIC_FIELDS) {
    const candidate = value[field];
    if (typeof candidate === 'number' && Number.isFinite(candidate)) {
      result[field] = candidate;
    }
  }
  return result;
}

function groupId(owner: string, repo: string): string {
  return `${owner}/${repo}`;
}

function ensureGroup(
  groups: Map<string, RepoGroup>,
  owner: string,
  repo: string
): RepoGroup {
  const id = groupId(owner, repo);
  const existing = groups.get(id);
  if (existing) return existing;
  const created: RepoGroup = { id, owner, repo };
  groups.set(id, created);
  return created;
}

function readFileEntry(
  data: Record<string, unknown>,
  query: PartialFileContentQuery
): FileEntry {
  return {
    path: readString(data.path) ?? String(query.path ?? ''),
    content: typeof data.content === 'string' ? data.content : '',
    totalLines: readNumber(data.totalLines),
    resolvedBranch: readString(data.resolvedBranch),
    pagination: readPagination(data.pagination),
    ...(data.isPartial === true ? { isPartial: true } : {}),
    startLine: readNumber(data.startLine),
    endLine: readNumber(data.endLine),
    lastModified: readString(data.lastModified),
    lastModifiedBy: readString(data.lastModifiedBy),
    warnings: readStringArray(data.warnings),
  };
}

function readDirectoryEntry(
  data: Record<string, unknown>,
  query: PartialFileContentQuery
): DirectoryEntry {
  const rawFiles = Array.isArray(data.files) ? data.files : [];
  const files = rawFiles.filter(isRecord).map(file => ({
    path: readString(file.path) ?? '',
    size: readNumber(file.size) ?? 0,
    type: readString(file.type) ?? 'file',
  }));

  return {
    path: String(query.path ?? ''),
    localPath: readString(data.localPath) ?? '',
    fileCount: readNumber(data.fileCount) ?? files.length,
    totalSize: readNumber(data.totalSize) ?? 0,
    ...(files.length > 0 ? { files } : {}),
    ...(data.cached === true ? { cached: true } : {}),
    resolvedBranch: readString(data.resolvedBranch),
  };
}

function buildGroups(
  results: readonly FlatQueryResult[],
  queries: readonly PartialFileContentQuery[]
): RepoGroup[] {
  const groups = new Map<string, RepoGroup>();

  results.forEach((result, index) => {
    if (result.status === 'error') return;
    const query = queries[index];
    if (!query) return;
    const owner = String(query.owner ?? '');
    const repo = String(query.repo ?? '');
    if (!owner || !repo) return;

    const group = ensureGroup(groups, owner, repo);
    const data = result.data;

    if (query.type === 'directory') {
      const directories = group.directories ?? [];
      directories.push(readDirectoryEntry(data, query));
      group.directories = directories;
      return;
    }

    const files = group.files ?? [];
    files.push(readFileEntry(data, query));
    group.files = files;
  });

  return Array.from(groups.values());
}

function getGroupItems(
  group: RepoGroup
): readonly (FileEntry | DirectoryEntry)[] {
  return [...(group.files ?? []), ...(group.directories ?? [])];
}

function setGroupItems(
  group: RepoGroup,
  items: Array<FileEntry | DirectoryEntry>
): RepoGroup {
  const files = items.filter((item): item is FileEntry => 'content' in item);
  const directories = items.filter(
    (item): item is DirectoryEntry => 'localPath' in item
  );
  return {
    ...group,
    ...(files.length > 0 ? { files } : { files: undefined }),
    ...(directories.length > 0 ? { directories } : { directories: undefined }),
  };
}

/**
 * The single paginatable text field on a fetch-content item. Directory entries
 * have no `content`, so they are atomic (getter returns undefined).
 */
const getFileContent = (
  item: FileEntry | DirectoryEntry
): string | undefined => ('content' in item ? item.content : undefined);
const setFileContent = (
  item: FileEntry | DirectoryEntry,
  content: string
): FileEntry | DirectoryEntry =>
  'content' in item ? { ...item, content } : item;

function buildRuntimeHints(
  groups: readonly RepoGroup[],
  responsePagination?: CharPagination
): string[] {
  const hints: string[] = [];

  for (const group of groups) {
    for (const file of group.files ?? []) {
      if (
        file.pagination?.hasMore &&
        typeof file.pagination.charOffset === 'number'
      ) {
        const currentLength = file.pagination.charLength ?? 0;
        hints.push(
          `Use charOffset=${file.pagination.charOffset + currentLength} for ${group.id}:${file.path} to continue this file.`
        );
      }
      // Partial line-range continuation is intentionally NOT hinted at the
      // top level — the agent already has isPartial/endLine/totalLines on
      // each file entry, so duplicating the math here is pure redundancy.
    }

    for (const directory of group.directories ?? []) {
      if (directory.cached)
        hints.push(
          `Directory ${group.id}:${directory.path} served from cache.`
        );
    }
  }

  if (responsePagination?.hasMore) {
    hints.push(
      `Use responseCharOffset=${responsePagination.charOffset + responsePagination.charLength} to continue this paginated bulk response.`
    );
  }

  return dedupeHints(hints);
}

function errorHints(error: string, status?: number): string[] | undefined {
  const lower = error.toLowerCase();
  if (status === 404 || lower.includes('not found') || lower.includes('404')) {
    return [
      'Verify owner/repo/path/branch.',
      'Use githubViewRepoStructure to confirm the path.',
    ];
  }
  if (status === 403 || lower.includes('forbidden') || lower.includes('403')) {
    return ['Check token permissions or repository visibility.'];
  }
  if (status === 429 || lower.includes('rate limit')) {
    return ['Retry after reset or authenticate with a higher-limit token.'];
  }
  return undefined;
}

function collectFileErrors(
  results: readonly FlatQueryResult[],
  queries: readonly PartialFileContentQuery[]
): FileContentResponse['errors'] {
  const base = collectFlatErrors(results);
  return base.map(error => {
    const index = results.findIndex(result => result.id === error.id);
    const query = index >= 0 ? queries[index] : undefined;
    return {
      id: error.id,
      owner: query?.owner,
      repo: query?.repo,
      path: query?.path ? String(query.path) : undefined,
      error: error.error,
      hints: errorHints(error.error),
    };
  });
}

export function buildGithubFetchContentFinalizer<
  TQuery extends PartialFileContentQuery,
>(): BulkFinalizer<TQuery, GitHubFetchContentOutputLocal> {
  return ({ queries, results, config }) => {
    let groups = buildGroups(results, queries);

    // Bulk char-pagination via the shared "explicit-or-overflow" policy. Pure
    // pagination — an oversized file's `content` is windowed by char offset
    // (not truncated): the next page is reached by advancing responseCharOffset
    // (or charOffset on the path).
    const bulk = applyBulkCharWindow(groups, config, {
      getItems: getGroupItems,
      setItems: setGroupItems,
      getItemText: getFileContent,
      setItemText: setFileContent,
    });
    groups = bulk.groups;
    const responsePagination = bulk.responsePagination;

    const errors = collectFileErrors(results, queries);
    const hints = dedupeHints([
      ...(config.peerHints ? collectPeerHints(results) : []),
      ...buildRuntimeHints(groups, responsePagination),
    ]);
    const responseData: FileContentResponse = { results: groups };

    if (responsePagination)
      responseData.responsePagination = responsePagination;
    if (hints.length > 0) responseData.hints = hints;
    if (errors && errors.length > 0) responseData.errors = errors;
    if (config.peerEvidence) {
      responseData.evidence = buildFetchEvidence(
        groups,
        responsePagination,
        errors ?? []
      );
    }

    // ── Verbosity shaping ───────────────────────────────────────────────
    applyGithubFetchContentVerbosity(responseData, queries);

    return formatFinalizedResponse<GitHubFetchContentOutputLocal>(
      responseData,
      [
        'results',
        'id',
        'owner',
        'repo',
        'files',
        'directories',
        'path',
        'content',
        'totalLines',
        'startLine',
        'endLine',
        'isPartial',
        'pagination',
        'responsePagination',
        'hints',
        'errors',
      ],
      groups.length === 0 && Boolean(errors && errors.length > 0)
    );
  };
}

/**
 * Per-tool verbosity shaping for githubGetFileContent. Under concise (when every
 * query asks for it), strips `content` from every file and emits a token
 * estimate + drill-back hint. Under compact (any query opts in), trims
 * advisory hints. Basic / omitted: passthrough.
 *
 * Mutates `responseData` in place; returns `true` when concise was applied.
 */
export function applyGithubFetchContentVerbosity(
  responseData: GitHubFetchContentOutputLocal,
  queries: readonly PartialFileContentQuery[]
): boolean {
  const queriesWithVerbosity = queries as Array<
    WithVerbosity<PartialFileContentQuery> & { fullContent?: boolean }
  >;
  const allConcise =
    queriesWithVerbosity.length > 0 &&
    queriesWithVerbosity.every(q => isConcise(q.verbosity));
  const anyCompact = queriesWithVerbosity.some(q => isCompact(q.verbosity));

  if (allConcise) {
    let totalLines = 0;
    let rawLen = 0;
    let minLen = 0;
    // Concise MINIFIES each file body (strip comments/whitespace per file type)
    // rather than blanking it — a minified body is a cheap, still-useful read,
    // not a dead-end. Heavy metadata is dropped; a raw→minified token summary
    // is emitted. Never larger than verbatim (applyMinification guards that).
    const shapedGroups = (responseData.results ?? []).map(g => ({
      ...g,
      files: (g.files ?? []).map(f => {
        totalLines += f.totalLines ?? 0;
        const raw = f.content ?? '';
        const min = f.path ? applyMinification(raw, f.path) : raw;
        rawLen += raw.length;
        minLen += min.length;
        const shaped: FileEntry = { ...f, content: min };
        delete (shaped as { lastModified?: string }).lastModified;
        delete (shaped as { lastModifiedBy?: string }).lastModifiedBy;
        return shaped;
      }),
    }));
    const rawTokens = Math.ceil(rawLen / 4);
    const minTokens = Math.ceil(minLen / 4);
    const fileCount = shapedGroups.reduce(
      (n, g) => n + (g.files?.length ?? 0),
      0
    );
    responseData.results = shapedGroups as typeof responseData.results;
    responseData.hints = [
      `${fileCount} files, ${totalLines} lines, ~${rawTokens}→${minTokens} tokens (minified)`,
    ];
    const userPassedFullContent = queriesWithVerbosity.some(
      q => q.fullContent === true
    );
    if (userPassedFullContent) {
      const downgrade: GroupedToolWarning = {
        kind: 'verbosity-downgrade',
        field: 'fullContent',
        detail: 'fullContent=true minified under concise',
      };
      responseData.warnings = [...(responseData.warnings ?? []), downgrade];
    }
    return true;
  }

  if (anyCompact) {
    responseData.hints = compactTrimHints(
      responseData.hints,
      isAdvisoryFetchContentHint,
      2
    );
  }
  return false;
}
