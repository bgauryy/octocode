import { lstat, realpath } from 'node:fs/promises';
import { dirname } from 'node:path';
import { acquireRootLock } from './rootLock.js';
import { recoverTransactions, serializeApply } from './transaction.js';
import type { AstRewriteRuntimeDeps } from './types.js';

export type SafetyResult<T> =
  | { ok: true; value: T }
  | {
      ok: false;
      errorCode: string;
      error: string;
      details?: Record<string, unknown>;
    };

/** Hold the canonical-root process lock through recovery and the full operation. */
export async function withAstRewriteSafety<T>(
  path: string,
  apply: boolean,
  deps: AstRewriteRuntimeDeps,
  operation: () => Promise<T>
): Promise<SafetyResult<T>> {
  let lockRoot: string;
  try {
    const canonical = await realpath(path);
    const info = await lstat(canonical);
    lockRoot = info.isDirectory() ? canonical : dirname(canonical);
  } catch {
    return { ok: true, value: await operation() };
  }

  const acquired = await acquireRootLock(lockRoot, {
    ...(deps.lockTimeoutMs === undefined
      ? {}
      : { timeoutMs: deps.lockTimeoutMs }),
    ...(deps.lockPollMs === undefined ? {} : { pollMs: deps.lockPollMs }),
  });
  if (acquired.ok === false) {
    return {
      ok: false,
      errorCode:
        acquired.reason === 'timeout'
          ? 'ast.rewrite.lock_timeout'
          : 'ast.rewrite.lock_unavailable',
      error: acquired.error,
    };
  }

  try {
    const recovery = await recoverTransactions(lockRoot, deps.rename);
    if (!recovery.ok) {
      return {
        ok: false,
        errorCode: 'ast.rewrite.recovery_failed',
        error:
          'An interrupted astRewrite transaction could not be recovered safely.',
        details: { errors: recovery.errors },
      };
    }
    const value = await (apply ? serializeApply(operation) : operation());
    return { ok: true, value };
  } finally {
    await acquired.lock.release();
  }
}
