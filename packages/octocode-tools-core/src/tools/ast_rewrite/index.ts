import { lstat, realpath } from 'node:fs/promises';
import { dirname } from 'node:path';
import { contextUtils } from '../../utils/contextUtils.js';
import { buildRuleConfigJson } from './nativeRewrite.js';
import { applyPreparedRewrite } from './apply.js';
import { prepareFiles } from './prepare.js';
import { publicFiles, rewriteError as error } from './result.js';
import { createRewriteSnapshot } from './snapshot.js';
import { withAstRewriteSafety } from './safety.js';
import type {
  AstGrepJsonMatch,
  AstRewriteExecutableReceipt,
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

const DEFAULT_MAX_PATCH_BYTES = 512 * 1024;
const DEFAULT_MAX_FILES = 2_000;
const DEFAULT_MAX_MATCHES = 10_000;
const DEFAULT_PAGE_SIZE = 100;
const ISOLATION_RECEIPT = {
  workingDirectory: 'ephemeral' as const,
  inheritedHome: false as const,
  repositoryConfig: 'not-discovered' as const,
};

/** Synthetic receipt emitted when the Rust engine handles the rewrite in-process. */
const NATIVE_EXECUTABLE_RECEIPT: AstRewriteExecutableReceipt = {
  path: 'native',
  version: 'embedded',
  sha256: '',
  capabilityContract: 1,
  capabilityDigest: 'native',
  capabilities: ['pattern', 'inline-rules', 'experimental'],
};

/** Low-level shape returned per file by the Rust engine JSON. */
type NativeRewriteMatch = {
  byteStart: number;
  byteEnd: number;
  range: {
    start: { line: number; column: number };
    end: { line: number; column: number };
  };
  text: string;
  replacedText: string;
  replacement: string;
  captures: Record<string, { kind: string; texts: string[] }>;
};
type NativeRewriteFileResult = { path: string; matches: NativeRewriteMatch[] };

/** Map a single Rust-engine rewrite match to the shared AstGrepJsonMatch shape. */
function nativeToAstGrepMatch(
  file: string,
  m: NativeRewriteMatch
): AstGrepJsonMatch {
  const single: Record<string, { text: string }> = {};
  const multi: Record<string, Array<{ text: string }>> = {};
  const transformed: Record<string, string> = {};
  for (const [name, cap] of Object.entries(m.captures ?? {})) {
    if (cap.kind === 'single') single[name] = { text: cap.texts[0] ?? '' };
    else if (cap.kind === 'multi') multi[name] = cap.texts.map(t => ({ text: t }));
    else if (cap.kind === 'transformed') transformed[name] = cap.texts[0] ?? '';
  }
  const hasCaptures = Object.keys(m.captures ?? {}).length > 0;
  return {
    file,
    text: m.text,
    replacement: m.replacement,
    range: {
      byteOffset: { start: m.byteStart, end: m.byteEnd },
      start: m.range.start,
      end: m.range.end,
    },
    ...(hasCaptures ? { metaVariables: { single, multi, transformed } } : {}),
  };
}

async function runAstRewriteUnlocked(
  query: AstRewriteQuery,
  deps: AstRewriteRuntimeDeps = {}
): Promise<AstRewriteResult> {
  if ((query.page ?? 1) < 1 || (query.pageSize ?? DEFAULT_PAGE_SIZE) < 1) {
    return error(
      'ast.rewrite.pagination_invalid',
      'page and pageSize must be positive integers.'
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

  // ── Native in-process rewrite (no external ast-grep binary) ─────────────
  const ruleConfigJson = buildRuleConfigJson(query);
  let nativeResultJson: string;
  try {
    nativeResultJson = await contextUtils.structuralRewriteFiles({
      path: realRoot,
      ruleConfigJson,
      include: query.include,
      exclude: query.exclude,
      maxFiles: query.maxFiles ?? DEFAULT_MAX_FILES,
    });
  } catch (execError) {
    return error(
      'ast.rewrite.execution_failed',
      execError instanceof Error
        ? execError.message
        : 'Native structural rewrite failed.'
    );
  }
  const nativeFiles: NativeRewriteFileResult[] = JSON.parse(nativeResultJson);
  const rawMatches = nativeFiles.flatMap(({ path: file, matches }) =>
    matches.map(m => nativeToAstGrepMatch(file, m))
  );
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
      NATIVE_EXECUTABLE_RECEIPT,
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
      executable: NATIVE_EXECUTABLE_RECEIPT,
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
    NATIVE_EXECUTABLE_RECEIPT,
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
      executable: NATIVE_EXECUTABLE_RECEIPT.path,
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
    executable: NATIVE_EXECUTABLE_RECEIPT,
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
