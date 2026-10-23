import { DEFAULT_MAX_BYTES, DEFAULT_MAX_LINES, formatSize, truncateHead, type ExtensionAPI, type ExtensionContext } from '@earendil-works/pi-coding-agent';
import { Type } from 'typebox';
import { plural, resultBlock, resultText, spillPath, toolHeader } from '../shared/render.js';
import { timingOf, timedTool } from '../shared/tool-timing.js';
import { visibleControls } from '../shared/sanitize.js';
import { visibleError } from '../shared/visible-error.js';
import { boundImagePart, capOutputToFile, saveFullOutput, saveImageFile } from '../shared/spill.js';
import { imageLimits } from '../shared/vision.js';
import { shortPath } from '../shared/format.js';
import { capChars, cutText, errorMessage, textResult } from '../shared/util.js';
import type { Permission } from '../permission/ask.js';
import { actionWhy } from '../permission/mode.js';
import { pageFetcher, unreadReason, type FetchedImage } from './fetch.js';
import { classifyHost } from './guard.js';
import { savedPageLead, savedPageTools } from './leads.js';
import { collapseLinkRuns } from './html.js';
import { search } from './search.js';

/** Default and largest page text a fetch returns; `maxChars` picks a size in between. */
const DEFAULT_FETCH_CHARS = 20_000;
const MAX_FETCH_CHARS = 50_000;

/** What the result row summarizes: the page size, or the search provider and hit count. */
type WebDetails = { kind: 'fetch'; bytes: number; lines: number; file?: string } | { kind: 'image'; mimeType: string; bytes: number; file?: string } | { kind: 'notice'; reason: 'redirect' | 'unsupported' } | { kind: 'search'; provider: string; results: number };

/** The `⎿` line: `Fetched 12 KB · 240 lines` or `8 results · duckduckgo`. */
export function webSummary(details: unknown): string | undefined {
  if (!details || typeof details !== 'object') return undefined;
  const value = details as Partial<WebDetails>;
  if (value.kind === 'fetch' && typeof value.bytes === 'number' && typeof value.lines === 'number') return `Fetched ${formatSize(value.bytes)} · ${plural(value.lines, 'line')}`;
  if (value.kind === 'image' && typeof value.bytes === 'number') return `Fetched ${value.mimeType ?? 'image'} · ${formatSize(value.bytes)}`;
  if (value.kind === 'notice') return value.reason === 'redirect' ? 'Not read: redirects to another host (not followed)' : 'Not read: not a text document';
  if (value.kind === 'search' && typeof value.results === 'number') return `${value.results === 0 ? 'No results' : plural(value.results, 'result')} · ${value.provider ?? 'search'}`;
  return undefined;
}

/**
 * A fetched image as a tool result: a line naming the URL, type, original size and the saved original, then the image
 * bounded to the model's limit (`boundImagePart`: resized with a dimension note when over budget, a file pointer when it
 * cannot be shown).
 */
async function imageResult(image: FetchedImage, model: ExtensionContext['model']) {
  const data = Buffer.from(image.bytes).toString('base64');
  const file = saveImageFile(data, image.mimeType, 'web image');
  const parts = await boundImagePart({ type: 'image' as const, data, mimeType: image.mimeType }, 'web image', { limits: imageLimits(model), ...(file ? { savedAs: file } : {}) });
  const line = `${image.url}: ${image.mimeType} image, ${formatSize(image.bytes.byteLength)}. ${file ? `Original saved to ${file}.` : 'The original could not be saved.'}`;
  const details: WebDetails = { kind: 'image', mimeType: image.mimeType, bytes: image.bytes.byteLength, ...(file ? { file } : {}) };
  return { content: [{ type: 'text' as const, text: visibleControls(line) }, ...parts], details };
}

/**
 * The inline part of a page whose whole text is already saved: its first `max` characters, fewer when they would pass
 * Pi's output budget (a multi-byte page is far more bytes than characters), then the note and its `hint`. Sized here so
 * the result fits and the output cap never saves this preview to a second file next to the page's own.
 */
function savedPreview(text: string, max: number, hint: string): string {
  const head = cutText(text, max, { chars: true });
  const budget = { maxBytes: DEFAULT_MAX_BYTES - Buffer.byteLength(hint) - 256, maxLines: DEFAULT_MAX_LINES - 4 };
  const fitted = truncateHead(head, budget);
  const kept = fitted.truncated ? fitted.content || cutText(head, budget.maxBytes) : head;
  return capChars(text, kept.length, hint);
}

/** How to reach the rest of a page longer than `max` characters when it could not be saved to a file. */
function unsavedHint(length: number, max: number): string {
  const unsaved = 'the whole page could not be saved to a file';
  if (length <= MAX_FETCH_CHARS) return `${unsaved}; call web again with maxChars ${length} for the rest`;
  const larger = max < MAX_FETCH_CHARS ? `; maxChars ${MAX_FETCH_CHARS} shows up to ${MAX_FETCH_CHARS}` : '';
  return `${unsaved}${larger}; the last ${length - MAX_FETCH_CHARS} characters are beyond web's ${MAX_FETCH_CHARS}-character limit: download the page with bash (curl -o) to read them`;
}

/**
 * Asks for the URL the agent chose: `web` for a public host (allowed per host for the session), `net.private` for a
 * private or local one (one prompt that also names the web reason). Resolves with whether the first request may reach
 * a private address; throws the refusal when the user said no or no one could be asked.
 */
async function approveFetch(permission: Permission, ctx: ExtensionContext, toolCallId: string, url: string, signal: AbortSignal | undefined): Promise<{ private: boolean }> {
  let target: URL;
  try {
    target = new URL(url);
  } catch {
    return { private: false };
  }
  const privateHost = (await classifyHost(target)).kind === 'private';
  const grant = { key: target.host, label: target.host };
  const verdict = await permission.check(ctx, {
    action: privateHost ? 'net.private' : 'web',
    title: privateHost ? 'Fetch a private address?' : 'Fetch this page?',
    what: [url],
    ...(privateHost ? { reason: actionWhy('web') } : {}),
    grant,
    toolCallId,
    cwd: ctx.cwd,
    ...(signal ? { signal } : {}),
  });
  if (!verdict.allowed) throw new Error(verdict.message);
  return { private: privateHost };
}

export function registerWebTool(pi: ExtensionAPI, permission: Permission): void {
  const fetchUrl = pageFetcher();
  pi.registerTool(timedTool({
    name: 'web',
    label: 'Web',
    description:
      'Search the public web with query, or read one http(s) URL with url; supply exactly one. Fetches extract static page text and metadata or format JSON; they do not run JavaScript. A PNG, JPEG, GIF or WebP URL returns the image itself (resized to fit when large; the original is saved and its path named); SVG is read as its XML source. Cross-host redirects name the next URL instead of following it. Successful page fetches are cached for 15 minutes (images are not). Large results name a saved file.',
    promptSnippet: 'Read a public URL or search the web',
    promptGuidelines: [
      'Use web for public sources; for GitHub repositories and packages, prefer a dedicated research tool when one is active. Read the source behind a search snippet before relying on its claim.',
      'Cite source URLs. When static text cannot answer the question, inspect the rendered page with `browser`, or delegate to a browser-capable profile when `browser` is not active; read saved output by relevant ranges rather than fetching it again.',
    ],
    parameters: Type.Object({
      url: Type.Optional(Type.String({ description: 'Public http(s) page to read; omit query' })),
      query: Type.Optional(Type.String({ description: 'Search terms; omit url' })),
      maxResults: Type.Optional(Type.Integer({ description: 'Search result limit (default 8, up to 20)' })),
      maxChars: Type.Optional(Type.Integer({ description: `Fetch preview size (default ${DEFAULT_FETCH_CHARS}, between 1,000 and ${MAX_FETCH_CHARS.toLocaleString('en-US')}); longer fetched text is saved to a file` })),
    }),
    renderCall(args, theme, context) {
      return toolHeader(theme, context, 'Web', args.url ? args.url : args.query ? `search "${args.query}"` : '');
    },
    renderResult(result, _options, theme, context) {
      const text = resultText(result);
      const details = result.details as WebDetails | undefined;
      const summary = context.isError ? text : (webSummary(details) ?? text);
      const file = details?.kind === 'fetch' || details?.kind === 'image' ? details.file : undefined;
      // Errors are one message: its first line is the summary, the rest the body.
      const body = context.isError ? text.split('\n').slice(1).join('\n') : text;
      return resultBlock(theme, context, { summary, body, spill: file ?? spillPath(text), ...timingOf(result.details) });
    },
    async execute(id, params, signal, _onUpdate, ctx) {
      try {
        signal?.throwIfAborted();
        const url = params.url?.trim();
        const query = params.query?.trim();
        if (url && query) throw new Error('Provide exactly one of url or query.');
        if (url) {
          const max = Math.min(Math.max(Math.floor(params.maxChars ?? DEFAULT_FETCH_CHARS), 1_000), MAX_FETCH_CHARS);
          // Page text is untrusted: terminal escapes, bidi and invisible characters become visible escapes (nothing is dropped) before the model sees it.
          const approved = await approveFetch(permission, ctx, id, url, signal);
          const fetched = await fetchUrl(url, signal, approved);
          if (typeof fetched !== 'string') return imageResult(fetched, ctx.model);
          const text = visibleControls(fetched);
          const reason = unreadReason(text);
          if (reason) return textResult<WebDetails>(text, { kind: 'notice', reason });
          const lines = text.split('\n').length;
          const size = { kind: 'fetch' as const, bytes: Buffer.byteLength(text), lines };
          if (text.length <= max) return textResult<WebDetails>(capOutputToFile(text, { label: 'web' }), size);
          // A longer page is saved whole (read the rest from the file, no refetch); the inline part folds link lists
          // (sidebars, tables of contents) so content comes first.
          const file = saveFullOutput(text, 'web');
          if (file) {
            // Octocode tools take an absolute path; without them the short form is enough to read.
            const tools = savedPageTools(pi.getActiveTools());
            const where = tools.clasify || tools.localSearch || tools.localFetch ? file : shortPath(file);
            const hint = `the whole page (${lines} lines, links included) is in ${where}: ${savedPageLead(tools)}`;
            return textResult<WebDetails>(savedPreview(collapseLinkRuns(text), max, hint), { ...size, file });
          }
          // Nothing holds the rest, so nothing is folded and the note says exactly what stays out of reach.
          return textResult<WebDetails>(capOutputToFile(capChars(text, max, unsavedHint(text.length, max)), { label: 'web' }), size);
        }
        if (!query) throw new Error('Provide url or query.');
        try {
          const { provider, hits, failures } = await search(query, Math.min(Math.max(Math.floor(params.maxResults ?? 8), 1), 20), process.env, signal);
          const details: WebDetails = { kind: 'search', provider, results: hits.length };
          // Which provider answered matters only when others failed first: results may differ in kind and quality.
          const via = failures.length > 0 ? `\n\n(via ${provider}; skipped ${failures.join('; ')})` : '';
          if (hits.length === 0) return textResult(visibleControls(`No results (${provider}).${failures.length > 0 ? ` (skipped ${failures.join('; ')})` : ''}`), details);
          return textResult(visibleControls(hits.map((hit, index) => `${index + 1}. ${hit.title}\n   ${hit.url}\n   ${hit.snippet}`).join('\n\n') + via), details);
        } catch (error) {
          throw new Error(`Search failed: ${errorMessage(error)}`);
        }
      } catch (error) {
        // Page text, URLs and provider answers reach the model in errors too.
        throw visibleError(error);
      }
    },
  }));
}
