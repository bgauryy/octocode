// Single "web" tool: lets the agent search the web and fetch/read pages like an agent.
// No API key, no npm deps (Node >=20 global fetch + node:dns + node:net).
//
// Security: web_fetch is an SSRF magnet. The guard (assertPublicUrl + per-hop
// re-validation in safeFetch) resolves every hostname and rejects any resolved IP in a
// private / loopback / link-local / ULA / CGNAT / IPv4-mapped range, blocking
// cloud-metadata (169.254.169.254) and localhost reach. Redirects are followed manually
// and each hop is re-validated. Residual: a pure DNS-rebinding race (public at check,
// private at connect) is not closed without connection pinning — documented follow-up.

import dns from 'node:dns/promises';
import net from 'node:net';

// A realistic mainstream-browser User-Agent so the agent reads pages like a browser.
// Override with OCTOCODE_WEB_USER_AGENT. API providers (Tavily/Serper) use their own
// auth headers and are unaffected by this.
export const DEFAULT_USER_AGENT =
  'Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/137.0.0.0 Safari/537.36';

// Client-hint headers Chrome always sends alongside its UA.
// Keep in sync with DEFAULT_USER_AGENT above.
export const DEFAULT_SEC_CH_UA =
  '"Chromium";v="137", "Google Chrome";v="137", "Not/A)Brand";v="24"';

export function resolveUserAgent(env?: Record<string, string | undefined>): string {
  const custom = env?.OCTOCODE_WEB_USER_AGENT;
  return custom && String(custom).trim() ? String(custom).trim() : DEFAULT_USER_AGENT;
}

// ─── SSRF guard ──────────────────────────────────────────────────────────────

/** [network base, mask bits] — security-relevant IPv4 ranges to block. */
const V4_BLOCKS: Array<[string, number]> = [
  ['0.0.0.0', 8],
  ['10.0.0.0', 8],
  ['100.64.0.0', 10],
  ['127.0.0.0', 8],
  ['169.254.0.0', 16],
  ['172.16.0.0', 12],
  ['192.0.0.0', 24],
  ['192.168.0.0', 16],
  ['198.18.0.0', 15],
];

function ipv4ToInt(ip: string): number | null {
  const parts = ip.split('.').map(Number);
  if (
    parts.length !== 4 ||
    parts.some((n) => Number.isNaN(n) || n < 0 || n > 255)
  )
    return null;
  return (
    (((parts[0]! << 24) >>> 0) +
      (parts[1]! << 16) +
      (parts[2]! << 8) +
      parts[3]!) >>>
    0
  );
}

function inV4(ip: string, base: string, maskBits: number): boolean {
  const i = ipv4ToInt(ip);
  const b = ipv4ToInt(base);
  if (i === null || b === null) return false;
  const mask = maskBits === 0 ? 0 : (0xffffffff << (32 - maskBits)) >>> 0;
  return (i & mask) === (b & mask);
}

/** Expand an IPv6 literal to its 8 16-bit groups, or null if unparseable. */
function expandIpv6(ip: string): number[] | null {
  let s = ip;
  // Fold a trailing dotted-quad (e.g. ::ffff:1.2.3.4) into two hextets.
  const dotted = s.match(/^(.*:)(\d+\.\d+\.\d+\.\d+)$/);
  if (dotted) {
    const v4 = ipv4ToInt(dotted[2]!);
    if (v4 === null) return null;
    s = `${dotted[1]}${((v4 >>> 16) & 0xffff).toString(16)}:${(v4 & 0xffff).toString(16)}`;
  }
  const halves = s.split('::');
  if (halves.length > 2) return null;
  const head = halves[0] ? halves[0].split(':') : [];
  const tail = halves.length === 2 && halves[1] ? halves[1].split(':') : [];
  let groups: string[];
  if (halves.length === 1) {
    groups = head;
  } else {
    const missing = 8 - head.length - tail.length;
    if (missing < 0) return null;
    groups = [...head, ...Array(missing).fill('0'), ...tail];
  }
  if (groups.length !== 8) return null;
  const nums = groups.map((h) => parseInt(h || '0', 16));
  return nums.some((n) => Number.isNaN(n) || n < 0 || n > 0xffff) ? null : nums;
}

/**
 * Extract the embedded IPv4 (dotted) from an IPv6 that carries one — IPv4-mapped
 * (::ffff:0:0/96), IPv4-compatible (::/96, deprecated), NAT64 (64:ff9b::/96), and
 * 6to4 (2002::/16). Returns null when the address carries no embedded IPv4.
 * These forms otherwise reach the loopback/metadata ranges past the SSRF guard.
 */
function embeddedIpv4(g: number[]): string | null {
  const asV4 = (a: number, b: number): string =>
    `${(a >>> 8) & 0xff}.${a & 0xff}.${(b >>> 8) & 0xff}.${b & 0xff}`;
  const zeroPrefix = g[0] === 0 && g[1] === 0 && g[2] === 0 && g[3] === 0 && g[4] === 0;
  if (zeroPrefix && (g[5] === 0xffff || g[5] === 0)) return asV4(g[6]!, g[7]!); // mapped / compatible
  if (g[0] === 0x64 && g[1] === 0xff9b && g[2] === 0 && g[3] === 0 && g[4] === 0 && g[5] === 0) {
    return asV4(g[6]!, g[7]!); // NAT64
  }
  if (g[0] === 0x2002) return asV4(g[1]!, g[2]!); // 6to4
  return null;
}

/** True if an IP literal is in a non-public range (or is not a valid IP → fail closed). */
export function isBlockedIp(ip: string): boolean {
  if (typeof ip !== 'string') return true;
  const version = net.isIP(ip);
  if (version === 4) return V4_BLOCKS.some(([base, bits]) => inV4(ip, base, bits));
  if (version === 6) {
    const lo = ip.toLowerCase();
    if (lo === '::1' || lo === '::') return true;
    const groups = expandIpv6(lo);
    if (groups) {
      const embedded = embeddedIpv4(groups);
      if (embedded) return isBlockedIp(embedded); // recurse into V4 ranges for any embedded form
    }
    if (/^fe[89ab]/.test(lo)) return true; // fe80::/10 link-local
    const first = groups ? groups[0]! : parseInt(lo.split(':')[0] ?? '0', 16);
    if ((first & 0xfe00) === 0xfc00) return true; // fc00::/7 ULA
    return false;
  }
  return true; // not a valid IP literal
}

type LookupFn = (
  hostname: string,
  opts: { all: true },
) => Promise<Array<{ address: string; family: number }> | { address: string; family: number }>;

/** Parse+validate a URL: http/https only, and every resolved IP must be public. */
export async function assertPublicUrl(
  urlStr: string,
  opts: { lookup?: LookupFn } = {},
): Promise<URL> {
  const lookup = opts.lookup ?? (dns.lookup as unknown as LookupFn);
  let url: URL;
  try {
    url = new URL(urlStr);
  } catch {
    throw new Error(`Invalid URL: ${urlStr}`);
  }
  if (url.protocol !== 'http:' && url.protocol !== 'https:') {
    throw new Error(`Blocked non-http(s) URL: ${url.protocol}`);
  }
  const hostname = url.hostname.replace(/^\[|\]$/g, '');
  if (net.isIP(hostname)) {
    if (isBlockedIp(hostname)) throw new Error(`Blocked private/loopback address: ${hostname}`);
    return url;
  }
  const records = await lookup(hostname, { all: true });
  const ips = Array.isArray(records) ? records : [records];
  if (ips.length === 0) throw new Error(`Could not resolve host: ${hostname}`);
  for (const rec of ips) {
    const addr = typeof rec === 'string' ? rec : rec.address;
    if (isBlockedIp(addr)) throw new Error(`Host ${hostname} resolves to blocked address ${addr}`);
  }
  return url;
}

// ─── Fetch with redirect validation ──────────────────────────────────────────

// `dispatcher` is a Node/undici fetch extension not present in the DOM RequestInit type.
type FetchImpl = (input: string, init?: RequestInit & { dispatcher?: unknown }) => Promise<Response>;

interface SafeFetchOptions {
  fetchImpl?: FetchImpl;
  lookup?: LookupFn;
  timeoutMs?: number;
  maxRedirects?: number;
  headers?: Record<string, string>;
  signal?: AbortSignal;
  env?: Record<string, string | undefined>;
}

export interface SafeFetchResult {
  res: Response;
  finalUrl: string;
}

/**
 * Lazily build an undici dispatcher whose connect-time DNS lookup validates every
 * resolved IP and pins the socket to a validated address. This closes the
 * DNS-rebinding TOCTOU: assertPublicUrl validates at check time, and the SAME
 * validation now runs at connect time on the exact IP the socket uses — so a host
 * that answers public-on-check / private-on-connect is rejected during connect.
 *
 * undici ships inside Node but is imported dynamically (no hard dependency): if it
 * is unavailable, we return null and fall back to per-hop re-validation only.
 */
let pinnedDispatcherPromise: Promise<unknown | null> | undefined;
function getPinnedDispatcher(): Promise<unknown | null> {
  if (!pinnedDispatcherPromise) {
    pinnedDispatcherPromise = (async () => {
      try {
        const undici = (await import('undici')) as {
          Agent: new (opts: unknown) => unknown;
        };
        return new undici.Agent({
          connect: {
            // net-style lookup: undici passes this straight to the socket connect.
            lookup: (
              hostname: string,
              _options: unknown,
              cb: (err: Error | null, address: string, family: number) => void,
            ): void => {
              // node:dns/promises API — resolve, validate every IP, pin to the first.
              dns.lookup(hostname, { all: true })
                .then((addresses) => {
                  const list = Array.isArray(addresses) ? addresses : [addresses];
                  if (list.length === 0) return cb(new Error(`Could not resolve host: ${hostname}`), '', 0);
                  for (const rec of list) {
                    if (isBlockedIp(rec.address)) {
                      return cb(new Error(`Blocked private/loopback address: ${rec.address}`), '', 0);
                    }
                  }
                  const chosen = list[0]!;
                  cb(null, chosen.address, chosen.family);
                })
                .catch((err: Error) => cb(err, '', 0));
            },
          },
        });
      } catch {
        return null;
      }
    })();
  }
  return pinnedDispatcherPromise;
}

/** Fetch with manual redirect following, re-validating every hop against the SSRF guard. */
export async function safeFetch(
  startUrl: string,
  opts: SafeFetchOptions = {},
): Promise<SafeFetchResult> {
  const {
    fetchImpl = globalThis.fetch as FetchImpl,
    lookup = dns.lookup as unknown as LookupFn,
    timeoutMs = 15000,
    maxRedirects = 5,
    headers = {},
    signal: externalSignal,
  } = opts;

  // Pin connections to validated IPs to close the DNS-rebinding TOCTOU — only on
  // the default global-fetch path (a custom fetchImpl, e.g. in tests, is untouched).
  const usingGlobalFetch = fetchImpl === (globalThis.fetch as FetchImpl);
  const dispatcher = usingGlobalFetch ? await getPinnedDispatcher() : null;

  let current = startUrl;
  for (let hop = 0; hop <= maxRedirects; hop++) {
    await assertPublicUrl(current, { lookup });
    const ac = new AbortController();
    if (externalSignal?.aborted) ac.abort(); // honor an already-aborted external signal
    const timer = setTimeout(() => ac.abort(), timeoutMs);
    const onAbort = (): void => ac.abort();
    if (externalSignal) externalSignal.addEventListener('abort', onAbort, { once: true });
    let res: Response;
    try {
      res = await fetchImpl(current, {
        redirect: 'manual',
        signal: ac.signal,
        ...(dispatcher ? { dispatcher } : {}),
        headers: {
          'user-agent': resolveUserAgent(opts.env),
          ...(opts.env?.OCTOCODE_WEB_USER_AGENT
            ? {}
            : {
                'sec-ch-ua': DEFAULT_SEC_CH_UA,
                'sec-ch-ua-mobile': '?0',
                'sec-ch-ua-platform': '"macOS"',
              }),
          'sec-fetch-site': 'none',
          'sec-fetch-mode': 'navigate',
          'sec-fetch-dest': 'document',
          accept:
            'text/html,application/xhtml+xml,application/xml;q=0.9,image/avif,image/webp,*/*;q=0.8',
          'accept-language': 'en-US,en;q=0.9',
          'accept-encoding': 'gzip, deflate, br',
          ...headers,
        },
      });
    } finally {
      clearTimeout(timer);
      if (externalSignal) externalSignal.removeEventListener('abort', onAbort);
    }
    const location =
      res.status >= 300 && res.status < 400 ? res.headers.get('location') : null;
    if (location) {
      current = new URL(location, current).toString();
      continue;
    }
    return { res, finalUrl: current };
  }
  throw new Error(`Too many redirects (>${maxRedirects})`);
}

// ─── Body reading ─────────────────────────────────────────────────────────────

interface ReadCappedResult {
  text: string;
  truncated: boolean;
}

type AbortFlag = { __aborted: true };

/**
 * Read a response body up to maxBytes, aborting the stream once the cap is hit
 * OR the optional `signal` fires. Without a signal a stalled body would hang.
 */
export async function readCapped(
  res: Response,
  maxBytes = 2_000_000,
  opts: { signal?: AbortSignal } = {},
): Promise<ReadCappedResult> {
  const { signal } = opts;
  if (!res.body || typeof res.body.getReader !== 'function') {
    const text = await res.text();
    return text.length > maxBytes
      ? { text: text.slice(0, maxBytes), truncated: true }
      : { text, truncated: false };
  }
  const reader = res.body.getReader();
  const decoder = new TextDecoder('utf-8', { fatal: false });
  const abortPromise: Promise<AbortFlag> | null = signal
    ? new Promise<AbortFlag>((resolve) => {
        if (signal.aborted) resolve({ __aborted: true });
        else signal.addEventListener('abort', () => resolve({ __aborted: true }), { once: true });
      })
    : null;
  let out = '';
  let total = 0;
  let truncated = false;
  for (;;) {
    const chunk = abortPromise
      ? await Promise.race([reader.read(), abortPromise])
      : await reader.read();
    if ((chunk as AbortFlag).__aborted) {
      truncated = true;
      try {
        await reader.cancel();
      } catch { /* ignore */ }
      break;
    }
    const { done, value } = chunk as ReadableStreamReadResult<Uint8Array>;
    if (done) break;
    if (value) {
      total += value.length;
      out += decoder.decode(value, { stream: true });
    }
    if (total >= maxBytes) {
      truncated = true;
      try {
        await reader.cancel();
      } catch { /* ignore */ }
      break;
    }
  }
  out += decoder.decode();
  return { text: out, truncated };
}

// ─── HTML utilities ───────────────────────────────────────────────────────────

const ENTITIES: Record<string, string> = {
  amp: '&',
  lt: '<',
  gt: '>',
  quot: '"',
  apos: "'",
  nbsp: ' ',
  '#39': "'",
  '#x27': "'",
};

/** Valid Unicode code point range; String.fromCodePoint throws (RangeError) outside it. */
function codePointOrEntity(cp: number, original: string): string {
  return Number.isInteger(cp) && cp >= 0 && cp <= 0x10ffff
    ? String.fromCodePoint(cp)
    : original; // out-of-range (e.g. &#999999999999;) → leave the raw entity, never throw
}

export function decodeEntities(str: string): string {
  return str
    .replace(/&#(\d+);/g, (m, d: string) => codePointOrEntity(Number(d), m))
    .replace(/&#x([0-9a-f]+);/gi, (m, h: string) => codePointOrEntity(parseInt(h, 16), m))
    .replace(
      /&([a-z]+|#x?\w+);/gi,
      (m, name: string) => ENTITIES[name] ?? ENTITIES[name.toLowerCase()] ?? m,
    );
}

export function extractTitle(html: string): string {
  const m = html.match(/<title[^>]*>([\s\S]*?)<\/title>/i);
  return m?.[1] ? decodeEntities(m[1]).replace(/\s+/g, ' ').trim() : '';
}

/** Strip a page to readable plain text: drop script/style/nav chrome, tags, collapse whitespace. */
export function htmlToText(html: string): string {
  const text = html
    .replace(/<!--[\s\S]*?-->/g, ' ')
    .replace(
      /<(script|style|noscript|template|svg|nav|aside|footer)([\s>][\s\S]*?)<\/\1>/gi,
      ' ',
    )
    .replace(/<([a-z][a-z0-9]*(?:-[a-z0-9]+)+)[\s\S]*?<\/\1>/gi, ' ')
    .replace(/<(\w+)[^>]+\baria-label="announcement"[^>]*>[\s\S]*?<\/\1>/gi, ' ')
    .replace(
      /<(\w+)[^>]+\btypeof="BreadcrumbList"[^>]*>[\s\S]*?<\/\1>/gi,
      ' ',
    )
    .replace(/<a[^>]+href="#[^"]*"[^>]*>\s*Skip[^<]*<\/a\s*>/gi, ' ')
    .replace(/<\/(p|div|section|article|h[1-6]|li|tr|br|header)>/gi, '\n')
    .replace(/<br\s*\/?>/gi, '\n')
    .replace(/<[^>]+>/g, ' ');
  return decodeEntities(text)
    .replace(/[ \t\f\v]+/g, ' ')
    .replace(/\n\s*\n\s*\n+/g, '\n\n')
    .split('\n')
    .map((l) => l.trim())
    .join('\n')
    .trim();
}

// ─── Deadline helper ──────────────────────────────────────────────────────────

interface DeadlineOptions {
  timeoutMs?: number;
  signal?: AbortSignal;
}

export interface Deadline {
  signal: AbortSignal;
  cleanup(): void;
}

/**
 * A single deadline (timeout + optional external signal) covering an entire
 * fetch op — headers AND body — so a stalled body stream can't hang.
 */
export function createDeadline(opts: DeadlineOptions = {}): Deadline {
  const ac = new AbortController();
  const timer = setTimeout(() => ac.abort(), opts.timeoutMs ?? 15000);
  const onExt = (): void => ac.abort();
  if (opts.signal) {
    if (opts.signal.aborted) ac.abort();
    else opts.signal.addEventListener('abort', onExt, { once: true });
  }
  return {
    signal: ac.signal,
    cleanup() {
      clearTimeout(timer);
      if (opts.signal) opts.signal.removeEventListener('abort', onExt);
    },
  };
}

// ─── Fetch a URL ──────────────────────────────────────────────────────────────

interface WebFetchOptions extends DeadlineOptions {
  maxChars?: number;
  maxBytes?: number;
  page?: number;
  env?: Record<string, string | undefined>;
  fetchImpl?: FetchImpl;
  lookup?: LookupFn;
}

export interface WebFetchResult {
  url?: string;
  title?: string;
  contentType?: string;
  page?: number;
  totalChars?: number;
  truncated?: boolean;
  text?: string;
  error?: string;
}

/**
 * Fetch one URL and return readable text + title. Never throws — returns { error }
 * on failure. Supports `page` (1-based) for long documents.
 */
export async function webFetch(
  url: string,
  opts: WebFetchOptions = {},
): Promise<WebFetchResult> {
  const maxChars = opts.maxChars ?? 15000;
  const page = Math.max(1, Math.floor(opts.page ?? 1));
  const start = (page - 1) * maxChars;
  const minBytes = Math.max(2_000_000, (start + maxChars) * 10);
  const maxBytes = Math.min(
    opts.maxBytes !== undefined ? Math.max(opts.maxBytes, minBytes) : minBytes,
    10_000_000,
  );
  const deadline = createDeadline(opts);
  try {
    const { res, finalUrl } = await safeFetch(url, { ...opts, signal: deadline.signal });
    if (!res.ok) return { error: `HTTP ${res.status} fetching ${url}` };
    const contentType = res.headers.get('content-type') ?? '';
    const { text: raw, truncated: bytesTruncated } = await readCapped(res, maxBytes, {
      signal: deadline.signal,
    });
    const isHtml = /html/i.test(contentType) || /^\s*<(!doctype|html)/i.test(raw);
    const title = isHtml ? extractTitle(raw) : '';
    const body = isHtml ? htmlToText(raw) : raw.trim();
    const end = start + maxChars;
    const hasMore = body.length > end;
    const text = body.slice(start, end);
    return {
      url: finalUrl,
      title,
      contentType,
      page,
      totalChars: body.length,
      truncated: bytesTruncated || hasMore,
      text: text || (start > 0 ? '(no content at this page offset)' : ''),
    };
  } catch (err) {
    const msg = err instanceof Error ? err.message : String(err);
    return { error: `web fetch failed: ${msg}` };
  } finally {
    deadline.cleanup();
  }
}

// ─── Search utilities ──────────────────────────────────────────────────────────

/** DuckDuckGo HTML result href are wrapped as /l/?uddg=<encoded-target>. Unwrap them. */
export function unwrapDdgHref(href: string): string {
  try {
    const u = new URL(href, 'https://duckduckgo.com');
    const uddg = u.searchParams.get('uddg');
    return uddg ? decodeURIComponent(uddg) : href;
  } catch {
    return href;
  }
}

export interface SearchResult {
  title: string;
  url: string;
  snippet: string;
}

/** Parse DuckDuckGo HTML SERP into { title, url, snippet }[]. */
export function parseDuckDuckGo(html: string, maxResults = 5): SearchResult[] {
  const results: SearchResult[] = [];
  const anchorRe =
    /<a[^>]+class="[^"]*result__a[^"]*"[^>]+href="([^"]+)"[^>]*>([\s\S]*?)<\/a>/gi;
  const snippetRe =
    /<a[^>]+class="[^"]*result__snippet[^"]*"[^>]*>([\s\S]*?)<\/a>/gi;
  const snippets: string[] = [];
  let s: RegExpExecArray | null;
  while ((s = snippetRe.exec(html)) !== null) snippets.push(htmlToText(s[1] ?? ''));
  let m: RegExpExecArray | null;
  let i = 0;
  while ((m = anchorRe.exec(html)) !== null && results.length < maxResults) {
    results.push({
      title: htmlToText(m[2] ?? ''),
      url: unwrapDdgHref(m[1] ?? ''),
      snippet: snippets[i] ?? '',
    });
    i++;
  }
  return results;
}

interface SearchOptions extends DeadlineOptions {
  maxResults?: number;
  maxBytes?: number;
  engine?: string;
  env?: Record<string, string | undefined>;
  apiKey?: string;
  depth?: string;
  topic?: string;
  timeRange?: string;
  includeDomains?: string[];
  excludeDomains?: string[];
  gl?: string;
  hl?: string;
  _timeRangeFallback?: boolean;
  fetchImpl?: FetchImpl;
  lookup?: LookupFn;
}

export interface WebSearchResult {
  engine?: string;
  query?: string;
  answer?: string;
  results?: SearchResult[];
  error?: string;
}

/** DuckDuckGo HTML scrape (no key). Never throws. */
export async function duckDuckGoSearch(
  query: string,
  opts: SearchOptions = {},
): Promise<WebSearchResult> {
  const maxResults = opts.maxResults ?? 5;
  const endpoint = `https://html.duckduckgo.com/html/?q=${encodeURIComponent(query)}`;
  const deadline = createDeadline(opts);
  try {
    const { res } = await safeFetch(endpoint, { ...opts, signal: deadline.signal });
    if (!res.ok) return { error: `HTTP ${res.status} from search backend` };
    const { text: html } = await readCapped(res, opts.maxBytes, { signal: deadline.signal });
    return { engine: 'duckduckgo', query, results: parseDuckDuckGo(html, maxResults) };
  } finally {
    deadline.cleanup();
  }
}

// Tavily allows a Bearer token or a raw key; strip any Authorization/Bearer prefix.
export function normalizeApiKey(raw: string | undefined): string {
  return String(raw ?? '')
    .trim()
    .replace(/^Authorization\s*:\s*/i, '')
    .replace(/^Bearer\s+/i, '')
    .trim();
}

// Serper time filter → Google `tbs` qdr token.
const SERPER_TBS: Record<string, string> = {
  day: 'qdr:d',
  week: 'qdr:w',
  month: 'qdr:m',
  year: 'qdr:y',
};

interface PostJsonDeps {
  fetchImpl?: FetchImpl;
  signal?: AbortSignal;
  timeoutMs?: number;
}

/**
 * Shared JSON POST for the search-provider APIs. Throws a status-bearing
 * Error on non-2xx.
 */
export async function postJson(
  url: string,
  opts: { headers?: Record<string, string>; body: unknown } & PostJsonDeps = {
    body: undefined,
  },
): Promise<unknown> {
  const { headers = {}, body, fetchImpl = globalThis.fetch as FetchImpl, signal, timeoutMs = 30000 } = opts;
  // Always enforce a timeout. When a caller signal is also present, race the two
  // so a passed signal never disables the deadline (a stalled provider would hang).
  const timeoutSignal = AbortSignal.timeout(timeoutMs);
  const combinedSignal = signal ? AbortSignal.any([signal, timeoutSignal]) : timeoutSignal;
  const res = await fetchImpl(url, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json', ...headers },
    body: JSON.stringify(body),
    signal: combinedSignal,
  });
  if (!res.ok) {
    const err: Error & { status?: number } = new Error(`HTTP ${res.status}`);
    err.status = res.status;
    throw err;
  }
  return res.json();
}

/**
 * Tavily /search — AI answer + results. Never throws.
 */
export async function tavilySearch(
  query: string,
  opts: SearchOptions = {},
  deps: PostJsonDeps = {},
): Promise<WebSearchResult> {
  const apiKey = normalizeApiKey(opts.apiKey);
  if (!apiKey) return { error: 'TAVILY_API_KEY not set' };
  const body: Record<string, unknown> = {
    query,
    search_depth: opts.depth ?? 'basic',
    topic: opts.topic ?? 'general',
    max_results: Math.max(0, Math.min(20, opts.maxResults ?? 5)),
    time_range: opts.timeRange ?? undefined,
    include_answer: true,
    include_raw_content: false,
  };
  if (opts.includeDomains?.length) body['include_domains'] = opts.includeDomains;
  if (opts.excludeDomains?.length) body['exclude_domains'] = opts.excludeDomains;
  try {
    const raw = (await postJson('https://api.tavily.com/search', {
      headers: { Authorization: `Bearer ${apiKey}` },
      body,
      fetchImpl: deps.fetchImpl,
      signal: opts.signal,
      timeoutMs: opts.timeoutMs,
    })) as {
      answer?: string;
      results?: Array<{ title?: string; url?: string; content?: string }>;
    };
    return {
      engine: 'tavily',
      query,
      answer: raw.answer ?? '',
      results: (raw.results ?? []).map((r) => ({
        title: r.title ?? '',
        url: r.url ?? '',
        snippet: r.content ?? '',
      })),
    };
  } catch (err) {
    const e = err as { status?: number; message?: string };
    return { error: `Tavily API ${e.status ?? 'error'}: ${e.message ?? String(err)}` };
  }
}

/**
 * Serper /search — Google SERP. Never throws.
 */
export async function serperSearch(
  query: string,
  opts: SearchOptions = {},
  deps: PostJsonDeps = {},
): Promise<WebSearchResult> {
  const apiKey = normalizeApiKey(opts.apiKey);
  if (!apiKey) return { error: 'SERPER_API_KEY not set' };
  const body: Record<string, unknown> = {
    q: query,
    num: opts.maxResults ?? 8,
    gl: opts.gl ?? 'us',
    hl: opts.hl ?? 'en',
  };
  const tbs = opts.timeRange ? SERPER_TBS[opts.timeRange] : undefined;
  if (tbs) body['tbs'] = tbs;
  let raw: {
    answerBox?: { answer?: string; snippet?: string; title?: string };
    knowledgeGraph?: { description?: string };
    organic?: Array<{ title?: string; link?: string; snippet?: string }>;
  };
  try {
    raw = (await postJson('https://google.serper.dev/search', {
      headers: { 'X-API-KEY': apiKey },
      body,
      fetchImpl: deps.fetchImpl,
      signal: opts.signal,
      timeoutMs: opts.timeoutMs,
    })) as typeof raw;
  } catch (err) {
    const e = err as { status?: number; message?: string };
    return { error: `Serper API ${e.status ?? 'error'}: ${e.message ?? String(err)}` };
  }
  let answer = '';
  if (raw.answerBox)
    answer =
      raw.answerBox.answer ??
      raw.answerBox.snippet ??
      raw.answerBox.title ??
      '';
  else if (raw.knowledgeGraph?.description) answer = raw.knowledgeGraph.description;
  return {
    engine: 'serper',
    query,
    answer,
    results: (raw.organic ?? [])
      .slice(0, opts.maxResults ?? 8)
      .map((r) => ({
        title: r.title ?? '',
        url: r.link ?? '',
        snippet: r.snippet ?? '',
      })),
  };
}

/**
 * Pick the search provider: explicit `engine` wins, else the ladder
 * Tavily → Serper → DuckDuckGo by which key is present in env.
 */
export function pickProvider(
  opts: { engine?: string; env?: Record<string, string | undefined> } = {},
): string {
  const env = opts.env ?? process.env;
  if (
    opts.engine === 'tavily' ||
    opts.engine === 'serper' ||
    opts.engine === 'duckduckgo'
  )
    return opts.engine;
  if (normalizeApiKey(env['TAVILY_API_KEY'] ?? env['TAVILY_API_TOKEN'])) return 'tavily';
  if (normalizeApiKey(env['SERPER_API_KEY'])) return 'serper';
  return 'duckduckgo';
}

/**
 * Search the web via the provider ladder. Never throws — returns { error } on
 * failure. Keys read from process.env (populated by env.ts from ~/.octocode/.env).
 * When `timeRange` is set and the first call returns zero results, retries once
 * without the time filter.
 */
export async function webSearch(
  query: string,
  opts: SearchOptions = {},
): Promise<WebSearchResult> {
  const env = opts.env ?? process.env;
  const provider = pickProvider({ engine: opts.engine, env });
  try {
    let result: WebSearchResult;
    if (provider === 'tavily') {
      result = await tavilySearch(
        query,
        { ...opts, apiKey: env['TAVILY_API_KEY'] ?? env['TAVILY_API_TOKEN'] },
        opts,
      );
    } else if (provider === 'serper') {
      result = await serperSearch(
        query,
        { ...opts, apiKey: env['SERPER_API_KEY'] },
        opts,
      );
    } else {
      result = await duckDuckGoSearch(query, opts);
    }
    // Silent fallback: timeRange + zero results → retry without it once.
    if (
      opts.timeRange &&
      !result.error &&
      result.results?.length === 0 &&
      !opts._timeRangeFallback
    ) {
      return webSearch(query, {
        ...opts,
        timeRange: undefined,
        _timeRangeFallback: true,
      });
    }
    return result;
  } catch (err) {
    const msg = err instanceof Error ? err.message : String(err);
    return { error: `web search (${provider}) failed: ${msg}` };
  }
}

// ─── Result rendering ──────────────────────────────────────────────────────────

/** Render a tool result object to the plain-text the model reads. */
export function renderWebResult(out: WebFetchResult | WebSearchResult): string {
  if (out.error) return out.error;
  const search = out as WebSearchResult;
  if (search.results) {
    const head = `Web results for "${search.query ?? ''}"${search.engine ? ` [${search.engine}]` : ''}:`;
    const answer = search.answer ? `Answer: ${search.answer}\n` : '';
    if (search.results.length === 0)
      return `${answer}No results for "${search.query ?? ''}".`;
    return [
      answer + head,
      ...search.results.map(
        (r, i) =>
          `${i + 1}. ${r.title}\n   ${r.url}${r.snippet ? `\n   ${r.snippet}` : ''}`,
      ),
    ].join('\n');
  }
  const fetch = out as WebFetchResult;
  const pageLabel = (fetch.page ?? 1) > 1 ? ` [page ${fetch.page}]` : '';
  const truncNote = fetch.truncated
    ? `(truncated — pass page: ${(fetch.page ?? 1) + 1} to continue)`
    : null;
  const head = [
    fetch.title && `# ${fetch.title}`,
    `Source: ${fetch.url ?? ''}${pageLabel}`,
    truncNote,
  ]
    .filter(Boolean)
    .join('\n');
  return `${head}\n\n${fetch.text ?? ''}`;
}

// ─── Tool dispatch ───────────────────────────────────────────────────────────

export interface WebToolParams {
  url?: string;
  query?: string;
  maxResults?: number;
  maxChars?: number;
  maxBytes?: number;
  page?: number;
  engine?: string;
  timeRange?: string;
  includeDomains?: string[];
  excludeDomains?: string[];
}

export interface WebToolDeps {
  signal?: AbortSignal;
  fetchImpl?: FetchImpl;
  lookup?: LookupFn;
}

/** Dispatch the single `web` tool: url → fetch/read a page; query → search. */
export async function runWebTool(
  params: WebToolParams = {},
  deps: WebToolDeps = {},
): Promise<WebFetchResult | WebSearchResult> {
  if (params.url)
    return webFetch(params.url, {
      ...deps,
      maxChars: params.maxChars,
      maxBytes: params.maxBytes,
      page: params.page,
    });
  if (params.query) {
    return webSearch(params.query, {
      ...deps,
      maxResults: params.maxResults,
      engine: params.engine,
      timeRange: params.timeRange,
      includeDomains: params.includeDomains,
      excludeDomains: params.excludeDomains,
    });
  }
  return { error: 'Provide either `url` (to read a page) or `query` (to search).' };
}
