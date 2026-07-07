import assert from 'node:assert/strict';
import { test } from 'vitest';
import {
  isBlockedIp,
  assertPublicUrl,
  safeFetch,
  htmlToText,
  extractTitle,
  decodeEntities,
  unwrapDdgHref,
  parseDuckDuckGo,
  runWebTool,
  renderWebResult,
  pickProvider,
  normalizeApiKey,
  tavilySearch,
  serperSearch,
  resolveUserAgent,
  DEFAULT_USER_AGENT,
  DEFAULT_SEC_CH_UA,
  readCapped,
  createDeadline,
  postJson,
  webFetch,
  duckDuckGoSearch,
  webSearch,
  type WebFetchResult,
} from '../src/web.js';

// ── test helpers ──────────────────────────────────────────────────────────────

const publicLookup = async (_h: string) => [{ address: '8.8.8.8', family: 4 }];

interface MockResponse {
  ok: boolean;
  status: number;
  headers: Map<string, string>;
  body: null | object;
  text?: () => Promise<string>;
  json?: () => Promise<unknown>;
}

const textRes = (body: string, opts: { status?: number; ct?: string } = {}): MockResponse => ({
  ok: (opts.status ?? 200) >= 200 && (opts.status ?? 200) < 300,
  status: opts.status ?? 200,
  headers: new Map([['content-type', opts.ct ?? 'text/html']]),
  body: null,
  text: async () => body,
});

const jsonRes = (obj: unknown, opts: { status?: number } = {}): MockResponse => ({
  ok: (opts.status ?? 200) >= 200 && (opts.status ?? 200) < 300,
  status: opts.status ?? 200,
  headers: new Map(),
  body: null,
  json: async () => obj,
});

const streamRes = (parts: string[]) => {
  const enc = new TextEncoder();
  const chunks = parts.map((p) => enc.encode(p));
  let i = 0;
  return {
    body: {
      getReader: () => ({
        read: async () =>
          i < chunks.length
            ? { done: false, value: chunks[i++] }
            : { done: true, value: undefined },
        cancel: async () => { /* no-op */ },
      }),
    },
  };
};

// ── tests ─────────────────────────────────────────────────────────────────────

test('isBlockedIp blocks private/loopback/link-local/metadata/ULA/mapped, allows public', () => {
  for (const ip of [
    '127.0.0.1', '10.1.2.3', '192.168.0.1', '172.16.5.5', '169.254.169.254',
    '100.64.0.1', '0.0.0.0', '::1', '::', 'fe80::1', 'fc00::1', '::ffff:127.0.0.1', 'not-an-ip',
  ]) {
    assert.equal(isBlockedIp(ip), true, `${ip} should be blocked`);
  }
  for (const ip of ['8.8.8.8', '1.1.1.1', '140.82.121.4', '2606:4700:4700::1111', '::ffff:8.8.8.8']) {
    assert.equal(isBlockedIp(ip), false, `${ip} should be allowed`);
  }
});

test('assertPublicUrl rejects non-http(s) and hosts resolving to private IPs (DNS rebinding)', async () => {
  await assert.rejects(() => assertPublicUrl('file:///etc/passwd'), /non-http/);
  await assert.rejects(() => assertPublicUrl('ftp://example.com'), /non-http/);
  await assert.rejects(
    () => assertPublicUrl('http://attacker.com/', { lookup: async () => [{ address: '127.0.0.1', family: 4 }] }),
    /blocked address/,
  );
  await assert.rejects(() => assertPublicUrl('http://169.254.169.254/latest/meta-data/'), /private\/loopback/);
  const ok = await assertPublicUrl('https://good.com/x', { lookup: async () => [{ address: '8.8.8.8', family: 4 }] });
  assert.equal(ok.hostname, 'good.com');
});

test('safeFetch re-validates every redirect hop and blocks a redirect into a private IP', async () => {
  const lookup = async () => [{ address: '8.8.8.8', family: 4 }];
  const fetchImpl = async (url: string) => {
    if (url === 'https://evil.com/') {
      return { status: 302, ok: false, headers: new Map([['location', 'http://169.254.169.254/']]) };
    }
    throw new Error('should not reach the private hop');
  };
  await assert.rejects(
    () => safeFetch('https://evil.com/', { fetchImpl: fetchImpl as unknown as typeof globalThis.fetch, lookup, timeoutMs: 1000 }),
    /private\/loopback|blocked address/,
  );
});

test('safeFetch follows a valid redirect and returns the final response', async () => {
  const lookup = async () => [{ address: '8.8.8.8', family: 4 }];
  const fetchImpl = async (url: string) => {
    if (url === 'https://a.com/')
      return { status: 301, ok: false, headers: new Map([['location', 'https://b.com/']]) };
    return { status: 200, ok: true, headers: new Map([['content-type', 'text/html']]), body: null, text: async () => '<title>Hi</title>ok' };
  };
  const { res, finalUrl } = await safeFetch('https://a.com/', {
    fetchImpl: fetchImpl as unknown as typeof globalThis.fetch,
    lookup,
  });
  assert.equal(res.status, 200);
  assert.equal(finalUrl, 'https://b.com/');
});

test('htmlToText / extractTitle / decodeEntities strip markup and decode entities', () => {
  const html = '<title>My &amp; Page</title><body><script>bad()</script><h1>Hi</h1><p>a &lt;b&gt; c&#39;s</p></body>';
  assert.equal(extractTitle(html), 'My & Page');
  const text = htmlToText(html);
  assert.ok(!text.includes('bad()'), 'script stripped');
  assert.ok(text.includes("c's"), 'numeric entity decoded');
  assert.ok(text.includes('Hi'), 'heading kept');
  assert.equal(decodeEntities('a&amp;b&#38;c'), 'a&b&c');
});

test('htmlToText strips aria announcements, BreadcrumbList, and skip-to links', () => {
  const withAnnouncement = '<body><section aria-label="Announcement"><a href="/event">Don\'t miss our event!</a></section><article><p>Real content.</p></article></body>';
  const t1 = htmlToText(withAnnouncement);
  assert.ok(!t1.includes("Don't miss"), 'aria announcement stripped');
  assert.ok(t1.includes('Real content'), 'article kept');

  const withCrumbs = '<body><ol typeof="BreadcrumbList"><li>JS</li><li>Promise</li></ol><main><p>Article here.</p></main></body>';
  const t2 = htmlToText(withCrumbs);
  assert.ok(!t2.includes('JS'), 'breadcrumb item stripped');
  assert.ok(!t2.includes('Promise'), 'breadcrumb item stripped');
  assert.ok(t2.includes('Article here'), 'main content kept');

  const withSkip = '<body><ul><li><a href="#content">Skip to main content</a></li><li><a href="#search">Skip to search</a></li></ul><p>Real article text.</p></body>';
  const t3 = htmlToText(withSkip);
  assert.ok(!t3.includes('Skip to main content'), 'skip-to link stripped');
  assert.ok(!t3.includes('Skip to search'), 'skip-to link stripped');
  assert.ok(t3.includes('Real article text'), 'article kept');

  const withSkipSpace = '<ul class="a11y-menu"><li><a href="#content" data-x="y" >Skip to main content</a > </li></ul><p>Body text.</p>';
  const t3b = htmlToText(withSkipSpace);
  assert.ok(!t3b.includes('Skip to main content'), 'skip-to link with </a > stripped');
  assert.ok(t3b.includes('Body text'), 'body kept');

  const withCustomEl = '<mdn-language-switcher locale="en-US"><ul><li>Deutsch</li><li>Español</li></ul></mdn-language-switcher><mdn-color-theme><span>Dark</span></mdn-color-theme><article><p>Real docs content.</p></article>';
  const t5 = htmlToText(withCustomEl);
  assert.ok(!t5.includes('Deutsch'), 'custom element content stripped');
  assert.ok(!t5.includes('Dark'), 'custom element content stripped');
  assert.ok(t5.includes('Real docs content'), 'article kept');

  const withNormalAnchor = '<p>See <a href="#section-2">Section 2</a> below.</p>';
  const t4 = htmlToText(withNormalAnchor);
  assert.ok(t4.includes('Section 2'), 'normal anchor link text kept');
});

test('htmlToText strips nav, aside, footer chrome but keeps article content', () => {
  const html = `
    <html><body>
      <nav><ul><li><a href="/">Home</a></li><li><a href="/about">About</a></li></ul></nav>
      <aside>Related: <a href="/x">Link</a></aside>
      <article><h1>Real Title</h1><p>Real content here.</p></article>
      <footer>Copyright 2025 Acme Inc. <a href="/privacy">Privacy</a></footer>
    </body></html>`;
  const text = htmlToText(html);
  assert.ok(!text.includes('Home'), 'nav link stripped');
  assert.ok(!text.includes('About'), 'nav link stripped');
  assert.ok(!text.includes('Related:'), 'aside stripped');
  assert.ok(!text.includes('Copyright'), 'footer stripped');
  assert.ok(!text.includes('Privacy'), 'footer link stripped');
  assert.ok(text.includes('Real Title'), 'article heading kept');
  assert.ok(text.includes('Real content here.'), 'article body kept');
});

test('unwrapDdgHref decodes DuckDuckGo redirect wrappers', () => {
  assert.equal(unwrapDdgHref('//duckduckgo.com/l/?uddg=https%3A%2F%2Fexample.com%2Fx'), 'https://example.com/x');
  assert.equal(unwrapDdgHref('https://plain.com/'), 'https://plain.com/');
});

test('parseDuckDuckGo extracts titles, urls, snippets and respects maxResults', () => {
  const html = `
    <a class="result__a" href="//duckduckgo.com/l/?uddg=https%3A%2F%2Fone.com">One Title</a>
    <a class="result__snippet">First snippet</a>
    <a class="result__a" href="//duckduckgo.com/l/?uddg=https%3A%2F%2Ftwo.com">Two Title</a>
    <a class="result__snippet">Second snippet</a>`;
  const r = parseDuckDuckGo(html, 1);
  assert.equal(r.length, 1);
  assert.equal(r[0]!.title, 'One Title');
  assert.equal(r[0]!.url, 'https://one.com');
  assert.equal(r[0]!.snippet, 'First snippet');
});

test('readCapped: an aborted signal short-circuits a hanging body read (no hang)', async () => {
  const hangingRes = {
    body: {
      getReader: () => ({
        read: () => new Promise(() => { /* never resolves */ }),
        cancel: async () => { /* no-op */ },
      }),
    },
  };
  const ac = new AbortController();
  ac.abort();
  const out = await readCapped(hangingRes as unknown as Response, 1000, { signal: ac.signal });
  assert.equal(out.truncated, true, 'aborted read returns truncated instead of hanging');
});

test('createDeadline: fires its signal on timeout and cleans up', async () => {
  const d = createDeadline({ timeoutMs: 5 });
  await new Promise((r) => setTimeout(r, 25));
  assert.equal(d.signal.aborted, true, 'deadline aborts after timeout');
  d.cleanup();
});

test('readCapped: assembles streamed chunks; truncates at maxBytes', async () => {
  const ok = await readCapped(streamRes(['hello ', 'world']) as unknown as Response, 1000);
  assert.equal(ok.text, 'hello world');
  assert.equal(ok.truncated, false);
  const cut = await readCapped(streamRes(['abcdef', 'ghi']) as unknown as Response, 3);
  assert.equal(cut.truncated, true);
});

test('postJson: returns parsed body on 2xx, throws status-bearing error otherwise', async () => {
  const raw = await postJson('u', { body: {}, fetchImpl: async () => jsonRes({ x: 1 }) as unknown as Response });
  assert.equal((raw as { x: number }).x, 1);
  await assert.rejects(
    () => postJson('u', { body: {}, fetchImpl: async () => jsonRes({}, { status: 500 }) as unknown as Response }),
    /HTTP 500/,
  );
});

test('serperSearch: knowledgeGraph description is used as answer when no answerBox', async () => {
  const out = await serperSearch(
    'q',
    { apiKey: 'k' },
    { fetchImpl: async () => jsonRes({ knowledgeGraph: { description: 'KG' }, organic: [] }) as unknown as Response },
  );
  assert.equal(out.answer, 'KG');
});

test('webFetch: HTML → title + readable text; sets final url', async () => {
  const out = await webFetch('https://x.com/', {
    fetchImpl: async () => textRes('<title>Hi</title><body><script>bad()</script><p>Body text</p></body>') as unknown as Response,
    lookup: publicLookup,
  });
  assert.equal(out.title, 'Hi');
  assert.equal(out.url, 'https://x.com/');
  assert.ok(out.text?.includes('Body text'));
  assert.ok(!out.text?.includes('bad()'), 'script stripped');
});

test('webFetch: non-HTML passthrough, HTTP error, and maxChars truncation', async () => {
  const json = await webFetch('https://x.com/d', {
    fetchImpl: async () => textRes('{"a":1}', { ct: 'application/json' }) as unknown as Response,
    lookup: publicLookup,
  });
  assert.equal(json.title, '');
  assert.equal(json.text, '{"a":1}');

  const err = await webFetch('https://x.com/404', {
    fetchImpl: async () => textRes('', { status: 404 }) as unknown as Response,
    lookup: publicLookup,
  });
  assert.match(err.error ?? '', /404/);

  const clip = await webFetch('https://x.com/big', {
    fetchImpl: async () => textRes(`<p>${'a'.repeat(100)}</p>`) as unknown as Response,
    lookup: publicLookup,
    maxChars: 10,
  });
  assert.equal(clip.truncated, true);
  assert.ok((clip.text?.length ?? 0) <= 10);
});

test('webFetch: page pagination slices extracted text and reports truncated + totalChars', async () => {
  const body = 'a'.repeat(30) + ' ' + 'b'.repeat(30) + ' ' + 'c'.repeat(30);
  const html = `<title>T</title><body><p>${body}</p></body>`;
  const fetchImpl = async () => textRes(html) as unknown as Response;

  const p1 = await webFetch('https://x.com/doc', { fetchImpl, lookup: publicLookup, maxChars: 40, page: 1 });
  assert.equal(p1.page, 1);
  assert.equal(p1.text?.length, 40);
  assert.equal(p1.truncated, true, 'more content exists');
  assert.ok((p1.totalChars ?? 0) >= 90, 'totalChars covers full body');

  const p2 = await webFetch('https://x.com/doc', { fetchImpl, lookup: publicLookup, maxChars: 40, page: 2 });
  assert.equal(p2.page, 2);
  assert.equal(p2.text?.length, 40);
  assert.ok(p2.text !== p1.text, 'different content slice');

  const p3 = await webFetch('https://x.com/doc', { fetchImpl, lookup: publicLookup, maxChars: 40, page: 3 });
  assert.equal(p3.page, 3);
  assert.ok((p3.text?.length ?? 0) > 0 && (p3.text?.length ?? 0) <= 40);
  assert.equal(p3.truncated, false, 'last page is not truncated');

  const p99 = await webFetch('https://x.com/doc', { fetchImpl, lookup: publicLookup, maxChars: 40, page: 99 });
  assert.match(p99.text ?? '', /no content at this page offset/);
  assert.equal(p99.truncated, false);
});

test('renderWebResult: shows page label and next-page hint when truncated', () => {
  const r1 = renderWebResult({ url: 'https://x.com', title: 'T', text: 'hello', page: 1, truncated: true });
  assert.match(r1, /page: 2/);
  assert.doesNotMatch(r1, /\[page 1\]/, 'page 1 label omitted');

  const r2 = renderWebResult({ url: 'https://x.com', title: 'T', text: 'hello', page: 2, truncated: true });
  assert.match(r2, /\[page 2\]/);
  assert.match(r2, /page: 3/);

  const r3 = renderWebResult({ url: 'https://x.com', title: 'T', text: 'hello', page: 2, truncated: false });
  assert.doesNotMatch(r3, /page:/);
});

test('duckDuckGoSearch: parses SERP via injected fetch', async () => {
  const html = '<a class="result__a" href="//duckduckgo.com/l/?uddg=https%3A%2F%2Fo.com">OT</a><a class="result__snippet">S</a>';
  const out = await duckDuckGoSearch('q', {
    fetchImpl: async () => textRes(html) as unknown as Response,
    lookup: publicLookup,
    maxResults: 1,
  });
  assert.equal(out.engine, 'duckduckgo');
  assert.deepEqual(out.results![0], { title: 'OT', url: 'https://o.com', snippet: 'S' });
});

test('webSearch: retries without timeRange when provider returns zero results', async () => {
  let calls = 0;
  const fetchImpl = async (_url: string, init?: RequestInit) => {
    calls++;
    const body = JSON.parse((init?.body as string | null) ?? '{}') as { time_range?: string };
    if (body.time_range)
      return { ok: true, json: async () => ({ answer: '', results: [] }) } as unknown as Response;
    return { ok: true, json: async () => ({ answer: 'Fallback answer', results: [{ title: 'T', url: 'u', content: 'c' }] }) } as unknown as Response;
  };
  const out = await webSearch('q', { env: { TAVILY_API_KEY: 'k' }, timeRange: 'month', fetchImpl });
  assert.equal(calls, 2, 'retried once without timeRange');
  assert.equal(out.results?.length, 1, 'fallback results returned');
  assert.equal(out.answer, 'Fallback answer');
});

test('webSearch: does not retry when timeRange returns results', async () => {
  let calls = 0;
  const fetchImpl = async () => {
    calls++;
    return { ok: true, json: async () => ({ answer: 'A', results: [{ title: 'T', url: 'u', content: 'c' }] }) } as unknown as Response;
  };
  const out = await webSearch('q', { env: { TAVILY_API_KEY: 'k' }, timeRange: 'week', fetchImpl });
  assert.equal(calls, 1, 'no retry when results exist');
  assert.equal(out.results?.length, 1);
});

test('webSearch: dispatches to the provider chosen by env, end to end', async () => {
  const tav = await webSearch('q', {
    env: { TAVILY_API_KEY: 'k' },
    fetchImpl: async () => jsonRes({ answer: 'AA', results: [{ title: 'T', url: 'u', content: 'c' }] }) as unknown as Response,
  });
  assert.equal(tav.engine, 'tavily');
  assert.equal(tav.answer, 'AA');

  const ser = await webSearch('q', {
    env: { SERPER_API_KEY: 'k' },
    fetchImpl: async () => jsonRes({ organic: [{ title: 'O', link: 'https://o', snippet: 's' }] }) as unknown as Response,
  });
  assert.equal(ser.engine, 'serper');

  const ddg = await webSearch('q', {
    env: {},
    lookup: publicLookup,
    maxResults: 1,
    fetchImpl: async () => textRes('<a class="result__a" href="https://z.com">Z</a>') as unknown as Response,
  });
  assert.equal(ddg.engine, 'duckduckgo');
});

test('webSearch returns provider errors and renderWebResult handles empty result sets', async () => {
  const failed = await webSearch('q', {
    env: {},
    lookup: publicLookup,
    fetchImpl: async () => { throw new Error('network down'); },
  });
  assert.match(failed.error ?? '', /web search \(duckduckgo\) failed: network down/);

  const rendered = renderWebResult({
    query: 'nothing',
    engine: 'duckduckgo',
    answer: 'Nope',
    results: [],
  });
  assert.match(rendered, /Answer: Nope/);
  assert.match(rendered, /No results for "nothing"/);
});

test('resolveUserAgent: browser-like default, env override honored', () => {
  assert.match(resolveUserAgent({}), /Mozilla\/5\.0.*Chrome/);
  assert.equal(resolveUserAgent({}), DEFAULT_USER_AGENT);
  assert.match(resolveUserAgent({}), /Chrome\/13[0-9]/, 'UA should use a reasonably current Chrome version');
  assert.equal(resolveUserAgent({ OCTOCODE_WEB_USER_AGENT: 'MyBot/1.0' }), 'MyBot/1.0');
});

test('safeFetch sends a browser-like User-Agent (overridable via env)', async () => {
  let hdrs: Record<string, string> = {};
  const fetchImpl = async (_url: string, init?: RequestInit) => {
    hdrs = init?.headers as Record<string, string>;
    return { status: 200, ok: true, headers: new Map(), body: null, text: async () => '' } as unknown as Response;
  };
  const lookup = async () => [{ address: '8.8.8.8', family: 4 }];
  await safeFetch('https://x.com/', { fetchImpl, lookup });
  assert.match(hdrs['user-agent']!, /Chrome/);
  assert.ok(hdrs['accept-language']);
  assert.equal(hdrs['sec-ch-ua'], DEFAULT_SEC_CH_UA, 'sec-ch-ua sent with default Chrome UA');
  assert.equal(hdrs['sec-ch-ua-mobile'], '?0');
  assert.equal(hdrs['sec-ch-ua-platform'], '"macOS"');
  assert.equal(hdrs['sec-fetch-site'], 'none');
  assert.equal(hdrs['sec-fetch-mode'], 'navigate');
  assert.equal(hdrs['sec-fetch-dest'], 'document');
  await safeFetch('https://x.com/', { fetchImpl, lookup, env: { OCTOCODE_WEB_USER_AGENT: 'Custom/9' } });
  assert.equal(hdrs['user-agent'], 'Custom/9');
  assert.equal(hdrs['sec-ch-ua'], undefined, 'sec-ch-ua omitted when custom UA is set');
});

test('pickProvider: explicit engine wins, else ladder Tavily→Serper→DuckDuckGo by key', () => {
  assert.equal(pickProvider({ engine: 'serper', env: {} }), 'serper');
  assert.equal(pickProvider({ env: { TAVILY_API_KEY: 't', SERPER_API_KEY: 's' } }), 'tavily');
  assert.equal(pickProvider({ env: { SERPER_API_KEY: 's' } }), 'serper');
  assert.equal(pickProvider({ env: {} }), 'duckduckgo');
  assert.equal(pickProvider({ engine: 'garbage', env: { SERPER_API_KEY: 's' } }), 'serper', 'unknown engine → ladder');
});

test('normalizeApiKey strips Authorization/Bearer prefixes', () => {
  assert.equal(normalizeApiKey('Bearer tvly-x'), 'tvly-x');
  assert.equal(normalizeApiKey('Authorization: Bearer k'), 'k');
  assert.equal(normalizeApiKey('  raw '), 'raw');
});

test('tavilySearch normalizes {answer, results[{title,url,snippet}]} and sends Bearer auth', async () => {
  let sent: { url: string; auth: string; body: Record<string, unknown> } = { url: '', auth: '', body: {} };
  const fetchImpl = async (url: string, init?: RequestInit) => {
    sent = {
      url,
      auth: (init?.headers as Record<string, string>)['Authorization']!,
      body: JSON.parse(init?.body as string) as Record<string, unknown>,
    };
    return { ok: true, json: async () => ({ answer: 'A1', results: [{ title: 'T', url: 'https://x', content: 'C' }] }) } as unknown as Response;
  };
  const out = await tavilySearch('q', { apiKey: 'k', maxResults: 3 }, { fetchImpl });
  assert.equal(sent.url, 'https://api.tavily.com/search');
  assert.equal(sent.auth, 'Bearer k');
  assert.equal(sent.body['include_answer'], true);
  assert.equal(out.engine, 'tavily');
  assert.equal(out.answer, 'A1');
  assert.deepEqual(out.results, [{ title: 'T', url: 'https://x', snippet: 'C' }]);
});

test('tavilySearch returns {error} on missing key or bad status (never throws)', async () => {
  assert.match(
    (await tavilySearch('q', {}, { fetchImpl: async () => ({}) as unknown as Response })).error ?? '',
    /TAVILY_API_KEY/,
  );
  const out = await tavilySearch(
    'q',
    { apiKey: 'k' },
    { fetchImpl: async () => ({ ok: false, status: 401 }) as unknown as Response },
  );
  assert.match(out.error ?? '', /Tavily API 401/);
});

test('serperSearch normalizes organic + answerBox and sends X-API-KEY + tbs', async () => {
  let sent: { url: string; key: string; body: Record<string, unknown> } = { url: '', key: '', body: {} };
  const fetchImpl = async (url: string, init?: RequestInit) => {
    sent = {
      url,
      key: (init?.headers as Record<string, string>)['X-API-KEY']!,
      body: JSON.parse(init?.body as string) as Record<string, unknown>,
    };
    return {
      ok: true,
      json: async () => ({
        answerBox: { answer: 'SA' },
        organic: [{ title: 'OT', link: 'https://o', snippet: 'OS' }],
      }),
    } as unknown as Response;
  };
  const out = await serperSearch('q', { apiKey: 'sk', timeRange: 'week', maxResults: 5 }, { fetchImpl });
  assert.equal(sent.url, 'https://google.serper.dev/search');
  assert.equal(sent.key, 'sk');
  assert.equal(sent.body['tbs'], 'qdr:w');
  assert.equal(out.engine, 'serper');
  assert.equal(out.answer, 'SA');
  assert.deepEqual(out.results, [{ title: 'OT', url: 'https://o', snippet: 'OS' }]);
});

test('renderWebResult surfaces the answer and serving engine', () => {
  const rendered = renderWebResult({
    query: 'q',
    engine: 'tavily',
    answer: 'The answer',
    results: [{ title: 'T', url: 'u', snippet: 's' }],
  });
  assert.ok(rendered.includes('Answer: The answer'));
  assert.ok(rendered.includes('[tavily]'));
  assert.ok(rendered.includes('1. T'));
});

test('runWebTool requires url or query; renderWebResult formats both modes', async () => {
  const out = await runWebTool({});
  assert.ok((out as { error?: string }).error && /Provide either/.test((out as { error: string }).error));
  assert.equal(renderWebResult({ error: 'boom' }), 'boom');
  assert.ok(renderWebResult({ query: 'q', results: [{ title: 'T', url: 'u', snippet: 's' }] }).includes('1. T'));
  assert.ok(renderWebResult({ url: 'https://x', title: 'Ti', text: 'body' }).includes('# Ti'));
});

test('runWebTool dispatches query options through webSearch', async () => {
  const html = '<a class="result__a" href="https://docs.example.com/a">Docs</a><a class="result__snippet">Snippet</a>';
  const out = await runWebTool(
    {
      query: 'octocode docs',
      maxResults: 1,
      engine: 'duckduckgo',
      timeRange: 'week',
      includeDomains: ['docs.example.com'],
      excludeDomains: ['noise.example.com'],
    },
    {
      lookup: publicLookup,
      fetchImpl: async () => textRes(html) as unknown as Response,
    },
  );
  assert.equal((out as { engine?: string }).engine, 'duckduckgo');
  assert.equal((out as { results?: Array<{ title: string }> }).results?.[0]?.title, 'Docs');
});

test('runWebTool: page param dispatched to webFetch and reflected in result', async () => {
  const body = 'a'.repeat(20) + ' ' + 'b'.repeat(20) + ' ' + 'c'.repeat(20);
  const html = `<title>Doc</title><body><p>${body}</p></body>`;
  const fetchImpl = async () => textRes(html) as unknown as Response;

  const p1 = await runWebTool(
    { url: 'https://x.com/doc', maxChars: 25, page: 1 },
    { fetchImpl, lookup: publicLookup },
  );
  const p2 = await runWebTool(
    { url: 'https://x.com/doc', maxChars: 25, page: 2 },
    { fetchImpl, lookup: publicLookup },
  );

  const f1 = p1 as WebFetchResult;
  const f2 = p2 as WebFetchResult;
  assert.equal(f1.page, 1);
  assert.equal(f2.page, 2);
  assert.ok(f1.text !== f2.text, 'different slices');
  assert.equal(f1.truncated, true);
});
