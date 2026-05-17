import type {
  BitbucketCodeSearchItem,
  BitbucketDiffstatEntry,
  BitbucketPaginatedResponse,
  BitbucketPullRequest,
  BitbucketRepository,
  BitbucketTreeEntry,
} from './types.js';

export function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null;
}

function readString(record: Record<string, unknown>, key: string): string {
  const value = record[key];
  return typeof value === 'string' ? value : '';
}

function readNumber(
  record: Record<string, unknown>,
  key: string
): number | undefined {
  const value = record[key];
  return typeof value === 'number' ? value : undefined;
}

function isCodeSearchSegment(value: unknown): boolean {
  return isRecord(value) && typeof value.text === 'string';
}

function isCodeSearchLine(value: unknown): boolean {
  return (
    isRecord(value) &&
    typeof value.line === 'number' &&
    Array.isArray(value.segments) &&
    value.segments.every(isCodeSearchSegment)
  );
}

function isCodeSearchContentMatch(value: unknown): boolean {
  return (
    isRecord(value) &&
    Array.isArray(value.lines) &&
    value.lines.every(isCodeSearchLine)
  );
}

function toContentMatches(
  value: unknown
): BitbucketCodeSearchItem['content_matches'] {
  return Array.isArray(value)
    ? (value.filter(
        isCodeSearchContentMatch
      ) as BitbucketCodeSearchItem['content_matches'])
    : [];
}

function toCodeSearchItem(value: unknown): BitbucketCodeSearchItem | null {
  if (!isRecord(value)) return null;
  const file = isRecord(value.file) ? value.file : {};
  const links = isRecord(file.links) ? file.links : undefined;
  const self = links && isRecord(links.self) ? links.self : undefined;
  const selfHref =
    self && typeof self.href === 'string' ? self.href : undefined;
  return {
    type: readString(value, 'type'),
    content_matches: toContentMatches(value.content_matches),
    path_matches:
      toContentMatches(value.path_matches).length > 0
        ? toContentMatches(value.path_matches)
        : undefined,
    file: {
      path: readString(file, 'path'),
      type: readString(file, 'type'),
      ...(selfHref ? { links: { self: { href: selfHref } } } : {}),
    },
  };
}

export function parseBitbucketCodeSearchPage(value: unknown): {
  values: BitbucketCodeSearchItem[];
  size: number;
  next?: string;
  page: number;
} | null {
  if (!isRecord(value)) return null;
  const rawValues = value.values;
  const values = (Array.isArray(rawValues) ? rawValues : [])
    .map(toCodeSearchItem)
    .filter((item): item is BitbucketCodeSearchItem => item !== null);
  const size = readNumber(value, 'size') ?? values.length;
  const next = typeof value.next === 'string' ? value.next : undefined;
  const page = readNumber(value, 'page') ?? 1;
  return { values, size, next, page };
}

export function parseBitbucketPaginatedResponse<T>(
  value: unknown,
  itemGuard: (item: unknown) => item is T
): BitbucketPaginatedResponse<T> | null {
  if (!isRecord(value)) return null;
  const values = Array.isArray(value.values) ? value.values : [];
  return {
    values: values.filter(itemGuard),
    page: readNumber(value, 'page'),
    pagelen: readNumber(value, 'pagelen'),
    size: readNumber(value, 'size'),
    next: typeof value.next === 'string' ? value.next : undefined,
    previous: typeof value.previous === 'string' ? value.previous : undefined,
  };
}

export function isBitbucketRepository(
  value: unknown
): value is BitbucketRepository {
  return (
    isRecord(value) &&
    typeof value.name === 'string' &&
    typeof value.full_name === 'string' &&
    typeof value.slug === 'string'
  );
}

export function isBitbucketPullRequest(
  value: unknown
): value is BitbucketPullRequest {
  return (
    isRecord(value) &&
    typeof value.id === 'number' &&
    typeof value.title === 'string' &&
    typeof value.state === 'string' &&
    isRecord(value.source) &&
    isRecord(value.destination)
  );
}

export function isBitbucketTreeEntry(
  value: unknown
): value is BitbucketTreeEntry {
  return (
    isRecord(value) &&
    (value.type === 'commit_file' || value.type === 'commit_directory') &&
    typeof value.path === 'string'
  );
}

export function isBitbucketDiffstatEntry(
  value: unknown
): value is BitbucketDiffstatEntry {
  return (
    isRecord(value) &&
    typeof value.type === 'string' &&
    typeof value.status === 'string' &&
    typeof value.lines_added === 'number' &&
    typeof value.lines_removed === 'number'
  );
}

export function parseBitbucketDefaultBranch(value: unknown): string | null {
  if (!isRecord(value) || !isRecord(value.mainbranch)) return null;
  return typeof value.mainbranch.name === 'string'
    ? value.mainbranch.name
    : null;
}
