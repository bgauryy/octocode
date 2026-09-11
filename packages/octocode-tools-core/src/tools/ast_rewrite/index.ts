import { lstat, realpath } from 'node:fs/promises';
import { dirname, isAbsolute } from 'node:path';
import { spawnWithTimeout } from '../../utils/exec/spawn/wrappers.js';
import { resolveAstGrepExecutable } from './executable.js';
import { buildArgs, decodeMatches } from './astGrep.js';
import { isWithin, prepareFiles, type PreparedFile } from './prepare.js';
import { createRewriteSnapshot } from './snapshot.js';
import { applyTransaction, serializeApply } from './transaction.js';
import type {
  AstRewriteError,
  AstRewriteFile,
  AstRewriteQuery,
  AstRewriteResult,
  AstRewriteRuntimeDeps,
} from './types.js';

export type {
  AstRewriteEmpty,
  AstRewriteError,
  AstRewriteFile,
  AstRewriteMatch,
  AstRewriteQuery,
  AstRewriteResult,
  AstRewriteRuntimeDeps,
  AstRewriteSuccess,
} from './types.js';

const DEFAULT_TIMEOUT_MS = 30_000;
const DEFAULT_MAX_OUTPUT_BYTES = 10 * 1024 * 1024;
const DEFAULT_MAX_PATCH_BYTES = 512 * 1024;
const DEFAULT_MAX_FILES = 2_000;
const DEFAULT_PAGE_SIZE = 100;

function error(
  errorCode: string,
  message: string,
  extra: Partial<AstRewriteError> = {}
): AstRewriteError {
  return {
    status: 'error',
    operation: 'rewrite',
    errorCode,
    error: message,
    complete: false,
    isPartial: true,
    ...extra,
  };
}

function publicFiles(files: PreparedFile[]): AstRewriteFile[] {
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

async function runAstRewriteUnlocked(
  query: AstRewriteQuery,
  deps: AstRewriteRuntimeDeps = {}
): Promise<AstRewriteResult> {
  const timeoutMs = deps.timeoutMs ?? DEFAULT_TIMEOUT_MS;
  if (query.apply && !deps.allowApply) {
    return error(
      'ast.rewrite.apply_disabled',
      'Applying rewrites requires the separate astRewrite apply capability.'
    );
  }
  if ((query.page ?? 1) < 1 || (query.pageSize ?? DEFAULT_PAGE_SIZE) < 1) {
    return error(
      'ast.rewrite.pagination_invalid',
      'page and pageSize must be positive integers.'
    );
  }

  const resolvedExecutable = await resolveAstGrepExecutable({
    explicit: deps.executable,
    timeoutMs,
  });
  if (resolvedExecutable.ok === false) {
    return error(resolvedExecutable.errorCode, resolvedExecutable.error);
  }

  let realRoot: string;
  let rootInfo: Awaited<ReturnType<typeof lstat>>;
  try {
    realRoot = await realpath(query.path);
    rootInfo = await lstat(realRoot);
  } catch {
    return error(
      'ast.rewrite.root_unavailable',
      'The requested rewrite path does not exist.'
    );
  }
  if (!rootInfo.isDirectory() && !rootInfo.isFile()) {
    return error(
      'ast.rewrite.root_invalid',
      'The requested rewrite path must be a file or directory.'
    );
  }
  const boundary = rootInfo.isDirectory() ? realRoot : dirname(realRoot);
  const cwd = boundary;
  const target = rootInfo.isDirectory() ? '.' : realRoot;
  const execution = await spawnWithTimeout(
    resolvedExecutable.executable.path,
    buildArgs(query, target),
    {
      cwd,
      timeout: timeoutMs,
      maxOutputSize: deps.maxProcessOutputBytes ?? DEFAULT_MAX_OUTPUT_BYTES,
    }
  );
  if (!execution.success) {
    return error(
      execution.timedOut
        ? 'ast.rewrite.timeout'
        : execution.outputLimitExceeded
          ? 'ast.rewrite.output_limit'
          : 'ast.rewrite.execution_failed',
      execution.error?.message || execution.stderr.trim() || 'ast-grep failed.'
    );
  }
  const rawMatches = decodeMatches(execution.stdout);
  if (!rawMatches) {
    return error(
      'ast.rewrite.output_invalid',
      'ast-grep returned output that does not match its versioned JSON contract.'
    );
  }
  if (rawMatches.length === 0) {
    const snapshot = createRewriteSnapshot(
      query,
      resolvedExecutable.executable,
      realRoot,
      [],
      [],
      query.maxFiles ?? DEFAULT_MAX_FILES,
      query.pageSize ?? DEFAULT_PAGE_SIZE
    );
    if ((query.apply || (query.page ?? 1) > 1) && query.snapshot !== snapshot) {
      const {
        snapshot: _snapshot,
        expectedHashes: _expectedHashes,
        ...scope
      } = query;
      return error(
        'ast.rewrite.snapshot_changed',
        'The source, executable, query, or selected file set changed. Preview again before continuing.',
        {
          details: { snapshot },
          next: {
            restart: {
              tool: 'astRewrite',
              query: { ...scope, apply: false, page: 1 },
              confidence: 'exact',
            },
          },
        }
      );
    }
    return {
      status: 'empty',
      operation: 'rewrite',
      mode: query.apply ? 'apply' : 'preview',
      root: realRoot,
      executable: resolvedExecutable.executable,
      totalMatches: 0,
      affectedFiles: 0,
      matches: [],
      files: [],
      complete: true,
      isPartial: false,
    };
  }

  const prepared = await prepareFiles(
    rawMatches,
    realRoot,
    boundary,
    cwd,
    query.maxFiles ?? DEFAULT_MAX_FILES,
    deps.maxPatchBytes ?? DEFAULT_MAX_PATCH_BYTES
  );
  if (prepared.ok === false) return prepared.result;

  const snapshot = createRewriteSnapshot(
    query,
    resolvedExecutable.executable,
    realRoot,
    prepared.files,
    prepared.matches.map(match => match.id),
    query.maxFiles ?? DEFAULT_MAX_FILES,
    query.pageSize ?? DEFAULT_PAGE_SIZE
  );
  if ((query.apply || (query.page ?? 1) > 1) && query.snapshot !== snapshot) {
    const {
      snapshot: _snapshot,
      expectedHashes: _expectedHashes,
      ...scope
    } = query;
    return error(
      'ast.rewrite.snapshot_changed',
      'The source, executable, query, or selected file set changed. Preview again before continuing.',
      {
        details: { snapshot },
        next: {
          restart: {
            tool: 'astRewrite',
            query: { ...scope, apply: false, page: 1 },
            confidence: 'exact',
          },
        },
      }
    );
  }
  const beforeHashes = Object.fromEntries(
    prepared.files.map(file => [file.absolutePath, file.beforeHash])
  );
  const afterHashes = Object.fromEntries(
    prepared.files.map(file => [file.absolutePath, file.afterHash])
  );
  let transaction:
    | {
        id: string;
        committed: true;
        files: number;
        cleanupWarnings?: string[];
        beforeHashes: Record<string, string>;
        afterHashes: Record<string, string>;
      }
    | undefined;

  if (query.apply) {
    const normalizedExpected = new Map<string, string>();
    for (const [path, expected] of Object.entries(query.expectedHashes ?? {})) {
      if (!isAbsolute(path) || !/^[a-f0-9]{64}$/i.test(expected)) {
        return error(
          'ast.rewrite.expected_hash_invalid',
          'Expected hash keys must be absolute paths and values must be SHA-256 hex digests.',
          { details: { path } }
        );
      }
      let canonical: string;
      try {
        canonical = await realpath(path);
      } catch {
        return error(
          'ast.rewrite.expected_hash_invalid',
          'An expected hash path could not be resolved.',
          { details: { path } }
        );
      }
      if (!isWithin(boundary, canonical)) {
        return error(
          'ast.rewrite.expected_hash_invalid',
          'An expected hash path is outside the real requested root.',
          { details: { path } }
        );
      }
      normalizedExpected.set(canonical, expected.toLowerCase());
    }
    const currentPaths = new Set(prepared.files.map(file => file.absolutePath));
    if (
      normalizedExpected.size !== currentPaths.size ||
      [...normalizedExpected.keys()].some(path => !currentPaths.has(path))
    ) {
      return error(
        'ast.rewrite.expected_hash_set_mismatch',
        'Apply requires exactly the affected file paths returned by the matching preview.',
        {
          details: {
            expectedPaths: [...normalizedExpected.keys()].sort(),
            actualPaths: [...currentPaths].sort(),
          },
        }
      );
    }
    for (const file of prepared.files) {
      const expected = normalizedExpected.get(file.absolutePath);
      if (!expected) {
        return error(
          'ast.rewrite.expected_hash_missing',
          'Apply requires the preview beforeHash for every affected absolute path.',
          { details: { path: file.absolutePath } }
        );
      }
      if (expected !== file.beforeHash) {
        return error(
          'ast.rewrite.hash_mismatch',
          'An expected source hash no longer matches; preview again before applying.',
          {
            details: {
              path: file.absolutePath,
              expected,
              actual: file.beforeHash,
            },
          }
        );
      }
    }
    const applied = await applyTransaction(
      prepared.files.map(file => ({
        absolutePath: file.absolutePath,
        before: file.before,
        after: file.after,
        mode: file.mode,
      })),
      deps.rename
    );
    if (applied.ok === false) {
      return error(
        'ast.rewrite.transaction_failed',
        'The rewrite could not be committed; staged changes were rolled back.',
        { details: { cause: applied.error, rollback: applied.rollback } }
      );
    }
    transaction = {
      ...applied.receipt,
      beforeHashes,
      afterHashes,
    };
  }

  const page = query.page ?? 1;
  const pageSize = query.pageSize ?? DEFAULT_PAGE_SIZE;
  const offset = (page - 1) * pageSize;
  const pageMatches = query.apply
    ? prepared.matches
    : prepared.matches.slice(offset, offset + pageSize);
  const hasMore = !query.apply && offset + pageSize < prepared.matches.length;
  const totalPages = Math.max(1, Math.ceil(prepared.matches.length / pageSize));
  return {
    operation: 'rewrite',
    mode: query.apply ? 'apply' : 'preview',
    root: realRoot,
    snapshot,
    executable: resolvedExecutable.executable,
    totalMatches: prepared.matches.length,
    affectedFiles: prepared.files.length,
    matches: pageMatches,
    files: publicFiles(prepared.files),
    complete: !hasMore,
    isPartial: hasMore,
    pagination: {
      currentPage: page,
      totalPages,
      pageSize,
      hasMore,
    },
    ...(hasMore
      ? {
          next: {
            nextPage: {
              tool: 'astRewrite' as const,
              query: {
                ...query,
                apply: false,
                page: page + 1,
                pageSize,
                snapshot,
              },
              confidence: 'exact' as const,
            },
          },
        }
      : {}),
    ...(transaction ? { transaction } : {}),
  };
}

export async function runAstRewrite(
  query: AstRewriteQuery,
  deps: AstRewriteRuntimeDeps = {}
): Promise<AstRewriteResult> {
  if (query.apply && !deps.allowApply) {
    return error(
      'ast.rewrite.apply_disabled',
      'Applying rewrites requires the separate astRewrite apply capability.'
    );
  }
  if (query.apply) {
    if (!query.snapshot) {
      return error(
        'ast.rewrite.snapshot_required',
        'Apply requires the exact snapshot returned by preview.'
      );
    }
    return serializeApply(() => runAstRewriteUnlocked(query, deps));
  }
  return runAstRewriteUnlocked(query, deps);
}
