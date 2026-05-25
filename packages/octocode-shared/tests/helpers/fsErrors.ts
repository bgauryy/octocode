/**
 * File System Error Helpers
 *
 * Utilities for creating mock filesystem errors in tests.
 * These simulate real error conditions like disk full, permission denied, etc.
 */

/**
 * Create a Node.js filesystem error with the proper structure
 */
function createFsError(
  code: string,
  message: string,
  path?: string
): NodeJS.ErrnoException {
  const fullMessage = path
    ? `${code}: ${message}, '${path}'`
    : `${code}: ${message}`;
  const error = new Error(fullMessage) as NodeJS.ErrnoException;
  error.code = code;
  if (path) {
    error.path = path;
  }
  return error;
}

/**
 * Common filesystem errors for testing
 */
export const FS_ERRORS = {
  /** Disk full / no space left on device */
  ENOSPC: (path?: string) =>
    createFsError('ENOSPC', 'no space left on device', path),

  /** Disk quota exceeded */
  EDQUOT: (path?: string) =>
    createFsError('EDQUOT', 'disk quota exceeded', path),

  /** Permission denied */
  EACCES: (path?: string) => createFsError('EACCES', 'permission denied', path),

  /** I/O error */
  EIO: (path?: string) => createFsError('EIO', 'i/o error', path),

  /** No such file or directory */
  ENOENT: (path: string) =>
    createFsError('ENOENT', `no such file or directory, open`, path),

  /** Cross-device link not permitted (rename across filesystems) */
  EXDEV: (oldPath?: string, newPath?: string) =>
    createFsError(
      'EXDEV',
      `cross-device link not permitted${oldPath ? `, '${oldPath}' -> '${newPath}'` : ''}`,
      oldPath
    ),

  /** Read-only file system */
  EROFS: (path?: string) =>
    createFsError('EROFS', 'read-only file system', path),

  /** File is busy */
  EBUSY: (path?: string) =>
    createFsError('EBUSY', 'resource busy or locked', path),

  /** Too many open files */
  EMFILE: () => createFsError('EMFILE', 'too many open files'),

  /** Invalid argument */
  EINVAL: (message?: string) =>
    createFsError('EINVAL', message ?? 'invalid argument'),

  /** File exists (for exclusive create operations) */
  EEXIST: (path?: string) =>
    createFsError('EEXIST', 'file already exists', path),
};

/**
 * Create a session with specific stats for testing edge cases
 */
export function createTestSession(overrides: {
  sessionId?: string;
  version?: number;
  createdAt?: string;
  lastActiveAt?: string;
  stats?: Partial<{
    toolCalls: number | string | null | undefined | object;
    errors: number | string | null | undefined | object;
    rateLimits: number | string | null | undefined | object;
  }>;
}) {
  return {
    version: overrides.version ?? 1,
    sessionId: overrides.sessionId ?? 'test-uuid',
    createdAt: overrides.createdAt ?? '2026-01-09T10:00:00.000Z',
    lastActiveAt: overrides.lastActiveAt ?? '2026-01-09T10:00:00.000Z',
    stats: {
      toolCalls: overrides.stats?.toolCalls ?? 0,
      errors: overrides.stats?.errors ?? 0,
      rateLimits: overrides.stats?.rateLimits ?? 0,
    },
  };
}

/**
 * Generate truncated JSON content at various points
 */
export function generateTruncatedJson(
  fullJson: string,
  truncateAt: 'start' | 'middle' | 'end' | number
): string {
  const length = fullJson.length;

  switch (truncateAt) {
    case 'start':
      return fullJson.slice(0, 10);
    case 'middle':
      return fullJson.slice(0, Math.floor(length / 2));
    case 'end':
      return fullJson.slice(0, length - 1); // Missing closing brace
    default:
      return fullJson.slice(0, truncateAt);
  }
}

/**
 * Generate malformed JSON strings for testing
 */
export const MALFORMED_JSON = {
  unclosedBrace: '{"unclosed": "string',
  trailingComma: '{"version": 1,}',
  unquotedKey: '{version: 1}',
  null: 'null',
  undefined: 'undefined',
  array: '[]',
  emptyString: '',
  randomGarbage: 'invalid json{{{',
  doubleComma: '{"a": 1,, "b": 2}',
  missingValue: '{"a": }',
  singleQuotes: "{'version': 1}",
};
