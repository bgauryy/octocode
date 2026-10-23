import { setTimeout as sleep } from 'node:timers/promises';
import { CrossHostRedirect, discardBody, NetworkError, publicFetch, type PublicFetchInit } from './guard.js';
import { formatSize } from '@earendil-works/pi-coding-agent';
import { untilAborted } from '../shared/util.js';
import { sniffImageType, type InlineImageType } from '../shared/vision.js';
import { describeHtml, RENDER_ADVICE } from './html.js';

export const USER_AGENT = 'Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0 Safari/537.36';
const TIMEOUT_MS = 30_000;
const MAX_BODY_BYTES = 5 * 1024 * 1024;
const MAX_BODY_MB = MAX_BODY_BYTES / 1024 / 1024;
/** Transient statuses a fetch retries, how often, the first backoff, and the longest Retry-After worth waiting for. */
const RETRY_STATUSES = new Set([408, 425, 429, 500, 502, 503, 504]);
const RETRIES = 2;
const RETRY_BASE_MS = 400;
const RETRY_AFTER_MAX_MS = 5_000;
/**
 * A fetched page is reused for this long (parallel agents often read the same pages), up to this many pages and bytes in all.
 * Only text is cached: an image is megabytes of bytes and the caller saves it, so a cached copy would point at a saved
 * file the session sweep may have removed. Parallel fetches of one image still share a request.
 */
const CACHE_TTL_MS = 15 * 60_000;
const CACHE_MAX = 64;
const CACHE_MAX_BYTES = 32 * 1024 * 1024;

/** An image the server sent: its real format (by signature) and bytes, at most `MAX_BODY_BYTES`. */
export interface FetchedImage {
  url: string;
  mimeType: InlineImageType;
  bytes: Uint8Array;
}

/** What a URL holds: text (a page, JSON, a notice) or an image. */
export type Fetched = string | FetchedImage;

/** The system's locale (Node follows LC_ALL / LANG), e.g. `en-US`. */
const systemLocale = (): string => Intl.DateTimeFormat().resolvedOptions().locale;

/**
 * Accept-Language the way a browser on this machine asks: the system's language first, English as the fallback
 * (`he-IL` → `he-IL,he;q=0.9,en;q=0.8`, `en-US` → `en-US,en;q=0.9`). A user who wants pages in another language
 * starts Pi with that LANG.
 */
export function language(locale = systemLocale()): string {
  const base = locale.split('-')[0]!.toLowerCase();
  const tags = [locale, ...(base !== locale ? [base] : []), ...(base !== 'en' ? ['en'] : [])];
  return tags.map((tag, index) => (index === 0 ? tag : `${tag};q=${(1 - index / 10).toFixed(1)}`)).join(',');
}

const REDIRECT_NOTE = 'The target is server-supplied and not verified; fetch it with web if you trust it.';
const UNREAD_NOTE = '; web cannot read it. Download it with bash (curl -o) if you need the file.';
/** Content types an image is read from; SVG is not one: it is XML text, shown as its source and never rendered. */
const INLINE_IMAGE_HEADERS = new Set(['image/png', 'image/jpeg', 'image/jpg', 'image/gif', 'image/webp']);
const mediaType = (contentType: string) => contentType.split(';')[0]!.trim().toLowerCase();

/** Why a fetch result is a notice rather than page text (a cross-host redirect, a binary type), or undefined for a page. */
export function unreadReason(text: string): 'redirect' | 'unsupported' | undefined {
  const lines = text.split('\n');
  if (lines.length === 2 && lines[1] === REDIRECT_NOTE) return 'redirect';
  if (lines.length === 1 && text.endsWith(UNREAD_NOTE)) return 'unsupported';
  return undefined;
}

/**
 * A URL as text: HTML reduced to its readable content with a note when a browser may show something else, JSON
 * pretty-printed, other text as is; or, for a PNG, JPEG, GIF or WebP, the image itself. Transient failures are retried; a
 * successful page is cached for 15 minutes (an image is not); a redirect to another host is reported instead of followed.
 * `private`: the first request may reach a private address (the user approved this URL as a `net.private` ask).
 */
export type PageFetcher = (url: string, signal?: AbortSignal, options?: { private?: boolean }) => Promise<Fetched>;

/**
 * A fetcher that owns its page cache and its fetches under way: the web tool makes one per registration, so a reload starts
 * empty, and a test makes one per case. Callers asking for the same page share a request, which is aborted once every
 * caller has stopped waiting.
 */
export function pageFetcher(): PageFetcher {
  /** Least recently used first (a Map keeps insertion order; a hit re-inserts). */
  const cache = new Map<string, { at: number; text: string; bytes: number }>();
  let cachedBytes = 0;
  const inFlight = new Map<string, { promise: Promise<Fetched>; controller: AbortController; waiting: number }>();

  function uncache(key: string): void {
    cachedBytes -= cache.get(key)?.bytes ?? 0;
    cache.delete(key);
  }

  /** Fetches one URL and caches its text. */
  async function loadUrl(url: string, signal: AbortSignal, options: { private?: boolean }): Promise<Fetched> {
    let text: Fetched;
    try {
      text = await fetchResource(url, signal, options);
    } catch (error) {
      // The model decides whether a server-supplied target is worth fetching; it is not followed on the page's word.
      if (error instanceof CrossHostRedirect) return `${error.message}\n${REDIRECT_NOTE}`;
      throw error;
    }
    if (typeof text !== 'string') return text;
    uncache(url);
    const bytes = Buffer.byteLength(text);
    cache.set(url, { at: Date.now(), text, bytes });
    cachedBytes += bytes;
    while (cache.size > 1 && (cache.size > CACHE_MAX || cachedBytes > CACHE_MAX_BYTES)) uncache(cache.keys().next().value!);
    return text;
  }

  return async function fetchUrl(url, signal, options = {}) {
    signal?.throwIfAborted();
    const cached = cache.get(url);
    if (cached && Date.now() - cached.at < CACHE_TTL_MS) {
      cache.delete(url);
      cache.set(url, cached);
      return cached.text;
    }
    let entry = inFlight.get(url);
    if (!entry) {
      const controller = new AbortController();
      const promise = loadUrl(url, controller.signal, options).finally(() => {
        if (inFlight.get(url)?.promise === promise) inFlight.delete(url);
      });
      // Every caller may have stopped waiting; the abort rejection then has no other handler.
      promise.catch(() => undefined);
      entry = { promise, controller, waiting: 0 };
      inFlight.set(url, entry);
    }
    entry.waiting += 1;
    try {
      return await untilAborted(entry.promise, signal);
    } finally {
      entry.waiting -= 1;
      if (entry.waiting === 0 && signal?.aborted) entry.controller.abort(signal.reason);
    }
  };
}

/** Retry-After in milliseconds (seconds or an HTTP date); undefined when absent or unreadable. */
export function retryAfterMs(header: string | null, now = Date.now()): number | undefined {
  if (!header) return undefined;
  if (/^\d+$/.test(header.trim())) return Number(header.trim()) * 1000;
  const at = Date.parse(header);
  return Number.isNaN(at) ? undefined : Math.max(0, at - now);
}

const backoff = (attempt: number) => RETRY_BASE_MS * 2 ** attempt + Math.floor(Math.random() * RETRY_BASE_MS);

/** The network cause of a failed fetch (ECONNRESET, ENOTFOUND, a TLS error), or a timeout. */
function describeFetchError(error: unknown, url: string, tries: number): Error {
  if (error instanceof Error && (error.name === 'TimeoutError' || (error.name === 'AbortError' && /timeout/i.test(error.message)))) {
    return new Error(`Timed out after ${TIMEOUT_MS / 1000}s fetching ${url}`);
  }
  if (!(error instanceof NetworkError)) return error instanceof Error ? error : new Error(String(error));
  const cause = (error as { cause?: { code?: unknown; message?: unknown } }).cause;
  const detail = [cause?.code, cause?.message].filter((part) => typeof part === 'string' && part).join(': ');
  return new Error(`Could not fetch ${url}: ${detail || error.message}${tries > 1 ? ` (tried ${tries} times)` : ''}. The site may block automated requests; ${RENDER_ADVICE}.`);
}

/**
 * `publicFetch`, retrying network errors and transient statuses (408/425/429/5xx) with backoff and jitter, honouring a
 * short Retry-After; a longer one is returned for the caller to report.
 */
async function fetchWithRetry(url: string, init: PublicFetchInit, options: { private?: boolean }, signal?: AbortSignal): Promise<Response> {
  for (let attempt = 0; ; attempt += 1) {
    let response: Response;
    try {
      response = await publicFetch(url, { ...init, signal: combineSignals(signal) }, { sameHost: true, firstHopPrivate: options.private === true });
    } catch (error) {
      // Only network failures are worth another try: not a refusal, a redirect, a timeout or the user's abort.
      if (signal?.aborted || !(error instanceof NetworkError) || attempt >= RETRIES) throw describeFetchError(error, url, attempt + 1);
      await sleep(backoff(attempt), undefined, { signal });
      continue;
    }
    if (!RETRY_STATUSES.has(response.status) || attempt >= RETRIES) return response;
    const wait = retryAfterMs(response.headers.get('retry-after'));
    if (wait !== undefined && wait > RETRY_AFTER_MAX_MS) return response;
    await discardBody(response);
    await sleep(wait ?? backoff(attempt), undefined, { signal });
  }
}

function httpError(response: Response, url: string): Error {
  const after = response.headers.get('retry-after');
  const status = [response.status, response.statusText].filter(Boolean).join(' ');
  const hint = response.status === 401 || response.status === 403 ? ` (login or bot block: ${RENDER_ADVICE}, or use a dedicated MCP tool for private content)` : '';
  return new Error(`HTTP ${status} for ${url}${after ? ` (retry after ${after})` : ''}${hint}`);
}

/**
 * An image the server sent, read whole up to `MAX_BODY_BYTES`. A cut image cannot be decoded, so one over the cap is
 * refused with a notice (nothing is saved: the whole file is a `curl -o` away), and so is a body that is not the image
 * its header claims: sending it on as an image would make the provider reject the whole conversation.
 */
async function fetchImage(response: Response, url: string, type: string): Promise<Fetched> {
  const where = response.url || url;
  const declared = Number(response.headers.get('content-length'));
  if (declared > MAX_BODY_BYTES) {
    await discardBody(response);
    return `${where} is ${type}, ${formatSize(declared)}, over web's ${MAX_BODY_MB} MB limit${UNREAD_NOTE}`;
  }
  const { bytes, truncated } = await readBytesCapped(response).catch((error: unknown) => {
    throw describeFetchError(error, url, 1);
  });
  if (truncated) return `${where} is ${type}, over web's ${MAX_BODY_MB} MB limit${UNREAD_NOTE}`;
  const mimeType = sniffImageType(bytes);
  if (!mimeType) return `${where} is ${type} by its header, but the body is not a PNG, JPEG, GIF or WebP image${UNREAD_NOTE}`;
  return { url: where, mimeType, bytes };
}

async function fetchResource(url: string, signal: AbortSignal | undefined, options: { private?: boolean }): Promise<Fetched> {
  const response = await fetchWithRetry(url, { headers: { 'user-agent': USER_AGENT, accept: 'text/html,application/json,text/plain,*/*', 'accept-language': language() } }, options, signal);
  const type = response.headers.get('content-type') ?? '';
  if (INLINE_IMAGE_HEADERS.has(mediaType(type))) {
    if (response.ok) return fetchImage(response, url, mediaType(type));
    await discardBody(response);
    throw httpError(response, url);
  }
  if (isBinaryType(type)) {
    await discardBody(response);
    if (!response.ok) throw httpError(response, url);
    // Decoding binary as UTF-8 would flood the context with garbage.
    return `${response.url} is ${mediaType(type)}, not text${UNREAD_NOTE}`;
  }
  // The timeout covers the body too: a slow stream reports like a slow connect, not as a bare abort.
  const { text: body, truncated } = await readCapped(response, type).catch((error: unknown) => {
    throw describeFetchError(error, url, 1);
  });
  if (!response.ok) throw httpError(response, url);
  const json = type.includes('json');
  // First, so the note survives the web tool's character cap. Cut JSON does not parse, so it is shown raw.
  const cut = truncated ? `[Truncated: the response is over ${MAX_BODY_MB} MB; only the first ${MAX_BODY_MB} MB was read${json ? ' and is shown raw, not pretty-printed' : ''}. Download it with bash (curl -o) for the whole body.]\n\n` : '';
  if (type.includes('html')) return cut + describeHtml(body, response.url || url);
  if (json && !truncated) {
    try {
      return JSON.stringify(JSON.parse(body), null, 2);
    } catch {
      return body;
    }
  }
  return cut + body;
}

export function isBinaryType(contentType: string): boolean {
  const type = mediaType(contentType);
  if (!type) return false;
  if (type.startsWith('text/') || /[+/](json|xml|javascript|ecmascript|x-www-form-urlencoded|yaml|toml|x-sh)$/.test(type)) return false;
  return /^(image|audio|video|font)\//.test(type) || /^application\/(pdf|zip|gzip|x-tar|x-7z-compressed|octet-stream|wasm|x-bzip2|vnd\.)/.test(type);
}

/** The body decoded (see `decodeBody`), at most MAX_BODY_BYTES (5 MiB) of it; `truncated` when there was more. */
export async function readCapped(response: Response, contentType: string): Promise<{ text: string; truncated: boolean }> {
  const { bytes, truncated } = await readBytesCapped(response);
  return { text: decodeBody(bytes, contentType, truncated), truncated };
}

/** The body's bytes, at most MAX_BODY_BYTES (5 MiB) of them; `truncated` when there was more. */
async function readBytesCapped(response: Response): Promise<{ bytes: Buffer; truncated: boolean }> {
  const reader = response.body?.getReader();
  if (!reader) return { bytes: Buffer.alloc(0), truncated: false };
  const chunks: Uint8Array[] = [];
  let size = 0;
  let truncated = false;
  for (;;) {
    const { done, value } = await reader.read();
    if (done) break;
    chunks.push(value);
    size += value.byteLength;
    if (size > MAX_BODY_BYTES) {
      truncated = true;
      break;
    }
  }
  await reader.cancel().catch(() => undefined);
  return { bytes: Buffer.concat(chunks).subarray(0, MAX_BODY_BYTES), truncated };
}

/**
 * Text in the body's charset: a byte-order mark, else Content-Type's charset, else (HTML) a `<meta>` charset in the
 * first bytes, else UTF-8; an unknown label falls back to UTF-8. A cut body ends at the last whole character.
 */
export function decodeBody(bytes: Uint8Array, contentType: string, cut = false): string {
  const label = bomCharset(bytes) ?? charsetParameter(contentType) ?? (contentType.includes('html') ? metaCharset(bytes) : undefined);
  // Node's global TextDecoder is a value; its instance type is the decoder.
  let decoder: InstanceType<typeof TextDecoder>;
  try {
    decoder = new TextDecoder(label ?? 'utf-8');
  } catch {
    decoder = new TextDecoder('utf-8');
  }
  // Streaming keeps an incomplete trailing sequence back instead of turning it into U+FFFD.
  return decoder.decode(bytes, { stream: cut });
}

function bomCharset(bytes: Uint8Array): string | undefined {
  if (bytes[0] === 0xef && bytes[1] === 0xbb && bytes[2] === 0xbf) return 'utf-8';
  if (bytes[0] === 0xfe && bytes[1] === 0xff) return 'utf-16be';
  if (bytes[0] === 0xff && bytes[1] === 0xfe) return 'utf-16le';
  return undefined;
}

function charsetParameter(value: string): string | undefined {
  return /;\s*charset\s*=\s*["']?([^"';\s]+)/i.exec(value)?.[1];
}

/** Bytes an HTML charset declaration must appear in (the HTML spec's prescan window). */
const META_PRESCAN_BYTES = 1024;

/** `<meta charset=…>` or `<meta http-equiv=content-type content="…; charset=…">` early in the document. */
function metaCharset(bytes: Uint8Array): string | undefined {
  const head = Buffer.from(bytes.subarray(0, META_PRESCAN_BYTES)).toString('latin1');
  for (const [tag] of head.matchAll(/<meta\b[^>]*>/gi)) {
    const direct = /\bcharset\s*=\s*["']?([^"'\s/>;]+)/i.exec(tag)?.[1];
    if (direct) return direct;
  }
  return undefined;
}

export function combineSignals(signal?: AbortSignal): AbortSignal {
  const timeout = AbortSignal.timeout(TIMEOUT_MS);
  return signal ? AbortSignal.any([signal, timeout]) : timeout;
}
