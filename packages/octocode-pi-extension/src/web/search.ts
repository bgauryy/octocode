import { errorMessage, isRecord } from '../shared/util.js';
import { USER_AGENT, combineSignals, language } from './fetch.js';
import { decodeEntities, htmlToText } from './html.js';

interface SearchHit {
  title: string;
  url: string;
  snippet: string;
}

export function parseDuckDuckGo(html: string, max: number): SearchHit[] {
  const hits: SearchHit[] = [];
  const blocks = html.split(/<div[^>]+class="[^"]*result__body/).slice(1);
  for (const block of blocks) {
    const link = /<a[^>]+class="result__a"[^>]+href="([^"]+)"[^>]*>([\s\S]*?)<\/a>/.exec(block);
    if (!link) continue;
    const snippet = /class="result__snippet"[^>]*>([\s\S]*?)<\/a>/.exec(block)?.[1] ?? '';
    const url = unwrapDuckDuckGo(decodeEntities(link[1]!));
    if (!url) continue;
    hits.push({ url, title: htmlToText(link[2]!), snippet: htmlToText(snippet) });
    if (hits.length >= max) break;
  }
  return hits;
}

/** DuckDuckGo lite's table layout: an `a.result-link` per hit, its `td.result-snippet` before the next hit. */
export function parseDuckDuckGoLite(html: string, max: number): SearchHit[] {
  const hits: SearchHit[] = [];
  const links = [...html.matchAll(/<a\b([^>]*class=['"]result-link['"][^>]*)>([\s\S]*?)<\/a>/gi)];
  links.forEach((link, index) => {
    const href = /href=['"]([^'"]+)['"]/.exec(link[1]!)?.[1];
    if (!href || hits.length >= max) return;
    const rest = html.slice(link.index + link[0].length, links[index + 1]?.index ?? html.length);
    const snippet = /<td[^>]*class=['"]result-snippet['"][^>]*>([\s\S]*?)<\/td>/i.exec(rest)?.[1] ?? '';
    const url = unwrapDuckDuckGo(decodeEntities(href));
    if (url) hits.push({ url, title: htmlToText(link[2]!), snippet: htmlToText(snippet) });
  });
  return hits;
}

/** Bing's HTML results: an `li.b_algo` per hit, with the link in its `h2` and the snippet in its first paragraph. */
export function parseBing(html: string, max: number): SearchHit[] {
  const hits: SearchHit[] = [];
  for (const block of html.split(/<li class="b_algo"/).slice(1)) {
    const link = /<h2[^>]*>\s*<a\b[^>]*\bhref="([^"]+)"[^>]*>([\s\S]*?)<\/a>/.exec(block);
    if (!link) continue;
    const snippet = /<p\b[^>]*>([\s\S]*?)<\/p>/.exec(block)?.[1] ?? '';
    const url = unwrapBing(decodeEntities(link[1]!));
    if (!url) continue;
    hits.push({ url, title: htmlToText(link[2]!), snippet: htmlToText(snippet) });
    if (hits.length >= max) break;
  }
  return hits;
}

/** `candidate` when it is an absolute http(s) URL; a result linking anywhere else (`javascript:`, `data:`) is dropped. */
function httpUrl(candidate: string | null | undefined): string | undefined {
  if (!candidate) return undefined;
  try {
    const { protocol } = new URL(candidate);
    return protocol === 'http:' || protocol === 'https:' ? candidate : undefined;
  } catch {
    return undefined;
  }
}

/** Bing wraps result links in a click tracker whose `u` parameter is `a1` plus the base64url target. */
function unwrapBing(href: string): string | undefined {
  try {
    const url = new URL(href);
    const target = url.searchParams.get('u');
    if (!url.hostname.endsWith('bing.com') || !target?.startsWith('a1')) return httpUrl(href);
    const decoded = Buffer.from(target.slice(2), 'base64url').toString('utf8').replace(/["'\s<>].*$/s, '');
    return httpUrl(decoded) ?? httpUrl(href);
  } catch {
    return undefined;
  }
}

/** DuckDuckGo links are relative trackers whose `uddg` parameter is the target. */
function unwrapDuckDuckGo(href: string): string | undefined {
  try {
    const url = new URL(href, 'https://duckduckgo.com');
    return httpUrl(url.searchParams.get('uddg') ?? url.toString());
  } catch {
    return undefined;
  }
}

interface SearchOutcome {
  provider: string;
  hits: SearchHit[];
  /** Providers tried before the one that answered, with why they did not. */
  failures: string[];
}

/**
 * Searches through a chain: every configured API (Tavily, Serper, Exa, Brave), then the keyless HTML pages
 * (DuckDuckGo, Bing, DuckDuckGo lite). A provider that fails (a bad key, a rate limit) passes to the next, and so
 * does one with no hits (a block page often parses as empty, and another index may know the query). Throws only when
 * every provider failed.
 */
export async function search(query: string, max: number, env: NodeJS.ProcessEnv, signal?: AbortSignal): Promise<SearchOutcome> {
  const post = async (url: string, headers: Record<string, string>, body: unknown) => {
    const response = await fetch(url, { method: 'POST', headers: { 'content-type': 'application/json', ...headers }, body: JSON.stringify(body), signal: combineSignals(signal) });
    if (!response.ok) throw new Error(`HTTP ${response.status} from ${new URL(url).host}`);
    return (await response.json()) as Record<string, unknown>;
  };
  const page = async (url: string, headers: Record<string, string> = {}) => {
    const response = await fetch(url, { headers: { 'user-agent': USER_AGENT, 'accept-language': language(env), ...headers }, signal: combineSignals(signal) });
    // Search pages answer rate limits with a non-200 challenge page; report it instead of "No results".
    if (response.status !== 200) throw new Error(`HTTP ${response.status} from ${new URL(url).host}${response.status === 202 || response.status === 429 ? ' (rate limited)' : ''}`);
    return response.text();
  };
  const q = encodeURIComponent(query);
  // API results are untrusted like scraped ones: only http(s) links become hits.
  const hits = (value: unknown, url: string, snippet: string, clean: (text: string) => string = (text) => text): SearchHit[] =>
    (Array.isArray(value) ? value.filter(isRecord) : []).flatMap((r) => {
      const link = httpUrl(String(r[url] ?? ''));
      return link ? [{ title: String(r['title'] ?? ''), url: link, snippet: clean(String(r[snippet] ?? '')) }] : [];
    });
  const keyed = [
    { key: 'TAVILY_API_KEY', name: 'tavily', run: async (key: string) => hits((await post('https://api.tavily.com/search', { authorization: `Bearer ${key}` }, { query, max_results: max }))['results'], 'url', 'content') },
    { key: 'SERPER_API_KEY', name: 'serper', run: async (key: string) => hits((await post('https://google.serper.dev/search', { 'x-api-key': key }, { q: query, num: max }))['organic'], 'link', 'snippet') },
    { key: 'EXA_API_KEY', name: 'exa', run: async (key: string) => hits((await post('https://api.exa.ai/search', { 'x-api-key': key }, { query, numResults: max, contents: { text: { maxCharacters: 400 } } }))['results'], 'url', 'text') },
    {
      key: 'BRAVE_API_KEY',
      name: 'brave',
      run: async (key: string) => {
        const data = JSON.parse(await page(`https://api.search.brave.com/res/v1/web/search?q=${q}&count=${max}`, { accept: 'application/json', 'x-subscription-token': key })) as Record<string, unknown>;
        return hits(isRecord(data['web']) ? data['web']['results'] : undefined, 'url', 'description', htmlToText);
      },
    },
  ].flatMap(({ key, name, run }) => {
    const value = env[key];
    return value ? [{ name, run: () => run(value) }] : [];
  });
  const keyless = [
    { name: 'duckduckgo', run: async () => parseDuckDuckGo(await page(`https://html.duckduckgo.com/html/?q=${q}`), max) },
    { name: 'bing', run: async () => parseBing(await page(`https://www.bing.com/search?q=${q}&count=${max}`), max) },
    { name: 'duckduckgo-lite', run: async () => parseDuckDuckGoLite(await page(`https://lite.duckduckgo.com/lite/?q=${q}`), max) },
  ];
  const failures: string[] = [];
  const empty: string[] = [];
  for (const provider of [...keyed, ...keyless]) {
    try {
      const hits = await provider.run();
      if (hits.length > 0) return { provider: provider.name, hits, failures: [...failures, ...empty.map((name) => `${name}: no results`)] };
      empty.push(provider.name);
    } catch (error) {
      if (signal?.aborted) throw error;
      failures.push(`${provider.name}: ${errorMessage(error)}`);
    }
  }
  if (empty.length > 0) return { provider: empty.join(', '), hits: [], failures };
  throw new Error(`every provider failed (${failures.join('; ')}). Set TAVILY_API_KEY, SERPER_API_KEY, EXA_API_KEY or BRAVE_API_KEY for reliable search.`);
}
