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

// A realistic mainstream-browser User-Agent so the agent reads pages like a browser —
// many sites (and DuckDuckGo's HTML endpoint) block or degrade non-browser UAs.
// Override with OCTOCODE_WEB_USER_AGENT. API providers (Tavily/Serper) use their own
// auth headers and are unaffected by this.
//
// Chrome version kept in sync with DEFAULT_SEC_CH_UA below — bump both together.
export const DEFAULT_USER_AGENT =
  'Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/137.0.0.0 Safari/537.36';

// Client-hint headers Chrome always sends alongside its UA.
// Absence of sec-ch-ua when the UA claims Chrome is a primary Cloudflare/bot-detection signal.
// Keep the version token in sync with DEFAULT_USER_AGENT above.
export const DEFAULT_SEC_CH_UA = '"Chromium";v="137", "Google Chrome";v="137", "Not/A)Brand";v="24"';

export function resolveUserAgent(env = process.env) {
  const custom = env && env.OCTOCODE_WEB_USER_AGENT;
  return custom && String(custom).trim() ? String(custom).trim() : DEFAULT_USER_AGENT;
}

// [network base, mask bits] — security-relevant IPv4 ranges to block.
const V4_BLOCKS = [
  ['0.0.0.0', 8], ['10.0.0.0', 8], ['100.64.0.0', 10], ['127.0.0.0', 8],
  ['169.254.0.0', 16], ['172.16.0.0', 12], ['192.0.0.0', 24], ['192.168.0.0', 16],
  ['198.18.0.0', 15],
];

function ipv4ToInt(ip) {
  const parts = ip.split('.').map(Number);
  if (parts.length !== 4 || parts.some((n) => Number.isNaN(n) || n < 0 || n > 255)) return null;
  return (((parts[0] << 24) >>> 0) + (parts[1] << 16) + (parts[2] << 8) + parts[3]) >>> 0;
}

function inV4(ip, base, maskBits) {
  const i = ipv4ToInt(ip);
  const b = ipv4ToInt(base);
  if (i === null || b === null) return false;
  const mask = maskBits === 0 ? 0 : (0xffffffff << (32 - maskBits)) >>> 0;
  return (i & mask) === (b & mask);
}

/** True if an IP literal is in a non-public range (or is not a valid IP → fail closed). */
export function isBlockedIp(ip) {
  if (typeof ip !== 'string') return true;
  const version = net.isIP(ip);
  if (version === 4) return V4_BLOCKS.some(([base, bits]) => inV4(ip, base, bits));
  if (version === 6) {
    const lo = ip.toLowerCase();
    if (lo === '::1' || lo === '::') return true;
    const mapped = lo.match(/^::ffff:(\d+\.\d+\.\d+\.\d+)$/);
    if (mapped) return isBlockedIp(mapped[1]);
    if (/^fe[89ab]/.test(lo)) return true;              // fe80::/10 link-local
    const first = parseInt(lo.split(':')[0] || '0', 16);
    if ((first & 0xfe00) === 0xfc00) return true;        // fc00::/7 ULA
    return false;
  }
  return true; // not a valid IP literal
}

/** Parse+validate a URL: http/https only, and every resolved IP must be public. */
export async function assertPublicUrl(urlStr, { lookup = dns.lookup } = {}) {
  let url;
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

/** Fetch with manual redirect following, re-validating every hop against the SSRF guard. */
export async function safeFetch(startUrl, opts = {}) {
  const {
    fetchImpl = globalThis.fetch,
    lookup = dns.lookup,
    timeoutMs = 15000,
    maxRedirects = 5,
    headers = {},
    signal: externalSignal,
  } = opts;

  let current = startUrl;
  for (let hop = 0; hop <= maxRedirects; hop++) {
    await assertPublicUrl(current, { lookup });
    const ac = new AbortController();
    const timer = setTimeout(() => ac.abort(), timeoutMs);
    const onAbort = () => ac.abort();
    if (externalSignal) externalSignal.addEventListener('abort', onAbort, { once: true });
    let res;
    try {
      res = await fetchImpl(current, {
        redirect: 'manual',
        signal: ac.signal,
        headers: {
          'user-agent': resolveUserAgent(opts.env),
          // Sec-CH-UA client hints: Chrome always sends these; their absence while
          // presenting a Chrome UA is a primary Cloudflare/bot-detector trigger.
          // Omit when a custom UA is set — the caller is responsible for consistency.
          ...(opts.env?.OCTOCODE_WEB_USER_AGENT ? {} : {
            'sec-ch-ua': DEFAULT_SEC_CH_UA,
            'sec-ch-ua-mobile': '?0',
            'sec-ch-ua-platform': '"macOS"',
          }),
          // Sec-Fetch headers that browsers attach for top-level navigations.
          'sec-fetch-site': 'none',
          'sec-fetch-mode': 'navigate',
          'sec-fetch-dest': 'document',
          accept: 'text/html,application/xhtml+xml,application/xml;q=0.9,image/avif,image/webp,*/*;q=0.8',
          'accept-language': 'en-US,en;q=0.9',
          'accept-encoding': 'gzip, deflate, br',
          ...headers,
        },
      });
    } finally {
      clearTimeout(timer);
      if (externalSignal) externalSignal.removeEventListener('abort', onAbort);
    }
    const location = res.status >= 300 && res.status < 400 ? res.headers.get('location') : null;
    if (location) {
      current = new URL(location, current).toString();
      continue;
    }
    return { res, finalUrl: current };
  }
  throw new Error(`Too many redirects (>${maxRedirects})`);
}

/**
 * Read a response body up to maxBytes, aborting the stream once the cap is hit OR the
 * optional `signal` fires (whole-operation deadline / caller cancel). Without a signal a
 * stalled body would hang, since the fetch's own timeout only covers headers.
 */
export async function readCapped(res, maxBytes = 2_000_000, { signal } = {}) {
  if (!res.body || typeof res.body.getReader !== 'function') {
    const text = await res.text();
    return text.length > maxBytes ? { text: text.slice(0, maxBytes), truncated: true } : { text, truncated: false };
  }
  const reader = res.body.getReader();
  const decoder = new TextDecoder('utf-8', { fatal: false });
  const abortPromise = signal
    ? new Promise((resolve) => {
        if (signal.aborted) resolve({ __aborted: true });
        else signal.addEventListener('abort', () => resolve({ __aborted: true }), { once: true });
      })
    : null;
  let out = '';
  let total = 0;
  let truncated = false;
  for (;;) {
    const chunk = abortPromise ? await Promise.race([reader.read(), abortPromise]) : await reader.read();
    if (chunk.__aborted) {
      truncated = true;
      try { await reader.cancel(); } catch { /* ignore */ }
      break;
    }
    const { done, value } = chunk;
    if (done) break;
    total += value.length;
    out += decoder.decode(value, { stream: true });
    if (total >= maxBytes) {
      truncated = true;
      try { await reader.cancel(); } catch { /* ignore */ }
      break;
    }
  }
  out += decoder.decode();
  return { text: out, truncated };
}

const ENTITIES = { amp: '&', lt: '<', gt: '>', quot: '"', apos: "'", nbsp: ' ', '#39': "'", '#x27': "'" };

export function decodeEntities(str) {
  return str
    .replace(/&#(\d+);/g, (_, d) => String.fromCodePoint(Number(d)))
    .replace(/&#x([0-9a-f]+);/gi, (_, h) => String.fromCodePoint(parseInt(h, 16)))
    .replace(/&([a-z]+|#x?\w+);/gi, (m, name) => ENTITIES[name] ?? ENTITIES[name.toLowerCase()] ?? m);
}

export function extractTitle(html) {
  const m = html.match(/<title[^>]*>([\s\S]*?)<\/title>/i);
  return m ? decodeEntities(m[1]).replace(/\s+/g, ' ').trim() : '';
}

/** Strip a page to readable plain text: drop script/style/nav noise, tags, collapse whitespace. */
export function htmlToText(html) {
  const text = html
    .replace(/<!--[\s\S]*?-->/g, ' ')
    .replace(/<(script|style|noscript|template|svg)[\s\S]*?<\/\1>/gi, ' ')
    .replace(/<\/(p|div|section|article|h[1-6]|li|tr|br|header|footer)>/gi, '\n')
    .replace(/<br\s*\/?>/gi, '\n')
    .replace(/<[^>]+>/g, ' ');
  return decodeEntities(text)
    .replace(/[ \t\f\v]+/g, ' ')
    .replace(/\n\s*\n\s*\n+/g, '\n\n')
    .split('\n').map((l) => l.trim()).join('\n')
    .trim();
}

/**
 * A single deadline (timeout + optional external signal) covering an entire fetch op —
 * headers AND body — so a stalled body stream can't hang. Returns a signal + cleanup.
 */
export function createDeadline(opts = {}) {
  const ac = new AbortController();
  const timer = setTimeout(() => ac.abort(), opts.timeoutMs ?? 15000);
  const onExt = () => ac.abort();
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

/** Fetch one URL and return readable text + title. Never throws — returns { error } on failure. */
export async function webFetch(url, opts = {}) {
  const maxChars = opts.maxChars ?? 15000;
  const deadline = createDeadline(opts);
  try {
    const { res, finalUrl } = await safeFetch(url, { ...opts, signal: deadline.signal });
    if (!res.ok) return { error: `HTTP ${res.status} fetching ${url}` };
    const contentType = res.headers.get('content-type') || '';
    const { text: raw, truncated: bytesTruncated } = await readCapped(res, opts.maxBytes, { signal: deadline.signal });
    const isHtml = /html/i.test(contentType) || /^\s*<(!doctype|html)/i.test(raw);
    const title = isHtml ? extractTitle(raw) : '';
    const body = isHtml ? htmlToText(raw) : raw.trim();
    const clipped = body.length > maxChars;
    return {
      url: finalUrl,
      title,
      contentType,
      truncated: bytesTruncated || clipped,
      text: clipped ? body.slice(0, maxChars) : body,
    };
  } catch (err) {
    return { error: `web fetch failed: ${err?.message ?? String(err)}` };
  } finally {
    deadline.cleanup();
  }
}

/** DuckDuckGo HTML result href are wrapped as /l/?uddg=<encoded-target>. Unwrap them. */
export function unwrapDdgHref(href) {
  try {
    const u = new URL(href, 'https://duckduckgo.com');
    const uddg = u.searchParams.get('uddg');
    return uddg ? decodeURIComponent(uddg) : href;
  } catch {
    return href;
  }
}

/** Parse DuckDuckGo HTML SERP into { title, url, snippet }[]. */
export function parseDuckDuckGo(html, maxResults = 5) {
  const results = [];
  const anchorRe = /<a[^>]+class="[^"]*result__a[^"]*"[^>]+href="([^"]+)"[^>]*>([\s\S]*?)<\/a>/gi;
  const snippetRe = /<a[^>]+class="[^"]*result__snippet[^"]*"[^>]*>([\s\S]*?)<\/a>/gi;
  const snippets = [];
  let s;
  while ((s = snippetRe.exec(html)) !== null) snippets.push(htmlToText(s[1]));
  let m;
  let i = 0;
  while ((m = anchorRe.exec(html)) !== null && results.length < maxResults) {
    results.push({
      title: htmlToText(m[2]),
      url: unwrapDdgHref(m[1]),
      snippet: snippets[i] ?? '',
    });
    i++;
  }
  return results;
}

/** DuckDuckGo HTML scrape (no key). Never throws. */
export async function duckDuckGoSearch(query, opts = {}) {
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
export function normalizeApiKey(raw) {
  return String(raw || '').trim().replace(/^Authorization\s*:\s*/i, '').replace(/^Bearer\s+/i, '').trim();
}

// Serper time filter → Google `tbs` qdr token.
const SERPER_TBS = { day: 'qdr:d', week: 'qdr:w', month: 'qdr:m', year: 'qdr:y' };

/**
 * Shared JSON POST for the search-provider APIs: sets Content-Type, applies a timeout or
 * caller signal, and parses the JSON body. Throws a status-bearing Error on non-2xx.
 */
export async function postJson(url, { headers = {}, body, fetchImpl = globalThis.fetch, signal, timeoutMs = 30000 } = {}) {
  const res = await fetchImpl(url, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json', ...headers },
    body: JSON.stringify(body),
    signal: signal ?? AbortSignal.timeout(timeoutMs),
  });
  if (!res.ok) {
    const err = new Error(`HTTP ${res.status}`);
    err.status = res.status;
    throw err;
  }
  return res.json();
}

/**
 * Tavily /search — AI answer + results. Contract ported from
 * skills/octocode-brainstorming/scripts/tavily-search.mjs. Never throws.
 */
export async function tavilySearch(query, opts = {}, deps = {}) {
  const apiKey = normalizeApiKey(opts.apiKey);
  if (!apiKey) return { error: 'TAVILY_API_KEY not set' };
  const body = {
    query,
    search_depth: opts.depth ?? 'basic',
    topic: opts.topic ?? 'general',
    max_results: Math.max(0, Math.min(20, opts.maxResults ?? 5)),
    time_range: opts.timeRange ?? undefined,
    include_answer: true,
    include_raw_content: false,
  };
  if (opts.includeDomains?.length) body.include_domains = opts.includeDomains;
  if (opts.excludeDomains?.length) body.exclude_domains = opts.excludeDomains;
  try {
    const raw = await postJson('https://api.tavily.com/search', {
      headers: { Authorization: `Bearer ${apiKey}` },
      body, fetchImpl: deps.fetchImpl, signal: opts.signal, timeoutMs: opts.timeoutMs,
    });
    return {
      engine: 'tavily',
      query,
      answer: raw.answer || '',
      results: (raw.results || []).map((r) => ({ title: r.title || '', url: r.url || '', snippet: r.content || '' })),
    };
  } catch (err) {
    return { error: `Tavily API ${err.status ?? 'error'}: ${err.message}` };
  }
}

/**
 * Serper /search — Google SERP. Contract + normalization ported from
 * skills/octocode-brainstorming/scripts/serper-search.mjs. Never throws.
 */
export async function serperSearch(query, opts = {}, deps = {}) {
  const apiKey = normalizeApiKey(opts.apiKey);
  if (!apiKey) return { error: 'SERPER_API_KEY not set' };
  const body = { q: query, num: opts.maxResults ?? 8, gl: opts.gl ?? 'us', hl: opts.hl ?? 'en' };
  const tbs = SERPER_TBS[opts.timeRange];
  if (tbs) body.tbs = tbs;
  let raw;
  try {
    raw = await postJson('https://google.serper.dev/search', {
      headers: { 'X-API-KEY': apiKey },
      body, fetchImpl: deps.fetchImpl, signal: opts.signal, timeoutMs: opts.timeoutMs,
    });
  } catch (err) {
    return { error: `Serper API ${err.status ?? 'error'}: ${err.message}` };
  }
  let answer = '';
  if (raw.answerBox) answer = raw.answerBox.answer || raw.answerBox.snippet || raw.answerBox.title || '';
  else if (raw.knowledgeGraph?.description) answer = raw.knowledgeGraph.description;
  return {
    engine: 'serper',
    query,
    answer,
    results: (raw.organic || []).slice(0, opts.maxResults ?? 8).map((r) => ({
      title: r.title || '', url: r.link || '', snippet: r.snippet || '',
    })),
  };
}

/**
 * Pick the search provider: explicit `engine` wins, else the ladder
 * Tavily → Serper → DuckDuckGo by which key is present in env.
 */
export function pickProvider({ engine, env = process.env } = {}) {
  if (engine === 'tavily' || engine === 'serper' || engine === 'duckduckgo') return engine;
  if (normalizeApiKey(env.TAVILY_API_KEY || env.TAVILY_API_TOKEN)) return 'tavily';
  if (normalizeApiKey(env.SERPER_API_KEY)) return 'serper';
  return 'duckduckgo';
}

/**
 * Search the web via the provider ladder. Never throws — returns { error } on failure.
 * Keys are read from process.env (populated by env.js from ~/.octocode/.env).
 */
export async function webSearch(query, opts = {}) {
  const env = opts.env ?? process.env;
  const provider = pickProvider({ engine: opts.engine, env });
  try {
    if (provider === 'tavily') {
      return await tavilySearch(query, { ...opts, apiKey: env.TAVILY_API_KEY || env.TAVILY_API_TOKEN }, opts);
    }
    if (provider === 'serper') {
      return await serperSearch(query, { ...opts, apiKey: env.SERPER_API_KEY }, opts);
    }
    return await duckDuckGoSearch(query, opts);
  } catch (err) {
    return { error: `web search (${provider}) failed: ${err?.message ?? String(err)}` };
  }
}

/** Render a tool result object to the plain-text the model reads. */
export function renderWebResult(out) {
  if (out.error) return out.error;
  if (out.results) {
    const head = `Web results for "${out.query}"${out.engine ? ` [${out.engine}]` : ''}:`;
    const answer = out.answer ? `Answer: ${out.answer}\n` : '';
    if (out.results.length === 0) return `${answer}No results for "${out.query}".`;
    return [answer + head, ...out.results.map((r, i) =>
      `${i + 1}. ${r.title}\n   ${r.url}${r.snippet ? `\n   ${r.snippet}` : ''}`)].join('\n');
  }
  const head = [out.title && `# ${out.title}`, `Source: ${out.url}`, out.truncated && '(truncated)']
    .filter(Boolean).join('\n');
  return `${head}\n\n${out.text}`;
}

/** Dispatch the single `web` tool: url → fetch/read a page; query → search. */
export async function runWebTool(params = {}, deps = {}) {
  if (params.url) return webFetch(params.url, { ...deps, maxChars: params.maxChars, maxBytes: params.maxBytes });
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
