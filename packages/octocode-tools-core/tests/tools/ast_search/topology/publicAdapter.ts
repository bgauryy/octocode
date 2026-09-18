import type { CallToolResult } from '@modelcontextprotocol/server';

import { AstSearchQuerySchema, type AstSearchQuery } from '@octocodeai/octocode-core/schema';
import { executeAstSearch } from '../../../../src/tools/ast_search/execution.js';
import {
  GraphAnalysisQuerySchema,
  type TopologyAnalysisQuery,
} from '../../../../src/tools/ast_search/topology/scheme.js';

type PublicTopologyQuery = AstSearchQuery | TopologyAnalysisQuery;

function toPublicTopologyQuery(query: unknown): AstSearchQuery {
  const publicQuery = AstSearchQuerySchema.safeParse(query);
  if (publicQuery.success && publicQuery.data.operation === 'topology') {
    return publicQuery.data;
  }

  const topologyQuery = GraphAnalysisQuerySchema.parse(query);
  const { operation: analysis, ...scope } = topologyQuery;
  return AstSearchQuerySchema.parse({
    operation: 'topology',
    analysis,
    ...scope,
  });
}

function validatePublicContinuations(result: CallToolResult): CallToolResult {
  const structured = result.structuredContent as {
    results?: Array<{ data?: Record<string, unknown> }>;
  };
  for (const row of structured.results ?? []) {
    const next = row.data?.next;
    if (!next || typeof next !== 'object') continue;
    for (const continuation of Object.values(next)) {
      if (!continuation || typeof continuation !== 'object') continue;
      const candidate = continuation as { tool?: unknown; query?: unknown };
      if (candidate.tool === 'astSearch') {
        AstSearchQuerySchema.parse(candidate.query);
      }
    }
  }
  return result;
}

/** Test-only adapter: all executable topology tests go through public astSearch. */
export async function runPublicTopology(args: {
  queries: PublicTopologyQuery[];
}): Promise<CallToolResult> {
  const queries = args.queries.map(query =>
    toPublicTopologyQuery({
      reasoning: 'Exercise public astSearch topology behavior and continuations.',
      debug: true,
      ...query,
    })
  );
  return validatePublicContinuations(await executeAstSearch({ queries }));
}
