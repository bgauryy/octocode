import { type CallToolResult } from '@modelcontextprotocol/sdk/types.js';
import type { AuthInfo } from '@modelcontextprotocol/sdk/server/auth/types.js';
import { OqlSearchInputSchema } from '../../oql/schema.js';
import {
  isBatchEnvelope,
  type OqlContinuation,
  type OqlContinuationHint,
  type OqlResultEnvelope,
  type OqlRunResult,
  type OqlSearchInput,
} from '../../oql/types.js';

type OqlSearchToolInput = Record<string, unknown> & {
  authInfo?: AuthInfo;
};

type OqlDirectResultRow = {
  id: string;
  status?: 'empty' | 'error';
  data: OqlDirectResultEnvelope;
};

type OqlDirectResultEnvelope = OqlResultEnvelope & {
  nextHints?: Record<string, OqlContinuationHint>;
};

type OqlDirectStructuredContent = {
  results: OqlDirectResultRow[];
  mode?: 'independent' | 'merge';
  merged?: OqlResultEnvelope;
  diagnostics?: OqlResultEnvelope['diagnostics'];
};

export async function executeOqlSearchTool(
  input: OqlSearchToolInput
): Promise<CallToolResult> {
  const oqlInput = stripTransportFields(input);
  const parsed = OqlSearchInputSchema.safeParse(oqlInput);

  if (!parsed.success) {
    throw parsed.error;
  }

  const { runOqlSearch } = await import('../../oql/run.js');
  const result = await runOqlSearch(parsed.data as OqlSearchInput, {
    authInfo: input.authInfo,
  });

  return formatOqlResult(result);
}

function stripTransportFields(
  input: OqlSearchToolInput
): Record<string, unknown> {
  const next = { ...input };
  delete next.authInfo;
  delete next.sessionId;
  delete next.responseCharOffset;
  delete next.responseCharLength;
  return next;
}

function formatOqlResult(result: OqlRunResult): CallToolResult {
  const structuredContent = buildOqlDirectStructuredContent(result);
  const hasError = structuredContent.results.some(
    row => row.status === 'error'
  );

  return {
    content: [
      {
        // Emit the same compacted payload as structuredContent — serializing
        // the raw run result here duplicated the uncompacted envelope and wasted
        // tokens versus every other tool. Sanitization still happens at egress.
        type: 'text',
        text: JSON.stringify(structuredContent, null, 2),
      },
    ],
    ...(hasError ? { isError: true } : {}),
    structuredContent: structuredContent as unknown as Record<string, unknown>,
  };
}

function buildOqlDirectStructuredContent(
  result: OqlRunResult
): OqlDirectStructuredContent {
  if (!isBatchEnvelope(result)) {
    return {
      results: [toDirectResultRow(result, result.queryId ?? 'oqlSearch-1')],
    };
  }

  return {
    results: result.children.map(child =>
      toDirectResultRow(child.envelope, child.queryId)
    ),
    mode: result.mode,
    ...(result.merged
      ? { merged: compactDirectOqlEnvelope(result.merged) }
      : {}),
    ...(result.diagnostics.length > 0
      ? { diagnostics: result.diagnostics }
      : {}),
  };
}

function toDirectResultRow(
  envelope: OqlResultEnvelope,
  fallbackId: string
): OqlDirectResultRow {
  const data = compactDirectOqlEnvelope(envelope);
  return {
    id: envelope.queryId ?? fallbackId,
    ...directStatus(envelope),
    data,
  };
}

function compactDirectOqlEnvelope(
  envelope: OqlResultEnvelope
): OqlDirectResultEnvelope {
  const nextHints: Record<string, OqlContinuationHint> = {
    ...(envelope.nextHints ?? {}),
  };
  let hasHints = Object.keys(nextHints).length > 0;

  const results = envelope.results.map(row => {
    const next = row.next;
    if (!next) return row;

    const compactNext: Record<string, Partial<OqlContinuation>> = {};
    let changed = false;

    for (const [key, continuation] of Object.entries(next)) {
      if (!continuation.why || !continuation.confidence) {
        compactNext[key] = continuation;
        continue;
      }
      const hint = {
        why: continuation.why,
        confidence: continuation.confidence,
      };
      const existing = nextHints[key];
      if (!existing) {
        nextHints[key] = hint;
        hasHints = true;
      }

      if (!existing || hintsEqual(existing, hint)) {
        const { why, confidence, ...queryOnly } = continuation;
        void why;
        void confidence;
        compactNext[key] = queryOnly;
        changed = true;
      } else {
        compactNext[key] = continuation;
      }
    }

    return changed ? { ...row, next: compactNext } : row;
  });

  return {
    ...envelope,
    results: results as OqlResultEnvelope['results'],
    ...(hasHints ? { nextHints } : {}),
  };
}

function hintsEqual(
  left: OqlContinuationHint,
  right: OqlContinuationHint
): boolean {
  return left.why === right.why && left.confidence === right.confidence;
}

function directStatus(
  envelope: OqlResultEnvelope
): Pick<OqlDirectResultRow, 'status'> {
  if (
    envelope.evidence.kind === 'unsupported' ||
    envelope.diagnostics.some(d => d.severity === 'error')
  ) {
    return { status: 'error' };
  }

  if (envelope.results.length === 0) {
    return { status: 'empty' };
  }

  return {};
}
