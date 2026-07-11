/** Per-row continuation builder for `semantics` record rows. */
import path from 'node:path';
import { statSync } from 'node:fs';
import type {
  OqlContinuation,
  OqlQuery,
  OqlRecordResultRow,
  OqlResultRow,
} from '../../types.js';
import type { ContinuationCtx } from './types.js';

export function buildSemanticsContinuations(
  row: OqlResultRow,
  ctx: ContinuationCtx
): Record<string, OqlContinuation> | undefined {
  const data = (row as OqlRecordResultRow).data;
  const rawUri = typeof data?.uri === 'string' ? data.uri : undefined;
  const sourceAnchor =
    semanticSourceAnchor(row as OqlRecordResultRow) ??
    semanticQueryAnchor(ctx.query);
  const uri = semanticContinuationUri(rawUri, sourceAnchor);
  if (!uri) return undefined;
  const line =
    typeof data.line === 'number'
      ? data.line
      : typeof data.startLine === 'number'
        ? data.startLine
        : undefined;
  return {
    'next.fetch': {
      query: {
        schema: 'oql',
        target: 'content',
        from: { kind: 'local', path: uri },
        fetch: {
          content: {
            contentView: 'none',
            ...(line ? { range: { startLine: line, contextLines: 2 } } : {}),
          },
        },
      },
      why: 'Read the code at this symbol location.',
      confidence: 'exact',
    },
  };
}

function semanticContinuationUri(
  rawUri: string | undefined,
  sourceAnchor: string | undefined
): string | undefined {
  if (rawUri && path.isAbsolute(rawUri)) return rawUri;
  if (sourceAnchor && path.isAbsolute(sourceAnchor)) {
    const base = semanticAnchorBase(sourceAnchor);
    return rawUri ? path.resolve(base, rawUri) : sourceAnchor;
  }
  return sourceAnchor ?? rawUri;
}

function semanticAnchorBase(sourceAnchor: string): string {
  try {
    return statSync(sourceAnchor).isDirectory()
      ? sourceAnchor
      : path.dirname(sourceAnchor);
  } catch {
    return path.dirname(sourceAnchor);
  }
}

function semanticSourceAnchor(row: OqlRecordResultRow): string | undefined {
  const source = row.source;
  if (source?.kind === 'local') return source.path;
  if (source?.kind === 'materialized') return source.localPath;
  return undefined;
}

function semanticQueryAnchor(query: OqlQuery): string | undefined {
  if (query.from?.kind === 'local') return query.from.path;
  if (query.from?.kind === 'materialized') return query.from.localPath;
  return undefined;
}
