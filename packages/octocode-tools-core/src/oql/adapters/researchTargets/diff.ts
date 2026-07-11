/**
 * `target:"diff"` has two typed lanes, discriminated by params shape:
 *   - PR patch:    { prNumber, files? }            -> ghHistoryResearch patches
 *   - direct file: { baseRef, headRef, path }      -> two ghGetFileContent reads
 *                                                     + a pure local line diff
 * A request that fits neither returns a repair diagnostic rather than silently
 * falling through to a PR-patch call.
 */
import nodePath from 'node:path';
import type { CallToolResult } from '@modelcontextprotocol/sdk/types.js';
import { runDirect } from '../runner.js';
import { diagnostic } from '../../diagnostics.js';
import { classifyDiffLane } from '../../diffLanes.js';
import { spawnWithTimeout } from '../../../utils/exec/spawn.js';
import type { AdapterResult } from '../local.js';
import { finishRecords } from './pagination.js';
import { errorText, isExistingDirectory, params, splitRepo } from './shared.js';
import type { OqlQuery, OqlRecordResultRow } from '../../types.js';

/**
 * Pull file content/status/error out of a ghGetFileContent (or localGetFileContent)
 * result. The row sits directly under structuredContent.results[0] with the file
 * in files[0] (no nested `.data`); some shapes nest under `.data` or `.results`.
 * Used by the direct two-ref diff lanes — reading `.data.content` is always
 * undefined for this tool and previously masqueraded as "files identical".
 */
function ghFileContentResult(result: CallToolResult): {
  content?: string;
  status?: string;
  error?: unknown;
} {
  const sc = result.structuredContent as
    { results?: Array<Record<string, unknown>> } | undefined;
  const row = sc?.results?.[0];
  if (!row) return {};
  const data = ('data' in row ? row.data : row) as
    Record<string, unknown> | undefined;
  const fileRow =
    (data?.files as Array<Record<string, unknown>> | undefined)?.[0] ??
    (data?.results as Array<Record<string, unknown>> | undefined)?.[0] ??
    data ??
    {};
  const content = fileRow.content;
  return {
    content: typeof content === 'string' ? content : undefined,
    status: row.status as string | undefined,
    error: fileRow.error ?? data?.error ?? row.error,
  };
}

export async function executeDiff(query: OqlQuery): Promise<AdapterResult> {
  const p = params(query);
  const { owner, repo } = splitRepo(query.from);
  // Lane discriminant is shared with the planner (diffLanes.ts) — one source of
  // truth, so dry-run plan and execution can never disagree.
  const lane = classifyDiffLane(p);

  if (lane.kind === 'prPatch') {
    // PR patch lane (unchanged behavior).
    const result = await runDirect('ghHistoryResearch', {
      ...(owner ? { owner } : {}),
      ...(repo ? { repo } : {}),
      content: { patches: { mode: 'all' } },
      ...p,
    });
    return finishRecords(
      result,
      'diff',
      'ghHistoryResearch',
      query.from ?? { kind: 'github' }
    );
  }

  if (lane.kind === 'directFile') {
    if (query.from?.kind === 'local' || query.from?.kind === 'materialized') {
      return executeLocalDirectFileDiff(query, {
        baseRef: lane.baseRef,
        headRef: lane.headRef,
        path: lane.path,
      });
    }
    return executeDirectFileDiff(query, owner, repo, {
      baseRef: lane.baseRef,
      headRef: lane.headRef,
      path: lane.path,
    });
  }

  return {
    results: [],
    diagnostics: [
      diagnostic(
        'invalidQuery',
        'target:"diff" needs either {prNumber} (PR patch diff) or {baseRef,headRef,path} (direct file diff between two refs).',
        {
          backend: 'ghHistoryResearch',
          repair: {
            message:
              'Add params.prNumber for a PR patch, or params.baseRef + params.headRef + params.path for a direct file diff.',
          },
        }
      ),
    ],
    provenance: [],
  };
}

/** Direct two local files via two content reads + a pure local line diff. */
// Git refs the diff lane will pass to `git show` — conservative shape, and
// never starting with '-' so a ref can't be parsed as an option.
const SAFE_GIT_REF = /^[A-Za-z0-9][A-Za-z0-9._/@^~-]*$/;

async function executeLocalDirectFileDiff(
  query: OqlQuery,
  refs: { baseRef: string; headRef: string; path: string }
): Promise<AdapterResult> {
  const source = query.from;
  const basePath =
    source?.kind === 'local'
      ? source.path
      : source?.kind === 'materialized'
        ? source.localPath
        : undefined;
  if (!basePath) {
    return {
      results: [],
      diagnostics: [
        diagnostic('invalidQuery', 'Local direct file diff needs from.path.', {
          backend: 'localGetFileContent',
        }),
      ],
      provenance: [],
    };
  }

  const invalid = (message: string): AdapterResult => ({
    results: [],
    diagnostics: [diagnostic('invalidQuery', message, { backend: 'git' })],
    provenance: [{ backend: 'git', source: query.from }],
  });

  // The lane contract is "path at baseRef vs path at headRef", so both sides
  // come from git object storage — not from files on disk (the worktree may
  // hold neither ref's version).
  const gitCwd = isExistingDirectory(basePath)
    ? basePath
    : nodePath.dirname(basePath);
  const rel = nodePath.isAbsolute(refs.path)
    ? nodePath.relative(gitCwd, refs.path)
    : refs.path;
  if (!rel || rel.startsWith('..') || rel.startsWith('-')) {
    return invalid(
      `params.path must resolve inside from.path for a local ref diff (got "${refs.path}").`
    );
  }
  if (!SAFE_GIT_REF.test(refs.baseRef) || !SAFE_GIT_REF.test(refs.headRef)) {
    return invalid(
      'baseRef/headRef must be plain git revisions (branch, tag, sha, HEAD~N).'
    );
  }

  // `ref:path` is repo-root-relative in git; the `./` prefix makes it
  // cwd-relative so from.path anchors the lookup as documented.
  const relPosix = `./${rel.split(nodePath.sep).join('/')}`;
  const show = (ref: string) =>
    spawnWithTimeout('git', ['-C', gitCwd, 'show', `${ref}:${relPosix}`], {
      timeout: 15_000,
    });
  const [base, head] = await Promise.all([
    show(refs.baseRef),
    show(refs.headRef),
  ]);
  if (!base.success || !head.success) {
    const failed = !base.success ? base : head;
    const ref = !base.success ? refs.baseRef : refs.headRef;
    return invalid(
      `git show ${ref}:${relPosix} failed: ${failed.stderr.trim().split('\n')[0] || failed.error?.message || `exit ${failed.exitCode}`}. from.path must be inside a git repository and the path must exist at both refs.`
    );
  }

  const diff = computeLineDiff(base.stdout, head.stdout);
  const row: OqlRecordResultRow = {
    kind: 'record',
    recordType: 'diff',
    id: `${refs.baseRef}..${refs.headRef}:${rel}`,
    ...(query.from ? { source: query.from } : {}),
    data: {
      path: rel,
      baseRef: refs.baseRef,
      headRef: refs.headRef,
      additions: diff.additions,
      deletions: diff.deletions,
      patch: diff.patch,
      unchanged: diff.unchanged,
    },
  };

  return {
    results: [row],
    diagnostics:
      diff.additions === 0 && diff.deletions === 0
        ? [
            diagnostic(
              'zeroMatches',
              `${rel} is identical at ${refs.baseRef} and ${refs.headRef}.`,
              { backend: 'git', severity: 'info', blocksAnswer: false }
            ),
          ]
        : [],
    provenance: [{ backend: 'git', source: query.from }],
  };
}

/** Direct two-ref GitHub file diff via two content reads + a pure local line diff. */
async function executeDirectFileDiff(
  query: OqlQuery,
  owner: string | undefined,
  repo: string | undefined,
  refs: { baseRef: string; headRef: string; path: string }
): Promise<AdapterResult> {
  if (!owner || !repo) {
    return {
      results: [],
      diagnostics: [
        diagnostic(
          'invalidQuery',
          'Direct file diff needs a concrete owner/repo.',
          { backend: 'ghGetFileContent' }
        ),
      ],
      provenance: [],
    };
  }

  const read = (ref: string) =>
    runDirect('ghGetFileContent', {
      owner,
      repo,
      path: refs.path,
      branch: ref,
      fullContent: true,
      minify: 'none',
    });

  const [baseRes, headRes] = await Promise.all([
    read(refs.baseRef),
    read(refs.headRef),
  ]);

  // ghGetFileContent returns the row directly under structuredContent.results[0]
  // (keys: id/owner/repo/files) with the file content in files[0].content — there
  // is no nested `.data`, so firstQueryData(...).data is empty. Reading it as
  // `.content` was always undefined, which previously masqueraded as "identical".
  const base = ghFileContentResult(baseRes);
  const head = ghFileContentResult(headRes);
  const unresolvedRef = [
    { label: 'base', ref: refs.baseRef, ...base },
    { label: 'head', ref: refs.headRef, ...head },
  ].find(item => item.status === 'error' || typeof item.content !== 'string');

  if (unresolvedRef) {
    const err = errorText(
      unresolvedRef.error,
      `Could not read ${unresolvedRef.label} ref "${unresolvedRef.ref}" for ${refs.path}.`
    );
    return {
      results: [],
      diagnostics: [
        diagnostic('invalidQuery', err, { backend: 'ghGetFileContent' }),
      ],
      provenance: [{ backend: 'ghGetFileContent', source: query.from }],
    };
  }

  const diff = computeLineDiff(base.content ?? '', head.content ?? '');
  const row: OqlRecordResultRow = {
    kind: 'record',
    recordType: 'diff',
    id: refs.path,
    ...(query.from ? { source: query.from } : {}),
    data: {
      path: refs.path,
      baseRef: refs.baseRef,
      headRef: refs.headRef,
      additions: diff.additions,
      deletions: diff.deletions,
      patch: diff.patch,
      unchanged: diff.unchanged,
    },
  };
  return {
    results: [row],
    diagnostics:
      diff.additions === 0 && diff.deletions === 0
        ? [
            diagnostic('zeroMatches', 'Files are identical at both refs.', {
              backend: 'ghGetFileContent',
              severity: 'info',
              blocksAnswer: false,
            }),
          ]
        : [],
    provenance: [{ backend: 'ghGetFileContent', source: query.from }],
  };
}

export interface LineDiff {
  additions: number;
  deletions: number;
  unchanged: number;
  /** Unified-style patch text (`+`/`-`/` ` line prefixes). */
  patch: string;
}

/**
 * Minimal LCS-based line diff between two file bodies. Pure and dependency-free
 * so it is unit-testable without any backend. Not a byte-perfect git patch —
 * a line-granular additions/deletions view for direct two-ref comparison.
 */
export function computeLineDiff(baseText: string, headText: string): LineDiff {
  const a = baseText === '' ? [] : baseText.split('\n');
  const b = headText === '' ? [] : headText.split('\n');
  const n = a.length;
  const m = b.length;

  // LCS length table.
  const lcs: number[][] = Array.from({ length: n + 1 }, () =>
    new Array<number>(m + 1).fill(0)
  );
  for (let i = n - 1; i >= 0; i--) {
    for (let j = m - 1; j >= 0; j--) {
      lcs[i]![j] =
        a[i] === b[j]
          ? lcs[i + 1]![j + 1]! + 1
          : Math.max(lcs[i + 1]![j]!, lcs[i]![j + 1]!);
    }
  }

  const lines: string[] = [];
  let additions = 0;
  let deletions = 0;
  let unchanged = 0;
  let i = 0;
  let j = 0;
  while (i < n && j < m) {
    if (a[i] === b[j]) {
      lines.push(`  ${a[i]}`);
      unchanged++;
      i++;
      j++;
    } else if (lcs[i + 1]![j]! >= lcs[i]![j + 1]!) {
      lines.push(`- ${a[i]}`);
      deletions++;
      i++;
    } else {
      lines.push(`+ ${b[j]}`);
      additions++;
      j++;
    }
  }
  while (i < n) {
    lines.push(`- ${a[i++]}`);
    deletions++;
  }
  while (j < m) {
    lines.push(`+ ${b[j++]}`);
    additions++;
  }

  return { additions, deletions, unchanged, patch: lines.join('\n') };
}
