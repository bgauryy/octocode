/**
 * Web tool — Pi tool wrapper around runWebTool from src/web.ts.
 * One tool for both web search and page fetch, no API key required.
 * SSRF-hardened: private/loopback/link-local/metadata IPs blocked.
 * Migrated to universal queries[] envelope with per-query reasoning.
 */
import { runWebTool, renderWebResult } from '../web.js';
import { propagateOctocodeEnv, getOctocodeHome } from '../env.js';
import { CLI_STATUS_TEXT } from '../tui/cli-design.js';
import type { TSchema, ToolDefinition, PiTheme, ToolCallResult } from '../types.js';
import type { registerUniqueTool } from './octocode-tools.js';
import { buildToolView } from './render-helpers.js';
import { buildQueryEnvelopeSchema, executeQueryBatch } from './query-envelope.js';

type TypeBoxBuilder = (typeof import('typebox'))['Type'];
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
  Type: TypeBoxBuilder,
  registeredToolNames: Set<string>,
  registerFn: RegisterFn,
): void {
  const querySchema = Type.Object(
    {
      url: Type.Optional(
        Type.String({ description: 'Absolute http(s) URL to fetch and read as text.' }),
      ),
      query: Type.Optional(
        Type.String({ description: 'Web search query (used when no url is given).' }),
      ),
      maxResults: Type.Optional(
        Type.Integer({ minimum: 1, maximum: 20, description: 'Search: max results (default 5).' }),
      ),
      maxChars: Type.Optional(
        Type.Integer({
          minimum: 500,
          maximum: 50000,
          description: 'Fetch: max characters of page text to return per page (default 15000).',
        }),
      ),
      page: Type.Optional(
        Type.Integer({
          minimum: 1,
          maximum: 20,
          description:
            'Fetch: page number for long documents (default 1). Each page is maxChars chars. Pass page: 2, 3\u2026 when the result shows truncated: true.',
        }),
      ),
      engine: Type.Optional(
        Type.String({
          description:
            'Search: force a provider \u2014 "tavily", "serper", "exa", or "duckduckgo" (default: auto by available key).',
        }),
      ),
      timeRange: Type.Optional(
        Type.String({
          description: 'Search: recency filter \u2014 "day", "week", "month", or "year".',
        }),
      ),
      includeDomains: Type.Optional(
        Type.Array(Type.String(), {
          description: 'Search (Tavily): allowlist domains, e.g. ["docs.python.org"].',
        }),
      ),
      excludeDomains: Type.Optional(
        Type.Array(Type.String(), {
          description: 'Search (Tavily): blocklist domains to drop noise.',
        }),
      ),
      exaType: Type.Optional(
        Type.String({
          description:
            'Search (Exa): result type \u2014 "auto" (default), "neural", or "keyword". "neural" for semantic/AI-native queries; "keyword" for exact-match.',
        }),
      ),
      exaCategory: Type.Optional(
        Type.String({
          description:
            'Search (Exa): category filter \u2014 "research paper", "news", "github", "company", "pdf". Narrows Exa results to a specific content type.',
        }),
      ),
    },
    { additionalProperties: false },
  ) as TSchema;

    const parameters = buildQueryEnvelopeSchema(Type, querySchema, {
      reasoningDescription: 'Concise reason this web fetch or search is necessary.',
      allowParallel: true,
  });

  registerFn(pi, registeredToolNames, {
    name: 'web',
    label: 'Web',
    description:
        'Browse the live web. Pass one or more queries[], each with reasoning plus either `url` (fetch page as text) or `query` (web search). ' +
        'Use queryRunType:"parallel" for independent reads; sequential remains the default. ' +
      'Search returns ranked {title, url, snippet} results plus an AI answer when available. ' +
      'Search uses the best configured provider (Tavily \u2192 Serper \u2192 Exa \u2192 DuckDuckGo); set a key in ~/.octocode/.env to upgrade. Use engine:"exa" for AI-native neural/academic search. ' +
      'Use for docs, changelogs, error messages, and current info beyond the codebase and training data.',
    promptSnippet: 'Search the web or fetch and read a page',
    promptGuidelines: [
      'Prefer Octocode/local tools for code and packages; use web for external docs, news, and live info. ' +
        'Search with `query` to discover, then read the best hit with `url`.',
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
