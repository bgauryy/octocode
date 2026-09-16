/**
 * Web tool — Pi tool wrapper around runWebTool from src/web.ts.
 * One tool for both web search and page fetch, no API key required.
 * SSRF-hardened: private/loopback/link-local/metadata IPs blocked.
 * Migrated to universal queries[] envelope with per-query reasoning.
 */
import { runWebTool, renderWebResult } from '../web.js';
import { propagateOctocodeEnv, getOctocodeHome } from '@octocodeai/config';
import { CLI_STATUS_TEXT } from '../tui/cli-design.js';
import type { ToolDefinition, PiTheme, ToolCallResult } from '../types.js';
import { DIRECT_TOOL_DESCRIPTIONS, type registerUniqueTool } from './octocode-tools.js';
import { buildToolView } from './render-helpers.js';
import { buildQueryEnvelopeSchema, executeQueryBatch } from './query-envelope.js';

import { z } from 'zod';
type RegisterFn = typeof registerUniqueTool;

// Lazy env-refresh: propagateOctocodeEnv runs once at activation, but if Pi
// started before all keys existed in ~/.octocode/.env, this ensures they land
// in process.env on the first web-tool call instead of failing silently.
let _webEnvEnsured = false;
function ensureWebEnv(): void {
  if (_webEnvEnsured) return;
  _webEnvEnsured = true;
  try {
    propagateOctocodeEnv({ home: getOctocodeHome(), trusted: false });
  } catch {
    // Non-fatal: fall back to whatever is already in process.env.
  }
}

export function registerWebTool(
  pi: { registerTool?(def: ToolDefinition): void },
  registeredToolNames: Set<string>,
  registerFn: RegisterFn,
): void {
  const parameters = buildQueryEnvelopeSchema(
    z.looseObject({
      url: z.string().optional().describe('Absolute http(s) URL to fetch and read as text.'),
      query: z.string().optional().describe('Search query; omit when url is given.'),
      maxResults: z.number().int().min(1).max(20).optional()
        .describe('Search: max results (default 5).'),
      maxChars: z.number().int().min(500).max(50000).optional()
        .describe('Max chars of page text per page (default 15000).'),
      page: z.number().int().min(1).max(20).optional()
        .describe('Page for long docs (default 1); maxChars chars each. Advance when truncated:true.'),
      engine: z.enum(['tavily', 'serper', 'exa', 'duckduckgo']).optional()
        .describe('Force provider: tavily|serper|exa|duckduckgo (default: auto).'),
      timeRange: z.enum(['day', 'week', 'month', 'year']).optional()
        .describe('Recency: day|week|month|year.'),
      includeDomains: z.array(z.string()).optional()
        .describe('Tavily allowlist domains, e.g. ["docs.python.org"].'),
      excludeDomains: z.array(z.string()).optional()
        .describe('Tavily blocklist domains.'),
      exaType: z.enum(['auto', 'neural', 'keyword']).optional()
        .describe('Exa result type: auto (default), neural (semantic), keyword (exact-match).'),
      exaCategory: z.string().optional()
        .describe('Exa category: "research paper", "news", "github", "company", "pdf".'),
    }),
    { reasoningDescription: 'Concise reason this web fetch or search is necessary.', allowParallel: true },
  );

  registerFn(pi, registeredToolNames, {
    name: 'web',
    label: 'Web',
    description:
      DIRECT_TOOL_DESCRIPTIONS.web!,
    promptSnippet: 'Search the web or fetch and read a page',
    promptGuidelines: [
      'url takes precedence over query. Fetch a discovered URL before using as evidence.',
      'On truncated:true, continue same URL with next page+maxChars; partial page cannot prove absence.',
      'On 403/bot/empty: use search-discovered alternative; repeating blocked fetch adds no evidence.',
      'Omit engine for auto-select. Tavily only: includeDomains/excludeDomains. Provider key needs config.',
      'Timeouts fixed: 15s fetch, 30s search. No per-call override.',
    ],
    parameters,

    async execute(
      toolCallId: string,
      params: Record<string, unknown>,
      signal?: AbortSignal,
      onUpdate?: unknown,
    ): Promise<ToolCallResult> {
      return executeQueryBatch({
        toolCallId,
        raw: params,
        signal,
        onUpdate: typeof onUpdate === 'function' ? onUpdate as (update: ToolCallResult) => void : undefined,
        passthroughSingle: true,
        allowParallel: true,
        preflight(query) {
          const hasUrl = typeof query['url'] === 'string' && (query['url'] as string).trim().length > 0;
          const hasQuery = typeof query['query'] === 'string' && (query['query'] as string).trim().length > 0;
          if (!hasUrl && !hasQuery) {
            throw new Error(
              'web requires either url (to fetch a page) or query (to search). ' +
              'Both are missing — provide url or query.',
            );
          }
        },
        async execute(query, _index, _callId, batchSignal) {
          ensureWebEnv();
          const out = await runWebTool(
            query as Parameters<typeof runWebTool>[0],
            { signal: batchSignal, env: process.env },
          );
          const errorMsg = (out as { error?: string }).error;
          if (errorMsg) throw new Error(errorMsg);
          return {
            content: [{ type: 'text' as const, text: renderWebResult(out) }],
            details: out,
          };
        },
      });
    },

    renderCall(args: unknown, theme?: PiTheme) {
      const envelope = (args ?? {}) as Record<string, unknown>;
      const queries = Array.isArray(envelope['queries'])
        ? (envelope['queries'] as Record<string, unknown>[])
        : [];
      const a = queries[0] ?? envelope;
      const url = typeof a['url'] === 'string' && a['url'] ? (a['url'] as string) : '';
      const query = typeof a['query'] === 'string' && a['query'] ? (a['query'] as string) : '';
      const displayUrl = url.length > 70 ? `${url.slice(0, 67)}\u2026` : url;
      const displayQuery = query.length > 70 ? `${query.slice(0, 67)}\u2026` : query;
      return buildToolView({
        name: 'web',
        state: 'request',
        segments: url
          ? [{ text: 'fetch', token: 'bright' }, { text: displayUrl, token: 'link' }]
          : query
            ? [{ text: 'search', token: 'bright' }, { text: `"${displayQuery}"`, token: 'dim' }]
            : [],
      }, theme);
    },

    renderResult(result: ToolCallResult, opts: { expanded?: boolean; isPartial?: boolean }, theme?: PiTheme) {
      if (opts.isPartial) {
        return buildToolView(() => ({ name: 'web', state: 'running', status: CLI_STATUS_TEXT.fetching }), theme);
      }
      const ok = !result.isError;
      const det = result.details as Record<string, unknown> | null;
      const segments: Array<{ text: string; token: 'count' | 'warning' | 'dim' }> = [];
      if (Array.isArray((det as Record<string, unknown> | null)?.results)) {
        const n = ((det as Record<string, unknown>).results as unknown[]).length;
        segments.push({ text: `${n} result${n === 1 ? '' : 's'}`, token: 'count' });
      } else if (det?.url) {
        const truncated = det.truncated === true;
        const pg = typeof det.page === 'number' && det.page > 1 ? ` p${det.page}` : '';
        segments.push({ text: `page${pg}`, token: 'count' });
        if (truncated) segments.push({ text: 'more pages available', token: 'warning' });
      }
      if (!opts.expanded) {
        return buildToolView({ name: 'web', state: ok ? 'success' : 'error', segments }, theme);
      }
      const text = (result.content as Array<{ type: string; text: string }>)
        ?.find?.((p) => p.type === 'text')?.text ?? '';
      const allLines = text.split('\n');
      const lines = allLines.slice(0, 20);
      const omitted = allLines.length - lines.length;
      return buildToolView({
        name: 'web',
        state: ok ? 'success' : 'error',
        segments,
        body: lines.map((text) => ({ text, token: ok ? 'dim' : 'error' })),
        hint: omitted > 0 ? `${omitted} more lines hidden in this view` : undefined,
      }, theme);
    },
  } satisfies ToolDefinition);
}
