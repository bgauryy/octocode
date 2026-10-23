import { afterEach, describe, expect, it, vi } from 'vitest';
import type * as guardModule from '../src/web/guard.js';
import type { PublicFetchInit } from '../src/web/guard.js';
import { search } from '../src/web/search.js';

/** Search provider calls go through `publicFetch`; a case sets `guard.stub` to answer them without a network. */
const guard = vi.hoisted(() => ({ stub: undefined as undefined | ((url: string, init: PublicFetchInit) => Promise<Response>) }));
vi.mock('../src/web/guard.js', async (importOriginal) => {
  const actual = await importOriginal<typeof guardModule>();
  return { ...actual, publicFetch: ((url, init, options) => (guard.stub ? guard.stub(url, init) : actual.publicFetch(url, init, options))) as typeof actual.publicFetch };
});

afterEach(() => {
  guard.stub = undefined;
});

const json = (data: unknown, status = 200) => new Response(JSON.stringify(data), { status, headers: { 'content-type': 'application/json' } });
/** Answers each host from `routes`; a host not listed is a network error. */
const route = (routes: Record<string, () => Response>) => {
  const hosts: string[] = [];
  guard.stub = async (url: string) => {
    const host = new URL(url).host;
    hosts.push(host);
    const answer = routes[host];
    if (!answer) throw new Error(`connect ECONNREFUSED ${host}`);
    return answer();
  };
  return hosts;
};
const block = (href: string, title: string) => `<div class="result__body"><a class="result__a" href="${href}">${title}</a></div>`;

describe('search provider chain', () => {
  const ddgHit = '<div class="result__body"><a class="result__a" href="//duckduckgo.com/l/?uddg=https%3A%2F%2Fd.example%2F">D</a><a class="result__snippet">ddg</a></div>';

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
      /^every provider failed \(brave: .*JSON.*; duckduckgo: connect ECONNREFUSED html\.duckduckgo\.com; bing: connect ECONNREFUSED www\.bing\.com; duckduckgo-lite: connect ECONNREFUSED lite\.duckduckgo\.com\)\. Do not repeat the same search: fetch a URL you already know with `web`, or ask the user to set TAVILY_API_KEY/,
    );
  });

  it('stops at once when the call is aborted, without trying the next provider', async () => {
    const controller = new AbortController();
    const hosts: string[] = [];
    guard.stub = async (url: string) => {
      hosts.push(new URL(url).host);
      controller.abort(new Error('stopped'));
      throw new Error('aborted fetch');
    };
    await expect(search('q', 3, { TAVILY_API_KEY: 't' }, controller.signal)).rejects.toThrow('aborted fetch');
    expect(hosts).toEqual(['api.tavily.com']);
    hosts.length = 0;
    await expect(search('q', 3, {}, controller.signal)).rejects.toThrow('aborted fetch');
    expect(hosts).toEqual(['html.duckduckgo.com']);
  });
});

describe('search through the guarded request', () => {
  it('posts to keyed APIs without following redirects, and sends the environment to the guard', async () => {
    const calls: Array<{ url: string; method?: string; redirect?: string }> = [];
    guard.stub = async (url, init) => {
      calls.push({ url, method: init.method, redirect: init.redirect });
      return json({ results: [{ title: 'T', url: 'https://t.example', content: 'c' }], web: { results: [] } });
    };
    await search('q', 3, { TAVILY_API_KEY: 't' });
    expect(calls).toEqual([{ url: 'https://api.tavily.com/search', method: 'POST', redirect: 'error' }]);
    calls.length = 0;
    guard.stub = async (url, init) => {
      calls.push({ url, method: init.method, redirect: init.redirect });
      return json({ web: { results: [{ title: 'B', url: 'https://b.example', description: 'd' }] } });
    };
    await search('q', 3, { BRAVE_API_KEY: 'b' });
    expect(calls).toEqual([{ url: 'https://api.search.brave.com/res/v1/web/search?q=q&count=3', method: undefined, redirect: 'error' }]);
  });

  it('reports a keyed API redirect or an oversized answer as that provider failing, then falls through', async () => {
    guard.stub = async (url) => {
      if (url.includes('tavily')) throw new Error('api.tavily.com answered with a redirect (HTTP 302); it is not followed');
      if (url.includes('serper')) return new Response('{"organic":"' + 'x'.repeat(6 * 1024 * 1024) + '"}', { status: 200 });
      return new Response('', { status: 200 });
    };
    const outcome = await search('q', 3, { TAVILY_API_KEY: 't', SERPER_API_KEY: 's' });
    expect(outcome.failures.slice(0, 2)).toEqual(['tavily: api.tavily.com answered with a redirect (HTTP 302); it is not followed', 'serper: the answer from google.serper.dev is over the size limit']);
  });
});

describe('search result parsing', () => {
  it('skips DuckDuckGo blocks without a result link, stops at max, and keeps only http(s) targets', async () => {
    const { parseDuckDuckGo } = await import('../src/web/search.js');
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
