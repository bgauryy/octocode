/**
 * Web tool — Pi tool wrapper around runWebTool from src/web.ts.
 * One tool for both web search and page fetch, no API key required.
 * SSRF-hardened: private/loopback/link-local/metadata IPs blocked.
 */
import { runWebTool, renderWebResult } from '../web.js';
import type { ToolDefinition } from '../types.js';
import type { registerUniqueTool } from './octocode-tools.js';

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
      return {
        content: [{ type: 'text' as const, text: renderWebResult(out) }],
        isError: Boolean((out as { error?: string }).error),
        details: out,
      };
    },
  } satisfies ToolDefinition);
}
