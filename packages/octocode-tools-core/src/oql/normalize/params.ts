/* ------------------------------- params ---------------------------------- */
import type { OqlInputQuery, OqlQuery } from '../types.js';

const GRAPH_LSP_PROOF_TERMS = [
  'relationship',
  'relationships',
  'reference',
  'references',
  'who uses',
  'used by',
  'usage',
  'caller',
  'callers',
  'callee',
  'callees',
  'call hierarchy',
  'blast radius',
  'safe to delete',
  'what breaks',
  'delete',
  'dead code',
  'unused export',
  'unused symbol',
  'retained by',
];

export function normalizeParams(
  raw: OqlInputQuery,
  target: OqlQuery['target']
): Record<string, unknown> | undefined {
  const params = raw.params
    ? { ...(raw.params as Record<string, unknown>) }
    : undefined;
  if (target !== 'graph' || !params) return params;
  if (!shouldDefaultGraphLspProof(params)) return params;
  return {
    ...params,
    proof: 'lsp',
    proofLimit:
      typeof params.proofLimit === 'number' && params.proofLimit > 0
        ? params.proofLimit
        : 5,
  };
}

function shouldDefaultGraphLspProof(params: Record<string, unknown>): boolean {
  if (params.proof !== undefined || params.mode === 'plan') return false;
  if (params.mode === 'prove') return false;
  if (params.relation !== undefined || params.direction !== undefined)
    return true;
  const goal = typeof params.goal === 'string' ? params.goal.toLowerCase() : '';
  return GRAPH_LSP_PROOF_TERMS.some(term => goal.includes(term));
}
