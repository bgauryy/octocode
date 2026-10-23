import fs from 'node:fs';
import path from 'node:path';
import { initTheme } from '@earendil-works/pi-coding-agent';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { PublicFetchInit } from '../src/web/guard.js';
import { search } from '../src/web/search.js';
import { webSummary } from '../src/web/web.js';
import { theme } from './fake-pi.js';
import { tmp } from './helpers.js';
import { serve, text, useWebServers, webTool } from './web-harness.js';

/** Search provider calls go through `publicFetch`; a case sets `guard.stub` to answer them without a network. */
const guard = vi.hoisted(() => ({ stub: undefined as undefined | ((url: string, init: PublicFetchInit) => Promise<Response>) }));
vi.mock('../src/web/guard.js', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../src/web/guard.js')>();
  return { ...actual, publicFetch: ((url, init, options) => (guard.stub ? guard.stub(url, init) : actual.publicFetch(url, init, options))) as typeof actual.publicFetch };
});

useWebServers();
afterEach(() => {
  guard.stub = undefined;
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
    const base = await serve((_req, res) => {
      res.writeHead(200, { 'content-type': 'text/plain' });
      res.end('hello from the page');
    });
    const tool = webTool();
    expect(text(await tool.execute('id', { url: `${base}/` }))).toBe('hello from the page');
    await expect(tool.execute('id', {})).rejects.toThrow('Provide url or query.');
  });

  it('names the Octocode tools this session has for reading the saved page, clasify first', async () => {
    vi.stubEnv('OCTOCODE_HOME', tmp());
    const base = await serve((_req, res) => {
      res.writeHead(200, { 'content-type': 'text/plain' });
      res.end('y'.repeat(30_000));
    });
    const call = async (active: string[]) => text(await webTool(active).execute('id', { url: `${base}/`, maxChars: 1_000 }));
    const all = await call(['mcp__octocode__clasify', 'mcp__octocode__localSearch', 'mcp__octocode__localFetch']);
    expect(all).toMatch(/is in \/\S+: screen a bounded unread selection with mcp__octocode__clasify \(source \{tool:"localFetch", query:\{path:<this file>, ranges:<observed line ranges>\}\}, a noul question about one stated fact; typed answers only, read deciding evidence\); search literal text with mcp__octocode__localSearch \(headings: regex "\^#\{1,3\} "\); read line ranges with mcp__octocode__localFetch\]$/);
    const saved = /is in (\/\S+):/.exec(all)![1]!;
    expect(fs.readFileSync(saved, 'utf8')).toBe('y'.repeat(30_000));
    // Without the classification provider the local tools remain; without Octocode tools the generic wording stands.
    expect(await call(['mcp__octocode__localSearch', 'mcp__octocode__localFetch'])).toMatch(/is in \/\S+: search literal text with mcp__octocode__localSearch \(headings: regex "\^#\{1,3\} "\); read line ranges with mcp__octocode__localFetch\]$/);
    expect(await call(['read', 'bash'])).toMatch(/is in \S+: search it or read it by line range\]$/);
  });

  it('returns 20 KB of a page by default, more with maxChars, and says how to get the rest', async () => {
    vi.stubEnv('OCTOCODE_HOME', tmp());
    const page = 'x'.repeat(80_000);
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
    expect(text(await tool.execute('id', { url: `${base}/`, maxChars: 1e9 }))).toMatch(
      /^x{50000}\n\[\u2026 30000 more characters cut; the whole page could not be saved to a file; the last 30000 characters are beyond web's 50000-character limit: download the page with bash \(curl -o\) to read them\]$/,
    );
    expect(text(await tool.execute('id', { url: `${base}/` }))).toMatch(/60000 more characters cut; the whole page could not be saved to a file; maxChars 50000 shows up to 50000; the last 30000 characters are beyond/);
  });

  it('shows hidden characters of a page as visible escapes, in the preview and in the saved page', async () => {
    vi.stubEnv('OCTOCODE_HOME', tmp());
    const page = `\u001b]8;;http://x\u0007link\u202e${'x'.repeat(30_000)}\u200b end\rlone\r\ncrlf`;
    const base = await serve((_req, res) => {
      res.writeHead(200, { 'content-type': 'text/plain' });
      res.end(page);
    });
    const tool = webTool();
    const first = text(await tool.execute('id', { url: `${base}/`, maxChars: 1_000 }));
    expect(first.startsWith('\\u{1b}]8;;http://x\\u{7}link\\u{202e}xxx')).toBe(true);
    expect(first).not.toMatch(/[\u001b\u0007\u202e\u200b]/u);
    const saved = fs.readFileSync(/is in (\S+):/.exec(first)![1]!, 'utf8');
    expect(saved).toBe(page.replace('\u001b', '\\u{1b}').replace('\u0007', '\\u{7}').replace('\u202e', '\\u{202e}').replace('\u200b', '\\u{200b}').replace('\rlone', '\\rlone'));
    const small = text(await tool.execute('id', { url: `${base}/x`, maxChars: 50_000 }));
    expect(small).toContain('\\u{200b} end\\rlone\r\ncrlf');
  });

  it('shows hidden characters in search results as visible escapes', async () => {
    guard.stub = async () => new Response(JSON.stringify({ results: [{ title: 'T\u202eitle', url: 'https://t.example', content: 'snip\u001b[31mpet\u200b' }] }), { status: 200, headers: { 'content-type': 'application/json' } });
    vi.stubEnv('TAVILY_API_KEY', 'tv');
    expect(text(await webTool().execute('id', { query: 'q' }))).toBe('1. T\\u{202e}itle\n   https://t.example\n   snip\\u{1b}[31mpet\\u{200b}');
  });

  it('keeps a multi-byte page preview within the output budget, so only the page itself is saved', async () => {
    const home = tmp();
    vi.stubEnv('OCTOCODE_HOME', home);
    const page = 'é'.repeat(80_000);
    const base = await serve((_req, res) => {
      res.writeHead(200, { 'content-type': 'text/plain; charset=utf-8' });
      res.end(page);
    });
    const out = text(await webTool().execute('id', { url: `${base}/`, maxChars: 50_000 }));
    // 50,000 characters are 100 KB: the preview is cut to the budget and says how much of the page it left out.
    expect(Buffer.byteLength(out)).toBeLessThanOrEqual(51_200);
    expect(out).not.toContain('Output trimmed');
    expect(out).not.toContain('Full output:');
    const [saved, ...other] = [...out.matchAll(/is in (\S+):/g)].map((match) => match[1]!);
    expect(other).toEqual([]);
    expect(fs.readFileSync(saved!, 'utf8')).toBe(page);
    const shown = /^(é+)\n\[… (\d+) more characters cut/.exec(out)!;
    expect(shown[1]!.length + Number(shown[2])).toBe(80_000);
    const files = fs.readdirSync(path.dirname(saved!)).filter((name) => name.startsWith('web-'));
    expect(files).toHaveLength(1);
  });

  it('searches through each configured provider with its own request shape', async () => {
    const requests: Array<{ url: string; headers: Record<string, string>; body: unknown }> = [];
    guard.stub = async (url, init) => {
      requests.push({ url, headers: init.headers ?? {}, body: init.body ? JSON.parse(init.body) : undefined });
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
    };
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
    guard.stub = async (url: string) => {
      const host = new URL(url).host;
      if (host === 'api.tavily.com') return new Response('bad key', { status: 401 });
      return new Response(body[host] ?? '', { status: status[host] ?? 200 });
    };
    const tool = webTool();
    expect(text(await tool.execute('id', { query: 'example' }))).toBe('1. Example\n   https://example.com/a\n   An example');

    status['html.duckduckgo.com'] = 202;
    expect(text(await tool.execute('id', { query: 'example' }))).toBe('1. Bing hit\n   https://example.com/b\n   From bing\n\n(via bing; skipped duckduckgo: HTTP 202 from html.duckduckgo.com (rate limited))');

    body['www.bing.com'] = '<html>nothing</html>';
    expect(text(await tool.execute('id', { query: 'example' }))).toBe('No results (bing, duckduckgo-lite). (skipped duckduckgo: HTTP 202 from html.duckduckgo.com (rate limited))');

    status['www.bing.com'] = 429;
    status['lite.duckduckgo.com'] = 202;
    await expect(tool.execute('id', { query: 'example' })).rejects.toThrow(/^Search failed: every provider failed \(duckduckgo: HTTP 202 .*; bing: HTTP 429 .*; duckduckgo-lite: HTTP 202 .*\)\. Do not repeat the same search: fetch a URL you already know with `web`, or ask the user to set TAVILY_API_KEY/);

    delete status['html.duckduckgo.com'];
    vi.stubEnv('TAVILY_API_KEY', 'tv');
    const outcome = await search('x', 3, process.env);
    expect(outcome).toMatchObject({ provider: 'duckduckgo', failures: ['tavily: HTTP 401 from api.tavily.com'] });
  });

  it('shows control characters in a failed search or fetch as visible escapes, and keeps the error type', async () => {
    guard.stub = async () => {
      throw new Error('proxy said \u001b[2J\u202eevil');
    };
    const tool = webTool();
    const failure = (params: Record<string, unknown>) => tool.execute('id', params).then(() => new Error('did not fail'), (error: unknown) => error as Error);
    const search = await failure({ query: 'example' });
    expect(search).toBeInstanceOf(Error);
    expect(search.message).toMatch(/^Search failed: .*\\u\{1b\}\[2J\\u\{202e\}evil/);
    expect(search.message).not.toMatch(/[\u001b\u202e]/);
    const fetched = await failure({ url: 'https://example.com/\u001b[2J' });
    expect(fetched.message).not.toMatch(/[\u001b\u202e]/);
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

  it('records the page size and search provider without duplicating Pi execution timing', async () => {
    const base = await serve((_req, res) => {
      res.writeHead(200, { 'content-type': 'text/plain' });
      res.end('line one\nline two');
    });
    const tool = webTool();
    const fetched = (await tool.execute('1', { url: `${base}/` })) as unknown as { details: Record<string, unknown> };
    expect(fetched.details).toMatchObject({ kind: 'fetch', bytes: 17, lines: 2 });
    expect(fetched.details).not.toHaveProperty('durationMs');
    expect(webSummary(fetched.details)).toBe('Fetched 17B · 2 lines');
    expect(webSummary(undefined)).toBeUndefined();
    expect(webSummary({ kind: 'other' })).toBeUndefined();
    expect(webSummary({ kind: 'notice', reason: 'redirect' })).toBe('Not read: redirects to another host (not followed)');
    expect(webSummary({ kind: 'notice', reason: 'unsupported' })).toBe('Not read: not a text document');
  });
});

