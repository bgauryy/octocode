/**
 * `target:"artifacts"` adapter: wraps localBinaryInspect. An artifact is a
 * single entity, so we keep ONE record row carrying the full payload (mode,
 * entries/strings/symbols, derived localPath, nextScanOffset) rather than
 * expanding inner arrays into rows — otherwise parent-level metadata
 * (localPath, scan cursor) would be lost to the continuation builders.
 */
import { runDirect, firstQueryData } from '../runner.js';
import { diagnostic } from '../../diagnostics.js';
import type { AdapterResult } from '../local.js';
import { statusDiagnostics } from './pagination.js';
import { records } from './rows.js';
import { params } from './shared.js';
import type { OqlContinuation, OqlDiagnostic, OqlQuery } from '../../types.js';

export async function executeArtifacts(
  query: OqlQuery
): Promise<AdapterResult> {
  const path =
    query.from?.kind === 'local'
      ? query.from.path
      : query.from?.kind === 'materialized'
        ? query.from.localPath
        : undefined;
  if (!path) {
    return {
      results: [],
      diagnostics: [
        diagnostic(
          'invalidQuery',
          'target:"artifacts" needs a local file `from` (path).',
          { backend: 'localBinaryInspect' }
        ),
      ],
      provenance: [],
    };
  }
  const result = await runDirect('localBinaryInspect', {
    path,
    ...params(query),
  });
  const { data, status } = firstQueryData(result);
  const diagnostics = statusDiagnostics(result, 'localBinaryInspect');
  if (status === 'error' || !data) {
    return {
      results: [],
      diagnostics: diagnostics.length
        ? diagnostics
        : [
            diagnostic('zeroMatches', 'localBinaryInspect returned no data.', {
              backend: 'localBinaryInspect',
              severity: 'info',
              blocksAnswer: false,
            }),
          ],
      provenance: [{ backend: 'localBinaryInspect', source: query.from }],
    };
  }
  return {
    results: records([data], 'artifact', query.from),
    diagnostics: [...diagnostics, ...artifactPartialDiagnostics(data, query)],
    provenance: [{ backend: 'localBinaryInspect', source: query.from }],
  };
}

type ArtifactTextPagination = {
  hasMore?: boolean;
  nextCharOffset?: number;
  charLength?: number;
};

function artifactPartialDiagnostics(
  data: Record<string, unknown>,
  query: OqlQuery
): OqlDiagnostic[] {
  const pagination =
    data.pagination && typeof data.pagination === 'object'
      ? (data.pagination as ArtifactTextPagination)
      : undefined;
  if (data.isPartial !== true && pagination?.hasMore !== true) return [];
  return [
    diagnostic(
      'partialResult',
      'Artifact text is paginated; follow the artifact continuation before treating the inline content as complete.',
      {
        backend: 'localBinaryInspect',
        blocksAnswer: true,
        continuation: artifactContentContinuation(query, pagination),
      }
    ),
  ];
}

function artifactContentContinuation(
  query: OqlQuery,
  pagination: ArtifactTextPagination | undefined
): OqlContinuation | undefined {
  if (
    pagination?.hasMore !== true ||
    typeof pagination.nextCharOffset !== 'number'
  ) {
    return undefined;
  }
  return {
    query: {
      ...query,
      params: {
        ...(query.params ?? {}),
        charOffset: pagination.nextCharOffset,
        ...(typeof pagination.charLength === 'number'
          ? { charLength: pagination.charLength }
          : {}),
      },
    },
    why: 'Read the next inline artifact text window.',
    confidence: 'exact',
  };
}
