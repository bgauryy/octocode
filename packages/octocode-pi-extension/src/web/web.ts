import { formatSize, type ExtensionAPI } from '@earendil-works/pi-coding-agent';
import { Type } from 'typebox';
import { timingOf, plural, resultBlock, resultText, spillPath, timedTool, toolHeader } from '../shared/render.js';
import { sanitizeTerminalText } from '../shared/sanitize.js';
import { capOutputToFile, saveFullOutput } from '../shared/spill.js';
import { shortPath } from '../shared/format.js';
import { capChars, errorMessage, textResult } from '../shared/util.js';
import { fetchUrl, unreadReason } from './fetch.js';
import { collapseLinkRuns } from './html.js';
import { search } from './search.js';

/** Default and largest page text a fetch returns; `maxChars` picks a size in between. */
const DEFAULT_FETCH_CHARS = 20_000;
const MAX_FETCH_CHARS = 50_000;

/** What the result row summarizes: the page size, or the search provider and hit count. */
type WebDetails = { kind: 'fetch'; bytes: number; lines: number; file?: string } | { kind: 'notice'; reason: 'redirect' | 'unsupported' } | { kind: 'search'; provider: string; results: number };

/** The `⎿` line: `Fetched 12 KB · 240 lines` or `8 results · duckduckgo`. */
export function webSummary(details: unknown): string | undefined {
  if (!details || typeof details !== 'object') return undefined;
  const value = details as Partial<WebDetails>;
  if (value.kind === 'fetch' && typeof value.bytes === 'number' && typeof value.lines === 'number') return `Fetched ${formatSize(value.bytes)} · ${plural(value.lines, 'line')}`;
  if (value.kind === 'notice') return value.reason === 'redirect' ? 'Not read: redirects to another host (not followed)' : 'Not read: not a text document';
  if (value.kind === 'search' && typeof value.results === 'number') return `${value.results === 0 ? 'No results' : plural(value.results, 'result')} · ${value.provider ?? 'search'}`;
  return undefined;
}

export function registerWebTool(pi: ExtensionAPI): void {
  pi.registerTool(timedTool({
    name: 'web',
    label: 'Web',
    description:
      'Search the public web with query, or read one http(s) URL with url; supply exactly one. Fetches extract static page text and metadata or format JSON; they do not run JavaScript. Cross-host redirects name the next URL instead of following it. Successful fetches are cached for 15 minutes. Large results name a saved file. Use browser, when available, for rendered pages, interaction or login.',
    promptSnippet: 'Fetch a URL as text or search the web',
    promptGuidelines: [
      'Use web for public sources; prefer active GitHub or package registry research tools for their repositories and packages. Read the source behind a search snippet before relying on its claim.',
      'Cite source URLs. When static text cannot answer the question, inspect the rendered page with browser when it is available; read saved output by relevant ranges rather than fetching it again.',
    ],
    parameters: Type.Object({
      url: Type.Optional(Type.String({ minLength: 1, description: 'Public http(s) page to read; omit query' })),
      query: Type.Optional(Type.String({ minLength: 1, description: 'Search terms; omit url' })),
      maxResults: Type.Optional(Type.Integer({ minimum: 1, maximum: 20, description: 'Search result limit (default 8)' })),
      maxChars: Type.Optional(Type.Integer({ minimum: 1_000, maximum: MAX_FETCH_CHARS, description: `Fetch preview size (default ${DEFAULT_FETCH_CHARS}); longer fetched text is saved to a file` })),
    }),
    renderCall(args, theme, context) {
      return toolHeader(theme, context, 'Web', args.url ? args.url : args.query ? `search "${args.query}"` : '');
    },
    renderResult(result, _options, theme, context) {
      const text = resultText(result);
      const details = result.details as WebDetails | undefined;
      const summary = context.isError ? text : (webSummary(details) ?? text);
      const file = details?.kind === 'fetch' ? details.file : undefined;
      // Errors are one message: its first line is the summary, the rest the body.
      const body = context.isError ? text.split('\n').slice(1).join('\n') : text;
      return resultBlock(theme, context, { summary, body, spill: file ?? spillPath(text), ...timingOf(result.details) });
    },
    async execute(_id, params, signal) {
      signal?.throwIfAborted();
      const url = params.url?.trim();
      const query = params.query?.trim();
      if (url && query) throw new Error('Provide exactly one of url or query.');
      if (url) {
        const max = Math.min(Math.max(Math.floor(params.maxChars ?? DEFAULT_FETCH_CHARS), 1_000), MAX_FETCH_CHARS);
        // Page text is untrusted: strip terminal escapes and bidi controls before the model sees it.
        const text = sanitizeTerminalText(await fetchUrl(url, signal));
        const reason = unreadReason(text);
        if (reason) return textResult<WebDetails>(text, { kind: 'notice', reason });
        const lines = text.split('\n').length;
        const size = { kind: 'fetch' as const, bytes: Buffer.byteLength(text), lines };
        if (text.length <= max) return textResult<WebDetails>(capOutputToFile(text, { label: 'web' }), size);
        // A longer page is saved whole (read the rest from the file, no refetch); the inline part folds link lists
        // (sidebars, tables of contents) so content comes first.
        const file = saveFullOutput(text, 'web');
        const hint = file ? `the whole page (${lines} lines, links included) is in ${shortPath(file)}: search it or read it by line range` : `call web again with a larger maxChars (up to ${MAX_FETCH_CHARS})`;
        return textResult<WebDetails>(capOutputToFile(capChars(collapseLinkRuns(text), max, hint), { label: 'web' }), { ...size, ...(file ? { file } : {}) });
      }
      if (!query) throw new Error('Provide url or query.');
      try {
        const { provider, hits, failures } = await search(query, Math.min(Math.max(Math.floor(params.maxResults ?? 8), 1), 20), process.env, signal);
        const details: WebDetails = { kind: 'search', provider, results: hits.length };
        // Which provider answered matters only when others failed first: results may differ in kind and quality.
        const via = failures.length > 0 ? `\n\n(via ${provider}; skipped ${failures.join('; ')})` : '';
        if (hits.length === 0) return textResult(sanitizeTerminalText(`No results (${provider}).${failures.length > 0 ? ` (skipped ${failures.join('; ')})` : ''}`), details);
        return textResult(sanitizeTerminalText(hits.map((hit, index) => `${index + 1}. ${hit.title}\n   ${hit.url}\n   ${hit.snippet}`).join('\n\n') + via), details);
      } catch (error) {
        throw new Error(`Search failed: ${errorMessage(error)}`);
      }
    },
  }));
}
