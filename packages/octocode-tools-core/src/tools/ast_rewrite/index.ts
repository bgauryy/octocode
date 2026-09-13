import { lstat, mkdtemp, realpath, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { spawnWithTimeout } from '../../utils/exec/spawn/wrappers.js';
import { applyPreparedRewrite } from './apply.js';
import { resolveAstGrepExecutable } from './executable.js';
import { buildArgs, decodeMatches, scanSucceeded } from './astGrep.js';
import { prepareFiles } from './prepare.js';
import { publicFiles, rewriteError as error } from './result.js';
import { createRewriteSnapshot } from './snapshot.js';
import { withAstRewriteSafety } from './safety.js';
import type {
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
const DEFAULT_MAX_MATCHES = 10_000;
const DEFAULT_PAGE_SIZE = 100;
const ISOLATION_RECEIPT = {
  workingDirectory: 'ephemeral' as const,
  inheritedHome: false as const,
  repositoryConfig: 'not-discovered' as const,
};

async function runAstRewriteUnlocked(
  query: AstRewriteQuery,
  deps: AstRewriteRuntimeDeps = {}
): Promise<AstRewriteResult> {
  const timeoutMs = deps.timeoutMs ?? DEFAULT_TIMEOUT_MS;
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
  if (
    query.ruleKind !== 'pattern' &&
    query.ruleKind !== undefined &&
    !resolvedExecutable.executable.capabilities.includes('inline-rules')
  ) {
    return error(
      'ast.rewrite.capability_incompatible',
      'This ast-grep executable does not support isolated inline rules.'
    );
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
  const isolationDirectory = await mkdtemp(
    join(tmpdir(), 'octocode-ast-rewrite-run-')
  );
  let execution: Awaited<ReturnType<typeof spawnWithTimeout>>;
  try {
    execution = await spawnWithTimeout(
      resolvedExecutable.executable.path,
      buildArgs(query, realRoot),
      {
        cwd: isolationDirectory,
        timeout: timeoutMs,
        maxOutputSize: deps.maxProcessOutputBytes ?? DEFAULT_MAX_OUTPUT_BYTES,
      }
    );
  } finally {
    await rm(isolationDirectory, { recursive: true, force: true });
  }
  if (!scanSucceeded(execution)) {
    return error(
      execution.timedOut
        ? 'ast.rewrite.timeout'
        : execution.outputLimitExceeded
          ? 'ast.rewrite.output_limit'
          : 'ast.rewrite.execution_failed',
      execution.error?.message || execution.stderr.trim() || 'ast-grep failed.',
      execution.outputLimitExceeded ? { terminalLimit: true } : {}
    );
  }
  const rawMatches = decodeMatches(execution.stdout);
  if (!rawMatches) {
    return error(
      'ast.rewrite.output_invalid',
      'ast-grep returned output that does not match its versioned JSON contract.'
    );
  }
  const maxMatches = query.maxMatches ?? DEFAULT_MAX_MATCHES;
  if (rawMatches.length > maxMatches) {
    return error(
      'ast.rewrite.match_limit',
      `The rewrite found ${rawMatches.length} matches, exceeding maxMatches=${maxMatches}. Narrow the scope.`,
      {
        terminalLimit: true,
        details: { observed: rawMatches.length, maxMatches },
      }
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
        selectedMatchIds: _selectedMatchIds,
        postconditions: _postconditions,
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
      isolation: ISOLATION_RECEIPT,
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
    boundary,
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
      selectedMatchIds: _selectedMatchIds,
      postconditions: _postconditions,
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
  let resultFiles = prepared.files;
  let resultMatches = prepared.matches;
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
    const applied = await applyPreparedRewrite({
      query,
      files: prepared.files,
      matches: prepared.matches,
      boundary,
      executable: resolvedExecutable.executable.path,
      deps,
      maxPatchBytes: deps.maxPatchBytes ?? DEFAULT_MAX_PATCH_BYTES,
    });
    if (!applied.ok) return applied.result;
    resultFiles = applied.applied.files;
    resultMatches = applied.applied.matches;
    transaction = applied.applied.transaction;
  }

  const page = query.apply ? 1 : (query.page ?? 1);
  const pageSize = query.apply
    ? resultMatches.length
    : (query.pageSize ?? DEFAULT_PAGE_SIZE);
  const offset = (page - 1) * pageSize;
  const pageMatches = query.apply
    ? resultMatches
    : resultMatches.slice(offset, offset + pageSize);
  const hasMore = !query.apply && offset + pageSize < resultMatches.length;
  const totalPages = Math.max(1, Math.ceil(resultMatches.length / pageSize));
  return {
    operation: 'rewrite',
    mode: query.apply ? 'apply' : 'preview',
    root: realRoot,
    snapshot,
    executable: resolvedExecutable.executable,
    isolation: ISOLATION_RECEIPT,
    totalMatches: resultMatches.length,
    affectedFiles: resultFiles.length,
    matches: pageMatches,
    files: publicFiles(resultFiles),
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
  if (query.apply && !query.snapshot) {
    return error(
      'ast.rewrite.snapshot_required',
      'Apply requires the exact snapshot returned by preview.'
    );
  }

  const result = await withAstRewriteSafety(
    query.path,
    query.apply === true,
    deps,
    () => runAstRewriteUnlocked(query, deps)
  );
  return result.ok
    ? result.value
    : error(result.errorCode, result.error, { details: result.details });
}
