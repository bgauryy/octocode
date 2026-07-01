import assert from 'node:assert/strict';
import test from 'node:test';
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
} from '../src/web.js';

// ── test helpers ──────────────────────────────────────────────────────────
const publicLookup = async () => [{ address: '8.8.8.8', family: 4 }];
const textRes = (body, { status = 200, ct = 'text/html' } = {}) => ({
  ok: status >= 200 && status < 300, status,
  headers: new Map([['content-type', ct]]), body: null, text: async () => body,
});
const jsonRes = (obj, { status = 200 } = {}) => ({
  ok: status >= 200 && status < 300, status, json: async () => obj,
});
const streamRes = (parts) => {
  const enc = new TextEncoder();
  const chunks = parts.map((p) => enc.encode(p));
  let i = 0;
  return { body: { getReader: () => ({ read: async () => (i < chunks.length ? { done: false, value: chunks[i++] } : { done: true }), cancel: async () => {} }) } };
};

test('isBlockedIp blocks private/loopback/link-local/metadata/ULA/mapped, allows public', () => {
  for (const ip of ['127.0.0.1', '10.1.2.3', '192.168.0.1', '172.16.5.5', '169.254.169.254',
    '100.64.0.1', '0.0.0.0', '::1', '::', 'fe80::1', 'fc00::1', '::ffff:127.0.0.1', 'not-an-ip']) {
    assert.equal(isBlockedIp(ip), true, `${ip} should be blocked`);
  }
  for (const ip of ['8.8.8.8', '1.1.1.1', '140.82.121.4', '2606:4700:4700::1111', '::ffff:8.8.8.8']) {
    assert.equal(isBlockedIp(ip), false, `${ip} should be allowed`);
  }
});

test('assertPublicUrl rejects non-http(s) and hosts resolving to private IPs (DNS rebinding)', async () => {
  await assert.rejects(() => assertPublicUrl('file:///etc/passwd'), /non-http/);
  await assert.rejects(() => assertPublicUrl('ftp://example.com'), /non-http/);
  // attacker.com resolves to loopback → must be blocked (DNS-rebinding vector)
  await assert.rejects(
    () => assertPublicUrl('http://attacker.com/', { lookup: async () => [{ address: '127.0.0.1', family: 4 }] }),
    /blocked address/,
  );
  // literal metadata IP blocked without needing DNS
  await assert.rejects(() => assertPublicUrl('http://169.254.169.254/latest/meta-data/'), /private\/loopback/);
  // public host allowed
  const ok = await assertPublicUrl('https://good.com/x', { lookup: async () => [{ address: '8.8.8.8', family: 4 }] });
  assert.equal(ok.hostname, 'good.com');
});

test('safeFetch re-validates every redirect hop and blocks a redirect into a private IP', async () => {
  const publicLookup = async (h) => [{ address: h === 'evil.com' ? '8.8.8.8' : '8.8.8.8', family: 4 }];
  // First hop public, redirects to a metadata IP → second hop must be blocked.
  const fetchImpl = async (url) => {
    if (url === 'https://evil.com/') {
      return { status: 302, ok: false, headers: new Map([['location', 'http://169.254.169.254/']]) };
    }
    throw new Error('should not reach the private hop');
  };
  await assert.rejects(
    () => safeFetch('https://evil.com/', { fetchImpl, lookup: publicLookup, timeoutMs: 1000 }),
    /private\/loopback|blocked address/,
  );
});

test('safeFetch follows a valid redirect and returns the final response', async () => {
  const lookup = async () => [{ address: '8.8.8.8', family: 4 }];
  const fetchImpl = async (url) => {
    if (url === 'https://a.com/') return { status: 301, ok: false, headers: new Map([['location', 'https://b.com/']]) };
    return { status: 200, ok: true, headers: new Map([['content-type', 'text/html']]), body: null, text: async () => '<title>Hi</title>ok' };
  };
  const { res, finalUrl } = await safeFetch('https://a.com/', { fetchImpl, lookup });
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
  assert.equal(r[0].title, 'One Title');
  assert.equal(r[0].url, 'https://one.com');
  assert.equal(r[0].snippet, 'First snippet');
});

test('readCapped: an aborted signal short-circuits a hanging body read (no hang)', async () => {
  // reader.read() never resolves — only the abort signal can end this.
  const hangingRes = { body: { getReader: () => ({ read: () => new Promise(() => {}), cancel: async () => {} }) } };
  const ac = new AbortController();
  ac.abort();
  const out = await readCapped(hangingRes, 1000, { signal: ac.signal });
  assert.equal(out.truncated, true, 'aborted read returns truncated instead of hanging');
});

test('createDeadline: fires its signal on timeout and cleans up', async () => {
  const d = createDeadline({ timeoutMs: 5 });
  await new Promise((r) => setTimeout(r, 25));
  assert.equal(d.signal.aborted, true, 'deadline aborts after timeout');
  d.cleanup();
});

test('readCapped: assembles streamed chunks; truncates at maxBytes', async () => {
  const ok = await readCapped(streamRes(['hello ', 'world']), 1000);
  assert.equal(ok.text, 'hello world');
  assert.equal(ok.truncated, false);
  const cut = await readCapped(streamRes(['abcdef', 'ghi']), 3);
  assert.equal(cut.truncated, true);
});

test('postJson: returns parsed body on 2xx, throws status-bearing error otherwise', async () => {
  const raw = await postJson('u', { body: {}, fetchImpl: async () => jsonRes({ x: 1 }) });
  assert.equal(raw.x, 1);
  await assert.rejects(() => postJson('u', { body: {}, fetchImpl: async () => jsonRes({}, { status: 500 }) }), /HTTP 500/);
});

test('serperSearch: knowledgeGraph description is used as answer when no answerBox', async () => {
  const out = await serperSearch('q', { apiKey: 'k' }, { fetchImpl: async () => jsonRes({ knowledgeGraph: { description: 'KG' }, organic: [] }) });
  assert.equal(out.answer, 'KG');
});

test('webFetch: HTML → title + readable text; sets final url', async () => {
  const out = await webFetch('https://x.com/', {
    fetchImpl: async () => textRes('<title>Hi</title><body><script>bad()</script><p>Body text</p></body>'),
    lookup: publicLookup,
  });
  assert.equal(out.title, 'Hi');
  assert.equal(out.url, 'https://x.com/');
  assert.ok(out.text.includes('Body text'));
  assert.ok(!out.text.includes('bad()'), 'script stripped');
});

test('webFetch: non-HTML passthrough, HTTP error, and maxChars truncation', async () => {
  const json = await webFetch('https://x.com/d', { fetchImpl: async () => textRes('{"a":1}', { ct: 'application/json' }), lookup: publicLookup });
  assert.equal(json.title, '');
  assert.equal(json.text, '{"a":1}');

  const err = await webFetch('https://x.com/404', { fetchImpl: async () => textRes('', { status: 404 }), lookup: publicLookup });
  assert.match(err.error, /404/);

  const clip = await webFetch('https://x.com/big', { fetchImpl: async () => textRes(`<p>${'a'.repeat(100)}</p>`), lookup: publicLookup, maxChars: 10 });
  assert.equal(clip.truncated, true);
  assert.ok(clip.text.length <= 10);
});

test('duckDuckGoSearch: parses SERP via injected fetch', async () => {
  const html = '<a class="result__a" href="//duckduckgo.com/l/?uddg=https%3A%2F%2Fo.com">OT</a><a class="result__snippet">S</a>';
  const out = await duckDuckGoSearch('q', { fetchImpl: async () => textRes(html), lookup: publicLookup, maxResults: 1 });
  assert.equal(out.engine, 'duckduckgo');
  assert.deepEqual(out.results[0], { title: 'OT', url: 'https://o.com', snippet: 'S' });
});

test('webSearch: dispatches to the provider chosen by env, end to end', async () => {
  const tav = await webSearch('q', { env: { TAVILY_API_KEY: 'k' }, fetchImpl: async () => jsonRes({ answer: 'AA', results: [{ title: 'T', url: 'u', content: 'c' }] }) });
  assert.equal(tav.engine, 'tavily');
  assert.equal(tav.answer, 'AA');

  const ser = await webSearch('q', { env: { SERPER_API_KEY: 'k' }, fetchImpl: async () => jsonRes({ organic: [{ title: 'O', link: 'https://o', snippet: 's' }] }) });
  assert.equal(ser.engine, 'serper');

  const ddg = await webSearch('q', { env: {}, lookup: publicLookup, maxResults: 1, fetchImpl: async () => textRes('<a class="result__a" href="https://z.com">Z</a>') });
  assert.equal(ddg.engine, 'duckduckgo');
});

test('resolveUserAgent: browser-like default, env override honored', () => {
  assert.match(resolveUserAgent({}), /Mozilla\/5\.0.*Chrome/);
  assert.equal(resolveUserAgent({}), DEFAULT_USER_AGENT);
  assert.match(resolveUserAgent({}), /Chrome\/13[0-9]/, 'UA should use a reasonably current Chrome version');
  assert.equal(resolveUserAgent({ OCTOCODE_WEB_USER_AGENT: 'MyBot/1.0' }), 'MyBot/1.0');
});

test('safeFetch sends a browser-like User-Agent (overridable via env)', async () => {
  let hdrs;
  const fetchImpl = async (_url, init) => { hdrs = init.headers; return { status: 200, ok: true, headers: new Map(), body: null, text: async () => '' }; };
  const lookup = async () => [{ address: '8.8.8.8', family: 4 }];
  await safeFetch('https://x.com/', { fetchImpl, lookup });
  assert.match(hdrs['user-agent'], /Chrome/);
  assert.ok(hdrs['accept-language']);
  // Sec-CH-UA headers should accompany the default Chrome UA.
  assert.equal(hdrs['sec-ch-ua'], DEFAULT_SEC_CH_UA, 'sec-ch-ua sent with default Chrome UA');
  assert.equal(hdrs['sec-ch-ua-mobile'], '?0');
  assert.equal(hdrs['sec-ch-ua-platform'], '"macOS"');
  assert.equal(hdrs['sec-fetch-site'], 'none');
  assert.equal(hdrs['sec-fetch-mode'], 'navigate');
  assert.equal(hdrs['sec-fetch-dest'], 'document');
  // Custom UA: sec-ch-ua must NOT be injected — caller is responsible for consistency.
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
  let sent;
  const fetchImpl = async (url, init) => {
    sent = { url, auth: init.headers.Authorization, body: JSON.parse(init.body) };
    return { ok: true, json: async () => ({ answer: 'A1', results: [{ title: 'T', url: 'https://x', content: 'C' }] }) };
  };
  const out = await tavilySearch('q', { apiKey: 'k', maxResults: 3 }, { fetchImpl });
  assert.equal(sent.url, 'https://api.tavily.com/search');
  assert.equal(sent.auth, 'Bearer k');
  assert.equal(sent.body.include_answer, true);
  assert.equal(out.engine, 'tavily');
  assert.equal(out.answer, 'A1');
  assert.deepEqual(out.results, [{ title: 'T', url: 'https://x', snippet: 'C' }]);
});

test('tavilySearch returns {error} on missing key or bad status (never throws)', async () => {
  assert.match((await tavilySearch('q', {}, { fetchImpl: async () => ({}) })).error, /TAVILY_API_KEY/);
  const out = await tavilySearch('q', { apiKey: 'k' }, { fetchImpl: async () => ({ ok: false, status: 401 }) });
  assert.match(out.error, /Tavily API 401/);
});

test('serperSearch normalizes organic + answerBox and sends X-API-KEY + tbs', async () => {
  let sent;
  const fetchImpl = async (url, init) => {
    sent = { url, key: init.headers['X-API-KEY'], body: JSON.parse(init.body) };
    return { ok: true, json: async () => ({
      answerBox: { answer: 'SA' },
      organic: [{ title: 'OT', link: 'https://o', snippet: 'OS' }],
    }) };
  };
  const out = await serperSearch('q', { apiKey: 'sk', timeRange: 'week', maxResults: 5 }, { fetchImpl });
  assert.equal(sent.url, 'https://google.serper.dev/search');
  assert.equal(sent.key, 'sk');
  assert.equal(sent.body.tbs, 'qdr:w');
  assert.equal(out.engine, 'serper');
  assert.equal(out.answer, 'SA');
  assert.deepEqual(out.results, [{ title: 'OT', url: 'https://o', snippet: 'OS' }]);
});

test('renderWebResult surfaces the answer and serving engine', () => {
  const rendered = renderWebResult({ query: 'q', engine: 'tavily', answer: 'The answer', results: [{ title: 'T', url: 'u', snippet: 's' }] });
  assert.ok(rendered.includes('Answer: The answer'));
  assert.ok(rendered.includes('[tavily]'));
  assert.ok(rendered.includes('1. T'));
});

test('runWebTool requires url or query; renderWebResult formats both modes', () => {
  return runWebTool({}).then((out) => {
    assert.ok(out.error && /Provide either/.test(out.error));
    assert.equal(renderWebResult({ error: 'boom' }), 'boom');
    assert.ok(renderWebResult({ query: 'q', results: [{ title: 'T', url: 'u', snippet: 's' }] }).includes('1. T'));
    assert.ok(renderWebResult({ url: 'https://x', title: 'Ti', text: 'body' }).includes('# Ti'));
  });
});
