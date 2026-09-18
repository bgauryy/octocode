import type { AstRewriteError, AstRewriteFile, PreparedFile } from './types.js';

export function rewriteError(
  errorCode: string,
  message: string,
  extra: Partial<AstRewriteError> = {}
): AstRewriteError {
  return {
    status: 'error',
    operation: 'rewrite',
    errorCode,
    error: message,
    ...(extra.terminalLimit
      ? { complete: false as const, isPartial: true as const }
      : {}),
    ...extra,
  };
}

export function publicFiles(files: PreparedFile[]): AstRewriteFile[] {
  return files.map(
    ({
      before: _before,
      after: _after,
      mode: _mode,
      matches: _matches,
      ...file
    }) => file
  );
}
