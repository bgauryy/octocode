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

export const FS_ERRORS = {
  ENOSPC: (path?: string) =>
    createFsError('ENOSPC', 'no space left on device', path),

  EDQUOT: (path?: string) =>
    createFsError('EDQUOT', 'disk quota exceeded', path),

  EACCES: (path?: string) => createFsError('EACCES', 'permission denied', path),

  EIO: (path?: string) => createFsError('EIO', 'i/o error', path),

  ENOENT: (path: string) =>
    createFsError('ENOENT', `no such file or directory, open`, path),

  EXDEV: (oldPath?: string, newPath?: string) =>
    createFsError(
      'EXDEV',
      `cross-device link not permitted${oldPath ? `, '${oldPath}' -> '${newPath}'` : ''}`,
      oldPath
    ),

  EROFS: (path?: string) =>
    createFsError('EROFS', 'read-only file system', path),

  EBUSY: (path?: string) =>
    createFsError('EBUSY', 'resource busy or locked', path),

  EMFILE: () => createFsError('EMFILE', 'too many open files'),

  EINVAL: (message?: string) =>
    createFsError('EINVAL', message ?? 'invalid argument'),

  EEXIST: (path?: string) =>
    createFsError('EEXIST', 'file already exists', path),
};

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
      return fullJson.slice(0, length - 1);
    default:
      return fullJson.slice(0, truncateAt);
  }
}

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
