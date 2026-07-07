/**
 * Single entry for OQL adapters to invoke a backing tool.
 *
 * Routes through `executeDirectTool` — the SAME path the CLI's `tools <name>`
 * uses — so OQL gets identical credential resolution (withSecurityValidation),
 * provider initialization, input validation, and secret sanitization. Calling
 * the bulk runners directly bypassed auth (the CLI runs `runOqlSearch` without
 * an MCP `authInfo`), which silently failed GitHub/npm calls.
 *
 * GitHub/package tools require `mainResearchGoal`; we fill OQL defaults.
 */
import type { CallToolResult } from '@modelcontextprotocol/sdk/types.js';
import { executeDirectTool } from '../../tools/directToolCatalog.js';

const RESEARCH_META = {
  mainResearchGoal: 'octocode search (OQL)',
  researchGoal: 'OQL target execution',
  reasoning: 'Compiled from an OQL query.',
};

/** Run one backing tool with a single query, via the shared direct-tool path. */
export function runDirect(
  name: string,
  query: Record<string, unknown>
): Promise<CallToolResult> {
  return executeDirectTool(name, {
    queries: [{ ...RESEARCH_META, ...query }],
  });
}

/** Pull the single query's `{data,status}` out of a bulk-shaped tool result. */
export function firstQueryData<T = Record<string, unknown>>(
  result: CallToolResult
): { data?: T; status?: string } {
  const sc = result.structuredContent as
    { results?: Array<{ status?: string; data?: unknown }> } | undefined;
  const first = sc?.results?.[0];
  return { data: first?.data as T | undefined, status: first?.status };
}

export function numberFrom(value: unknown): number | undefined {
  return typeof value === 'number' && Number.isFinite(value)
    ? value
    : undefined;
}

export function stringFrom(value: unknown): string | undefined {
  return typeof value === 'string' ? value : undefined;
}
