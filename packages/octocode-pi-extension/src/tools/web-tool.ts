/**
 * Web tool — Pi tool wrapper around runWebTool from src/web.ts.
 * One tool for both web search and page fetch, no API key required.
 * SSRF-hardened: private/loopback/link-local/metadata IPs blocked.
 */
import { runWebTool, renderWebResult } from '../web.js';
import type { ToolDefinition, PiTheme, ToolCallResult } from '../types.js';
import type { registerUniqueTool } from './octocode-tools.js';
import { makeRenderer, truncateToWidth } from './render-helpers.js';

type TypeBoxBuilder = (typeof import('typebox'))['Type'];
type RegisterFn = typeof registerUniqueTool;

export function registerWebTool(
  pi: { registerTool?(def: ToolDefinition): void },
  Type: TypeBoxBuilder,
  registeredToolNames: Set<string>,
  registerFn: RegisterFn,
): void {
  registerFn(pi, registeredToolNames, {
    name: 'web',
    label: 'Web',
    description:
      'Browse the live web. Pass `url` to fetch and read a page as clean text (like visiting it), ' +
      'or `query` to run a web search and get ranked {title, url, snippet} results (plus an AI answer when available). ' +
      'Search uses the best configured provider (Tavily → Serper → DuckDuckGo); set a key in ~/.octocode/.env to upgrade. ' +
      'Use for docs, changelogs, error messages, and current info beyond the codebase and training data. ' +
      'One of `url` or `query` is required.',
    promptSnippet: 'Search the web or fetch and read a page',
    promptGuidelines: [
      'Prefer Octocode/local tools for code and packages; use web for external docs, news, and live info. ' +
        'Search with `query` to discover, then read the best hit with `url`.',
    ],
    parameters: Type.Object({
      url: Type.Optional(
        Type.String({ description: 'Absolute http(s) URL to fetch and read as text.' }),
      ),
      query: Type.Optional(
        Type.String({ description: 'Web search query (used when no url is given).' }),
      ),
      maxResults: Type.Optional(
        Type.Integer({
          minimum: 1,
          maximum: 20,
          description: 'Search: max results (default 5).',
        }),
      ),
      maxChars: Type.Optional(
        Type.Integer({
          minimum: 500,
          maximum: 50000,
          description:
            'Fetch: max characters of page text to return per page (default 15000).',
        }),
      ),
      page: Type.Optional(
        Type.Integer({
          minimum: 1,
          maximum: 20,
          description:
            'Fetch: page number for long documents (default 1). Each page is maxChars chars. Pass page: 2, 3… when the result shows truncated: true.',
        }),
      ),
      engine: Type.Optional(
        Type.String({
          description:
            'Search: force a provider — "tavily", "serper", or "duckduckgo" (default: auto by available key).',
        }),
      ),
      timeRange: Type.Optional(
        Type.String({
          description: 'Search: recency filter — "day", "week", "month", or "year".',
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
    }),

    async execute(
      _toolCallId: string,
      params: Record<string, unknown>,
      signal?: AbortSignal,
    ) {
      const out = await runWebTool(
        params as Parameters<typeof runWebTool>[0],
        { signal },
      );
      const errorMsg = (out as { error?: string }).error;
      if (errorMsg) {
        // Throw so Pi sets isError:true in the session and the LLM sees tool failure.
        // Returning isError:true in the result object has no effect per Pi docs:
        // "Returning a value never sets the error flag regardless of what properties
        // you include in the return object." (extensions.md)
        throw new Error(errorMsg);
      }
      return {
        content: [{ type: 'text' as const, text: renderWebResult(out) }],
        details: out,
      };
    },

    renderCall(args: unknown, theme?: PiTheme) {
      const a = (args ?? {}) as Record<string, unknown>;
      const url = typeof a.url === 'string' && a.url ? a.url : '';
      const query = typeof a.query === 'string' && a.query ? a.query : '';
      const nameStr = theme?.fg('toolTitle', theme.bold('web')) ?? 'web';
      const detail = url
        ? (theme?.fg('accent', url.length > 70 ? url.slice(0, 67) + '…' : url) ?? url)
        : query
        ? (theme?.fg('dim', `"${query.length > 70 ? query.slice(0, 67) + '…' : query}"`) ?? `"${query}"`)
        : '';
      const rawLine = detail ? `${nameStr} ${detail}` : nameStr;
      return makeRenderer((w) => [truncateToWidth(rawLine, w)]);
    },

    renderResult(result: ToolCallResult, opts: { expanded?: boolean; isPartial?: boolean }, theme?: PiTheme) {
      if (opts.isPartial) {
        const msg = theme?.fg('warning', 'Fetching…') ?? 'Fetching…';
        return makeRenderer((w) => [truncateToWidth(msg, w)]);
      }
      const ok = !result.isError;
      const icon = theme?.fg(ok ? 'success' : 'error', ok ? '✓' : '✗') ?? (ok ? '✓' : '✗');
      const nameStr = theme?.fg('toolTitle', 'web') ?? 'web';
      // Extract meaningful stats from details
      const det = result.details as Record<string, unknown> | null;
      let stat = '';
      if (Array.isArray((det as Record<string, unknown> | null)?.results)) {
        const n = ((det as Record<string, unknown>).results as unknown[]).length;
        stat = theme?.fg('dim', ` · ${n} result${n === 1 ? '' : 's'}`) ?? ` · ${n} results`;
      } else if (det?.url) {
        const truncated = det.truncated === true;
        const pg = typeof det.page === 'number' && det.page > 1 ? ` p${det.page}` : '';
        stat = truncated
          ? (theme?.fg('dim', ` · page${pg} (more pages available)`) ?? ` · page${pg} (more)`)
          : (theme?.fg('dim', ` · page${pg}`) ?? ` · page${pg}`);
      }
      const header = `${icon} ${nameStr}${stat}`;
      if (!opts.expanded) {
        const hint = theme?.fg('dim', ' · expand for full output') ?? ' · expand for full output';
        return makeRenderer((w) => [truncateToWidth(`${header}${hint}`, w)]);
      }
      const text = (result.content as Array<{ type: string; text: string }>)
        ?.find?.((p) => p.type === 'text')?.text ?? '';
      const allLines = text.split('\n');
      const lines = allLines.slice(0, 20);
      const omitted = allLines.length - lines.length;
      return makeRenderer((w) => [
        truncateToWidth(header, w),
        ...lines.map((l) => truncateToWidth(theme?.fg('dim', l) ?? l, w)),
        ...(omitted > 0
          ? [truncateToWidth(theme?.fg('muted', `… ${omitted} more lines`) ?? `… ${omitted} more lines`, w)]
          : []),
      ]);
    },
  } satisfies ToolDefinition);
}
