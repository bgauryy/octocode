import fs from 'node:fs';
import http from 'node:http';
import type { AddressInfo } from 'node:net';
import { initTheme, type ExtensionAPI } from '@earendil-works/pi-coding-agent';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { clearFetchCache, fetchUrl, unreadReason } from '../src/web/fetch.js';
import { search } from '../src/web/search.js';
import { registerWebTool, webSummary } from '../src/web/web.js';
import { theme } from './fake-pi.js';
import { tmp } from './helpers.js';

type ToolResult = { content: Array<{ type: string; text?: string }> };
type WebTool = {
  execute: (id: string, params: Record<string, unknown>, signal?: AbortSignal) => Promise<ToolResult>;
  renderCall: Function;
  renderResult: Function;
};

const servers: http.Server[] = [];
afterEach(async () => {
  clearFetchCache();
  vi.unstubAllEnvs();
  vi.unstubAllGlobals();
  await Promise.all(servers.splice(0).map((server) => new Promise((resolve) => server.close(resolve))));
});

async function serve(handler: http.RequestListener): Promise<string> {
  const server = http.createServer(handler);
  servers.push(server);
  await new Promise<void>((resolve) => server.listen(0, '127.0.0.1', resolve));
  return `http://127.0.0.1:${(server.address() as AddressInfo).port}`;
}

const privateOk = { OCTOCODE_WEB_ALLOW_PRIVATE: '1' };

function webTool(): WebTool {
  let tool: WebTool | undefined;
  registerWebTool({ registerTool: (definition: WebTool) => (tool = definition) } as unknown as ExtensionAPI);
  return tool!;
}

const text = (result: ToolResult) => result.content.map((part) => part.text ?? '').join('\n');

describe('fetchUrl against a local server', () => {
  it('converts HTML with its title and entities, pretty-prints JSON and passes other text through', async () => {
    const base = await serve((req, res) => {
      if (req.url === '/page') {
        res.writeHead(200, { 'content-type': 'text/html; charset=utf-8' });
        res.end('<html><head><title>Caf&eacute; &amp; &#x41;&#66;</title></head><body><h2>Menu</h2><p>Tea &lt;hot&gt; &#0; &bogus;</p></body></html>');
      } else if (req.url === '/json') {
        res.writeHead(200, { 'content-type': 'application/json' });
        res.end('{"a":[1,2]}');
      } else if (req.url === '/broken-json') {
        res.writeHead(200, { 'content-type': 'application/json' });
        res.end('{not json');
      } else {
        res.writeHead(200, { 'content-type': 'text/plain' });
        res.end('plain body');
      }
    });
    const page = await fetchUrl(`${base}/page`, undefined, privateOk);
    expect(page).toMatch(/^# Caf&eacute; & AB\nSource: http:\/\/127\.0\.0\.1:\d+\/page/);
    expect(page).toContain('## Menu');
    expect(page).toContain('Tea <hot> &#0; &bogus;');
    expect(await fetchUrl(`${base}/json`, undefined, privateOk)).toBe('{\n  "a": [\n    1,\n    2\n  ]\n}');
    expect(await fetchUrl(`${base}/broken-json`, undefined, privateOk)).toBe('{not json');
    expect(await fetchUrl(`${base}/text`, undefined, privateOk)).toBe('plain body');
  });

  it('describes binary bodies instead of decoding them, and reports HTTP errors', async () => {
    const base = await serve((req, res) => {
      if (req.url === '/logo.png') {
        res.writeHead(200, { 'content-type': 'image/png' });
        res.end(Buffer.alloc(64, 1));
      } else if (req.url === '/missing.pdf') {
        res.writeHead(404, { 'content-type': 'application/pdf' });
        res.end('nope');
      } else {
        res.writeHead(500, { 'content-type': 'text/html' });
        res.end('<p>oops</p>');
      }
    });
    const logo = await fetchUrl(`${base}/logo.png`, undefined, privateOk);
    expect(logo).toMatch(/is image\/png, not text; web cannot read it/);
    expect(unreadReason(logo)).toBe('unsupported');
    expect(unreadReason('plain body')).toBeUndefined();
    await expect(fetchUrl(`${base}/missing.pdf`, undefined, privateOk)).rejects.toThrow(/HTTP 404 .* for http/);
    await expect(fetchUrl(`${base}/error`, undefined, privateOk)).rejects.toThrow(/HTTP 500/);
  });

  it('stops reading a body at the 5 MB cap', async () => {
    const chunk = 'x'.repeat(1024 * 1024);
    let written = 0;
    const base = await serve((_req, res) => {
      res.writeHead(200, { 'content-type': 'text/plain' });
      // Far more than the cap: the reader must stop early instead of buffering it all.
      const write = () => {
        while (written < 64) {
          written += 1;
          if (!res.write(chunk)) return void res.once('drain', write);
        }
        res.end();
      };
      res.on('error', () => undefined);
      write();
    });
    const body = await fetchUrl(`${base}/big`, undefined, privateOk);
    expect(body.length).toBeGreaterThanOrEqual(5 * 1024 * 1024);
    expect(body.length).toBeLessThan(8 * 1024 * 1024);
  });

  it('retries transient statuses, honours Retry-After, and names the network cause', async () => {
    let hits = 0;
    const base = await serve((req, res) => {
      hits += 1;
      if (req.url === '/flaky' && hits < 3) {
        res.writeHead(503, { 'retry-after': '0' });
        return void res.end('busy');
      }
      if (req.url === '/reset') return void req.socket.destroy();
      res.writeHead(200, { 'content-type': 'text/plain' });
      res.end(`ok after ${hits}`);
    });
    expect(await fetchUrl(`${base}/flaky`, undefined, privateOk)).toBe('ok after 3');
    await expect(fetchUrl(`${base}/reset`, undefined, privateOk)).rejects.toThrow(/^Could not fetch http:\/\/127\.0\.0\.1:\d+\/reset: \S+.* \(tried 3 times\)\. The site may block/);
  });

  it('caches a page for a while, keyed by language, and sends Accept-Language', async () => {
    const languages: string[] = [];
    const base = await serve((req, res) => {
      languages.push(String(req.headers['accept-language']));
      res.writeHead(200, { 'content-type': 'text/plain' });
      res.end(`visit ${languages.length}`);
    });
    expect(await fetchUrl(`${base}/c`, undefined, privateOk)).toBe('visit 1');
    expect(await fetchUrl(`${base}/c`, undefined, privateOk)).toBe('visit 1');
    expect(await fetchUrl(`${base}/c`, undefined, { ...privateOk, OCTOCODE_WEB_LANGUAGE: 'he-IL' })).toBe('visit 2');
    expect(languages).toEqual(['en-US,en;q=0.9', 'he-IL']);
  });

  it('follows same-host redirects and reports ones to another host', async () => {
    const base = await serve((req, res) => {
      if (req.url === '/old') {
        res.writeHead(301, { location: '/new' });
        return void res.end();
      }
      if (req.url === '/away') {
        res.writeHead(302, { location: 'https://elsewhere.example/landing' });
        return void res.end();
      }
      res.writeHead(200, { 'content-type': 'text/plain' });
      res.end('new place');
    });
    expect(await fetchUrl(`${base}/old`, undefined, privateOk)).toBe('new place');
    const away = await fetchUrl(`${base}/away`, undefined, privateOk);
    expect(away).toMatch(/redirects \(HTTP 302\) to another host: https:\/\/elsewhere\.example\/landing/);
    expect(away).toContain('server-supplied and not verified');
    expect(unreadReason(away)).toBe('redirect');
  });

  it('marks static HTML, warns about JavaScript apps and shows page metadata first', async () => {
    const base = await serve((_req, res) => {
      res.writeHead(200, { 'content-type': 'text/html' });
      res.end(
        '<html lang="en"><head><title>Plans</title><meta property="og:price:amount" content="17"><script type="application/ld+json">{"@context":"https://schema.org","@type":"Offer","price":"17","priceCurrency":"USD"}</script>' +
          '<script id="wix-warmup-data">{}</script></head><body><main>' + '<p>Business plan with everything you need to grow.</p>'.repeat(20) + '<p hidden>$159 .77 placeholder</p></main></body></html>',
      );
    });
    const page = await fetchUrl(`${base}/plans`, undefined, privateOk);
    expect(page).toMatch(/^# Plans\nSource: http:\/\/127\.0\.0\.1:\d+\/plans · static HTML · lang en\n\nNote: static HTML of a JavaScript app/);
    expect(page).toContain('## Page metadata (from the HTML; untrusted)\nog:price:amount: 17\nJSON-LD: {"@type":"Offer","price":"17","priceCurrency":"USD"}');
    expect(page).not.toContain('$159');
  });

  it('refuses private addresses unless allowed', async () => {
    const base = await serve((_req, res) => res.end('secret'));
    await expect(fetchUrl(`${base}/`, undefined, {})).rejects.toThrow(/Blocked private/);
  });
});

describe('web tool', () => {
  it('rejects ambiguous or empty requests before making a network call', async () => {
    const tool = webTool();
    const fetch = vi.fn(() => { throw new Error('unexpected network call'); });
    vi.stubGlobal('fetch', fetch);
    await expect(tool.execute('id', { url: 'https://example.com', query: 'different source' })).rejects.toThrow('Provide exactly one of url or query.');
    await expect(tool.execute('id', { query: '   ' })).rejects.toThrow('Provide url or query.');
    expect(fetch).not.toHaveBeenCalled();
  });
  beforeEach(() => {
    for (const key of ['TAVILY_API_KEY', 'SERPER_API_KEY', 'EXA_API_KEY', 'BRAVE_API_KEY']) vi.stubEnv(key, '');
  });

  it('fetches a URL through execute and needs url or query', async () => {
    vi.stubEnv('OCTOCODE_WEB_ALLOW_PRIVATE', '1');
    const base = await serve((_req, res) => {
      res.writeHead(200, { 'content-type': 'text/plain' });
      res.end('hello from the page');
    });
    const tool = webTool();
    expect(text(await tool.execute('id', { url: `${base}/` }))).toBe('hello from the page');
    await expect(tool.execute('id', {})).rejects.toThrow('Provide url or query.');
  });

  it('returns 20 KB of a page by default, more with maxChars, and says how to get the rest', async () => {
    vi.stubEnv('OCTOCODE_WEB_ALLOW_PRIVATE', '1');
    vi.stubEnv('OCTOCODE_HOME', tmp());
    const page = `\u001b]8;;http://x\u0007${'x'.repeat(80_000)}\u202e`;
    const base = await serve((_req, res) => {
      res.writeHead(200, { 'content-type': 'text/plain' });
      res.end(page);
    });
    const tool = webTool();
    const first = text(await tool.execute('id', { url: `${base}/` }));
    // A longer page is saved whole: the rest is read from the file, not refetched.
    expect(first).toMatch(/^x{20000}\n\[… 60000 more characters cut; the whole page \(1 lines, links included\) is in \S+: search it or read it by line range\]$/);
    const saved = /is in (\S+):/.exec(first)![1]!;
    expect(fs.readFileSync(saved, 'utf8')).toBe('x'.repeat(80_000));
    const more = text(await tool.execute('id', { url: `${base}/`, maxChars: 40_000 }));
    expect(more.startsWith('x'.repeat(40_000) + '\n[… 40000 more')).toBe(true);
    const most = text(await tool.execute('id', { url: `${base}/`, maxChars: 1e9 }));
    expect(most).toMatch(/30000 more characters cut; the whole page .* is in /);
    const blocker = `${tmp()}/file`;
    fs.writeFileSync(blocker, '');
    vi.stubEnv('OCTOCODE_HOME', blocker);
    expect(text(await tool.execute('id', { url: `${base}/`, maxChars: 1e9 }))).toMatch(/30000 more characters cut; call web again with a larger maxChars \(up to 50000\)\]$/);
  });

  it('searches through each configured provider with its own request shape', async () => {
    const requests: Array<{ url: string; headers: Record<string, string>; body: unknown }> = [];
    vi.stubGlobal('fetch', async (url: string, init: RequestInit) => {
      requests.push({ url, headers: init.headers as Record<string, string>, body: init.body ? JSON.parse(String(init.body)) : undefined });
      const host = new URL(url).host;
      const data =
        host === 'api.tavily.com'
          ? { results: [{ title: 'T', url: 'https://t.example', content: 'tavily snippet' }, { title: 'Bad', url: 'javascript:alert(1)' }, { title: 'None' }, 'junk'] }
          : host === 'google.serper.dev'
            ? { organic: [{ title: 'S', link: 'https://s.example', snippet: 'serper snippet' }] }
            : host === 'api.search.brave.com'
              ? { web: { results: [{ title: 'B', url: 'https://b.example', description: 'brave <strong>snippet</strong>' }] } }
              : { results: [{ title: 'E', url: 'https://e.example', text: 'exa snippet' }] };
      return new Response(JSON.stringify(data), { status: 200, headers: { 'content-type': 'application/json' } });
    });
    const tool = webTool();

    vi.stubEnv('TAVILY_API_KEY', 'tv');
    expect(text(await tool.execute('id', { query: 'pi agent', maxResults: 2.7 }))).toBe('1. T\n   https://t.example\n   tavily snippet');
    expect(requests.at(-1)).toMatchObject({ url: 'https://api.tavily.com/search', headers: { authorization: 'Bearer tv' }, body: { query: 'pi agent', max_results: 2 } });

    vi.stubEnv('TAVILY_API_KEY', '');
    vi.stubEnv('SERPER_API_KEY', 'sp');
    expect(text(await tool.execute('id', { query: 'q', maxResults: 100 }))).toContain('serper snippet');
    expect(requests.at(-1)).toMatchObject({ url: 'https://google.serper.dev/search', headers: { 'x-api-key': 'sp' }, body: { q: 'q', num: 20 } });

    vi.stubEnv('SERPER_API_KEY', '');
    vi.stubEnv('EXA_API_KEY', 'ex');
    expect(text(await tool.execute('id', { query: 'q', maxResults: 0 }))).toContain('exa snippet');
    expect(requests.at(-1)).toMatchObject({ url: 'https://api.exa.ai/search', headers: { 'x-api-key': 'ex' }, body: { numResults: 1 } });

    vi.stubEnv('EXA_API_KEY', '');
    vi.stubEnv('BRAVE_API_KEY', 'br');
    expect(text(await tool.execute('id', { query: 'q b', maxResults: 3 }))).toBe('1. B\n   https://b.example\n   brave snippet');
    expect(requests.at(-1)).toMatchObject({ url: 'https://api.search.brave.com/res/v1/web/search?q=q%20b&count=3', headers: { 'x-subscription-token': 'br' } });
  });

  it('falls through the keyless pages, reporting empty results, rate limits and provider errors', async () => {
    const ddg = '<div class="result__body"><a class="result__a" href="//duckduckgo.com/l/?uddg=https%3A%2F%2Fexample.com%2Fa">Example</a><a class="result__snippet" href="#">An example</a></div>';
    const bing = `<li class="b_algo"><h2><a href="https://www.bing.com/ck/a?!&amp;&amp;p=x&amp;u=a1${Buffer.from('https://example.com/b').toString('base64url')}&amp;ntb=1">Bing hit</a></h2><p class="b_lineclamp2">From bing</p></li>`;
    const status: Record<string, number> = {};
    const body: Record<string, string> = { 'html.duckduckgo.com': ddg, 'www.bing.com': bing, 'lite.duckduckgo.com': '' };
    vi.stubGlobal('fetch', async (url: string) => {
      const host = new URL(url).host;
      if (host === 'api.tavily.com') return new Response('bad key', { status: 401 });
      return new Response(body[host] ?? '', { status: status[host] ?? 200 });
    });
    const tool = webTool();
    expect(text(await tool.execute('id', { query: 'example' }))).toBe('1. Example\n   https://example.com/a\n   An example');

    status['html.duckduckgo.com'] = 202;
    expect(text(await tool.execute('id', { query: 'example' }))).toBe('1. Bing hit\n   https://example.com/b\n   From bing\n\n(via bing; skipped duckduckgo: HTTP 202 from html.duckduckgo.com (rate limited))');

    body['www.bing.com'] = '<html>nothing</html>';
    expect(text(await tool.execute('id', { query: 'example' }))).toBe('No results (bing, duckduckgo-lite). (skipped duckduckgo: HTTP 202 from html.duckduckgo.com (rate limited))');

    status['www.bing.com'] = 429;
    status['lite.duckduckgo.com'] = 202;
    await expect(tool.execute('id', { query: 'example' })).rejects.toThrow(/^Search failed: every provider failed \(duckduckgo: HTTP 202 .*; bing: HTTP 429 .*; duckduckgo-lite: HTTP 202 .*\)\. Set TAVILY_API_KEY/);

    delete status['html.duckduckgo.com'];
    vi.stubEnv('TAVILY_API_KEY', 'tv');
    const outcome = await search('x', 3, process.env);
    expect(outcome).toMatchObject({ provider: 'duckduckgo', failures: ['tavily: HTTP 401 from api.tavily.com'] });
  });

  it('renders the call as the URL or the search query', () => {
    initTheme('dark');
    const tool = webTool();
    const line = (args: Record<string, unknown>) => tool.renderCall(args, theme, { lastComponent: undefined }).render(200).join('\n');
    expect(line({ url: 'https://example.com' })).toContain('Web(https://example.com)');
    expect(line({ query: 'pi' })).toContain('Web(search "pi")');
    expect(line({})).toContain('Web');
    const render = (result: Record<string, unknown>, context: Record<string, unknown> = {}) =>
      tool.renderResult({ content: [], ...result }, { expanded: false }, theme, { lastComponent: undefined, isError: false, ...context }).render(200).join('\n');
    expect(render({ content: [{ type: 'text', text: 'one\ntwo' }] }, { expanded: true })).toContain('two');
    const page = render({ content: [{ type: 'text', text: 'a\nb\nc\nd\ne' }], details: { kind: 'fetch', bytes: 12_288, lines: 240, file: '/tmp/page.txt', durationMs: 5 } });
    expect(page).toContain('⎿  Fetched 12.0KB · 240 lines');
    expect(page).toContain('… +2 lines');
    expect(page).not.toContain('saved:');
    expect(render({ content: [{ type: 'text', text: 'a\nb\nc\nd\ne' }], details: { kind: 'fetch', bytes: 12_288, lines: 240, file: '/tmp/page.txt' } }, { expanded: true })).toContain('saved: /tmp/page.txt');
    expect(render({ content: [{ type: 'text', text: 'x' }], details: { kind: 'search', provider: 'duckduckgo', results: 8 } })).toContain('⎿  8 results · duckduckgo');
    expect(render({ content: [{ type: 'text', text: 'No results (bing).' }], details: { kind: 'search', provider: 'bing', results: 0 } })).toContain('No results · bing');
    expect(render({ content: [{ type: 'text', text: 'Search failed: offline\nmore' }] }, { isError: true })).toContain('⎿  Search failed: offline');
  });

  it('records the page size, search provider and duration in details', async () => {
    vi.stubEnv('OCTOCODE_WEB_ALLOW_PRIVATE', '1');
    const base = await serve((_req, res) => {
      res.writeHead(200, { 'content-type': 'text/plain' });
      res.end('line one\nline two');
    });
    const tool = webTool();
    const fetched = (await tool.execute('1', { url: `${base}/` })) as unknown as { details: Record<string, unknown> };
    expect(fetched.details).toMatchObject({ kind: 'fetch', bytes: 17, lines: 2 });
    expect(typeof fetched.details['durationMs']).toBe('number');
    expect(webSummary(fetched.details)).toBe('Fetched 17B · 2 lines');
    expect(webSummary(undefined)).toBeUndefined();
    expect(webSummary({ kind: 'other' })).toBeUndefined();
    expect(webSummary({ kind: 'notice', reason: 'redirect' })).toBe('Not read: redirects to another host (not followed)');
    expect(webSummary({ kind: 'notice', reason: 'unsupported' })).toBe('Not read: not a text document');
  });
});

describe('search provider chain', () => {
  const json = (data: unknown, status = 200) => new Response(JSON.stringify(data), { status, headers: { 'content-type': 'application/json' } });
  const ddgHit = '<div class="result__body"><a class="result__a" href="//duckduckgo.com/l/?uddg=https%3A%2F%2Fd.example%2F">D</a><a class="result__snippet">ddg</a></div>';
  /** Answers each host from `routes`; a host not listed is a network error. */
  const route = (routes: Record<string, () => Response>) => {
    const hosts: string[] = [];
    vi.stubGlobal('fetch', async (url: string) => {
      const host = new URL(url).host;
      hosts.push(host);
      const answer = routes[host];
      if (!answer) throw new Error(`connect ECONNREFUSED ${host}`);
      return answer();
    });
    return hosts;
  };

  it('passes a failing keyed provider to the next keyed one', async () => {
    const hosts = route({
      'api.tavily.com': () => new Response('down', { status: 500 }),
      'google.serper.dev': () => json({ organic: [{ title: 'S', link: 'https://s.example', snippet: 'serper' }] }),
    });
    const outcome = await search('q', 3, { TAVILY_API_KEY: 't', SERPER_API_KEY: 's' });
    expect(outcome).toEqual({ provider: 'serper', hits: [{ title: 'S', url: 'https://s.example', snippet: 'serper' }], failures: ['tavily: HTTP 500 from api.tavily.com'] });
    expect(hosts).toEqual(['api.tavily.com', 'google.serper.dev']);
  });

  it('falls through a keyed provider with no hits, as a keyless one does, and reports it', async () => {
    route({
      'api.tavily.com': () => json({ results: [] }),
      // A malformed answer (results not a list) counts as no hits too.
      'api.exa.ai': () => json({ results: 'nope' }),
      'html.duckduckgo.com': () => new Response(ddgHit),
    });
    const outcome = await search('q', 3, { TAVILY_API_KEY: 't', EXA_API_KEY: 'e' });
    expect(outcome).toEqual({ provider: 'duckduckgo', hits: [{ title: 'D', url: 'https://d.example/', snippet: 'ddg' }], failures: ['tavily: no results', 'exa: no results'] });
  });

  it('names every provider that came back empty when none had hits', async () => {
    route({ 'api.tavily.com': () => json({ results: [] }), 'html.duckduckgo.com': () => new Response(''), 'www.bing.com': () => new Response(''), 'lite.duckduckgo.com': () => new Response('') });
    expect(await search('q', 3, { TAVILY_API_KEY: 't' })).toEqual({ provider: 'tavily, duckduckgo, bing, duckduckgo-lite', hits: [], failures: [] });
  });

  it('reports keyed and keyless failures (bad JSON, network errors) when every provider failed', async () => {
    route({ 'api.search.brave.com': () => new Response('<html>not json</html>') });
    await expect(search('q', 3, { BRAVE_API_KEY: 'b' })).rejects.toThrow(
      /^every provider failed \(brave: .*JSON.*; duckduckgo: connect ECONNREFUSED html\.duckduckgo\.com; bing: connect ECONNREFUSED www\.bing\.com; duckduckgo-lite: connect ECONNREFUSED lite\.duckduckgo\.com\)\. Set TAVILY_API_KEY/,
    );
  });

  it('stops at once when the call is aborted, without trying the next provider', async () => {
    const controller = new AbortController();
    const hosts: string[] = [];
    vi.stubGlobal('fetch', async (url: string) => {
      hosts.push(new URL(url).host);
      controller.abort(new Error('stopped'));
      throw new Error('aborted fetch');
    });
    await expect(search('q', 3, { TAVILY_API_KEY: 't' }, controller.signal)).rejects.toThrow('aborted fetch');
    expect(hosts).toEqual(['api.tavily.com']);
    hosts.length = 0;
    await expect(search('q', 3, {}, controller.signal)).rejects.toThrow('aborted fetch');
    expect(hosts).toEqual(['html.duckduckgo.com']);
  });
});

describe('search result parsing', () => {
  it('skips DuckDuckGo blocks without a result link, stops at max, and keeps only http(s) targets', async () => {
    const { parseDuckDuckGo } = await import('../src/web/search.js');
    const block = (href: string, title: string) => `<div class="result__body"><a class="result__a" href="${href}">${title}</a></div>`;
    const html = [
      '<div class="result__body"><span>ad, no link</span></div>',
      block('//duckduckgo.com/l/?uddg=javascript%3Aalert(1)', 'Script'),
      block('data:text/html,hi', 'Data'),
      block('http://[broken', 'Broken'),
      block('/l/?kh=1', 'Relative'),
      block('//duckduckgo.com/l/?uddg=http%3A%2F%2Fone.example%2F', 'One'),
      block('https://two.example/', 'Two'),
    ].join('');
    expect(parseDuckDuckGo(html, 5)).toEqual([
      { title: 'Relative', url: 'https://duckduckgo.com/l/?kh=1', snippet: '' },
      { title: 'One', url: 'http://one.example/', snippet: '' },
      { title: 'Two', url: 'https://two.example/', snippet: '' },
    ]);
    expect(parseDuckDuckGo(html, 2).map((hit) => hit.title)).toEqual(['Relative', 'One']);
  });

  it('keeps only http(s) targets from Bing and DuckDuckGo lite', async () => {
    const { parseBing, parseDuckDuckGoLite } = await import('../src/web/search.js');
    const tracker = `https://www.bing.com/ck/a?u=a1${Buffer.from('javascript:alert(1)').toString('base64url')}`;
    const bing = [`<li class="b_algo"><h2><a href="${tracker}">Tracker</a></h2></li>`, '<li class="b_algo"><h2><a href="javascript:void(0)">Script</a></h2></li>', '<li class="b_algo"><h2><a href="not a url">Bad</a></h2></li>'].join('');
    // A tracker hiding a non-web target keeps the (https) tracker link; a non-web or unparsable link is dropped.
    expect(parseBing(bing, 5)).toEqual([{ title: 'Tracker', url: tracker, snippet: '' }]);
    const lite = `<a class="result-link" href="//duckduckgo.com/l/?uddg=javascript%3Aalert(1)">Script</a><a class='result-link'>No href</a><a class="result-link" href="https://ok.example/">Ok</a>`;
    expect(parseDuckDuckGoLite(lite, 5)).toEqual([{ title: 'Ok', url: 'https://ok.example/', snippet: '' }]);
  });
});
