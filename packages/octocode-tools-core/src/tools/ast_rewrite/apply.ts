import { realpath } from 'node:fs/promises';
import { isAbsolute } from 'node:path';
import { evaluatePostconditions } from './postcondition.js';
import { isWithin, selectPreparedMatches } from './prepare.js';
import { rewriteError } from './result.js';
import { applyTransaction } from './transaction.js';
import type {
  AstRewriteError,
  AstRewriteMatch,
  AstRewriteQuery,
  AstRewriteRuntimeDeps,
  PreparedFile,
} from './types.js';

export interface AppliedRewrite {
  files: PreparedFile[];
  matches: AstRewriteMatch[];
  transaction: {
    id: string;
    committed: true;
    files: number;
    cleanupWarnings?: string[];
    beforeHashes: Record<string, string>;
    afterHashes: Record<string, string>;
  };
}

export async function applyPreparedRewrite(options: {
  query: AstRewriteQuery;
  files: PreparedFile[];
  matches: AstRewriteMatch[];
  boundary: string;
  executable: string;
  deps: AstRewriteRuntimeDeps;
  maxPatchBytes: number;
}): Promise<
  { ok: true; applied: AppliedRewrite } | { ok: false; result: AstRewriteError }
> {
  const selected = options.query.selectedMatchIds
    ? selectPreparedMatches(
        options.files,
        options.query.selectedMatchIds,
        options.maxPatchBytes
      )
    : { ok: true as const, files: options.files, matches: options.matches };
  if (!selected.ok) return selected;

  const normalizedExpected = new Map<string, string>();
  for (const [path, expected] of Object.entries(
    options.query.expectedHashes ?? {}
  )) {
    if (!isAbsolute(path) || !/^[a-f0-9]{64}$/i.test(expected)) {
      return {
        ok: false,
        result: rewriteError(
          'ast.rewrite.expected_hash_invalid',
          'Expected hash keys must be absolute paths and values must be SHA-256 hex digests.',
          { details: { path } }
        ),
      };
    }
    let canonical: string;
    try {
      canonical = await realpath(path);
    } catch {
      return {
        ok: false,
        result: rewriteError(
          'ast.rewrite.expected_hash_invalid',
          'An expected hash path could not be resolved.',
          { details: { path } }
        ),
      };
    }
    if (!isWithin(options.boundary, canonical)) {
      return {
        ok: false,
        result: rewriteError(
          'ast.rewrite.expected_hash_invalid',
          'An expected hash path is outside the real requested root.',
          { details: { path } }
        ),
      };
    }
    normalizedExpected.set(canonical, expected.toLowerCase());
  }
  const currentPaths = new Set(selected.files.map(file => file.absolutePath));
  if (
    normalizedExpected.size !== currentPaths.size ||
    [...normalizedExpected.keys()].some(path => !currentPaths.has(path))
  ) {
    return {
      ok: false,
      result: rewriteError(
        'ast.rewrite.expected_hash_set_mismatch',
        'Apply requires exactly the affected file paths returned by the matching preview.',
        {
          details: {
            expectedPaths: [...normalizedExpected.keys()].sort(),
            actualPaths: [...currentPaths].sort(),
          },
        }
      ),
    };
  }
  for (const file of selected.files) {
    const expected = normalizedExpected.get(file.absolutePath);
    if (!expected || expected !== file.beforeHash) {
      return {
        ok: false,
        result: rewriteError(
          expected
            ? 'ast.rewrite.hash_mismatch'
            : 'ast.rewrite.expected_hash_missing',
          expected
            ? 'An expected source hash no longer matches; preview again before applying.'
            : 'Apply requires the preview beforeHash for every affected absolute path.',
          {
            details: {
              path: file.absolutePath,
              ...(expected ? { expected, actual: file.beforeHash } : {}),
            },
          }
        ),
      };
    }
  }

  const postconditions = await evaluatePostconditions(
    options.query,
    options.executable,
    selected.files,
    options.deps
  );
  if (!postconditions.ok) return postconditions;

  const applied = await applyTransaction(
    selected.files.map(file => ({
      absolutePath: file.absolutePath,
      before: file.before,
      after: file.after,
      mode: file.mode,
    })),
    options.deps.rename,
    { rootBoundary: options.boundary }
  );
  if (!applied.ok) {
    return {
      ok: false,
      result: rewriteError(
        'ast.rewrite.transaction_failed',
        applied.rollback.restored
          ? 'The rewrite transaction failed; automatic recovery completed.'
          : 'The rewrite transaction failed; automatic recovery is incomplete. Inspect the recovery errors before retrying.',
        { details: { cause: applied.error, rollback: applied.rollback } }
      ),
    };
  }
  return {
    ok: true,
    applied: {
      files: selected.files,
      matches: selected.matches,
      transaction: {
        ...applied.receipt,
        beforeHashes: Object.fromEntries(
          selected.files.map(file => [file.absolutePath, file.beforeHash])
        ),
        afterHashes: Object.fromEntries(
          selected.files.map(file => [file.absolutePath, file.afterHash])
        ),
      },
    },
  };
}
