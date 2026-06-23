import { type CallToolResult } from '@modelcontextprotocol/sdk/types.js';
import type { AuthInfo } from '@modelcontextprotocol/sdk/server/auth/types.js';
import { OqlSearchInputSchema } from './scheme.js';
import type { OqlRunResult, OqlSearchInput } from '../../oql/types.js';

type OqlSearchToolInput = Record<string, unknown> & {
  authInfo?: AuthInfo;
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
  return {
    content: [
      {
        type: 'text',
        text: JSON.stringify(result, null, 2),
      },
    ],
    structuredContent: result as unknown as Record<string, unknown>,
  };
}
