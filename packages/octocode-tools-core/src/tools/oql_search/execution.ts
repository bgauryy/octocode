import { type CallToolResult } from '@modelcontextprotocol/sdk/types.js';
import type { AuthInfo } from '@modelcontextprotocol/sdk/server/auth/types.js';
import { OqlSearchInputSchema } from '../../oql/schema.js';
import {
  isBatchEnvelope,
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
  data: OqlResultEnvelope;
};

type OqlDirectStructuredContent = {
  results: OqlDirectResultRow[];
  oql: OqlRunResult;
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
        type: 'text',
        text: JSON.stringify(result, null, 2),
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
      oql: result,
    };
  }

  return {
    results: result.children.map(child =>
      toDirectResultRow(child.envelope, child.queryId)
    ),
    oql: result,
    mode: result.mode,
    ...(result.merged ? { merged: result.merged } : {}),
    ...(result.diagnostics.length > 0
      ? { diagnostics: result.diagnostics }
      : {}),
  };
}

function toDirectResultRow(
  envelope: OqlResultEnvelope,
  fallbackId: string
): OqlDirectResultRow {
  return {
    id: envelope.queryId ?? fallbackId,
    ...directStatus(envelope),
    data: envelope,
  };
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
