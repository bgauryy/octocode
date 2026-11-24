import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createServer } from 'node:http';
import { mkdirSync, mkdtempSync, rmSync, writeFileSync, readFileSync, existsSync, symlinkSync, realpathSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawn } from 'node:child_process';
import { canonicalUrl } from '../search.mjs';
import { normalize, quoteInText, pageDates, dayOf, dateInWindow, textDate } from '../verify-claims.mjs';
import { findMatches } from '../read-page.mjs';

const scripts = fileURLToPath(new URL('../', import.meta.url));

function serve(handler) {
  return new Promise((resolve) => {
    const server = createServer(handler);
    server.listen(0, '127.0.0.1', () => resolve({ server, base: `http://127.0.0.1:${server.address().port}` }));
  });
}
function run(script, args, env = {}) {
  const cwd = mkdtempSync(join(tmpdir(), 'brainstorming-provider-cwd-'));
  const home = mkdtempSync(join(tmpdir(), 'brainstorming-provider-home-'));
  return new Promise((resolve) => {
    const child = spawn(process.execPath, [join(scripts, script), ...args], { cwd, env: { PATH: process.env.PATH, OCTOCODE_HOME: home, ...env } });
    let stdout = '';
    child.stdout.on('data', (d) => { stdout += d; });
    child.on('close', (status) => {
      rmSync(cwd, { recursive: true, force: true });
      rmSync(home, { recursive: true, force: true });
      resolve({ status, stdout, json: (() => { try { return JSON.parse(stdout); } catch { return null; } })() });
    });
  });
}
const readBody = (req) => new Promise((resolve) => { let s = ''; req.on('data', (d) => { s += d; }); req.on('end', () => resolve(s ? JSON.parse(s) : {})); });

test('canonicalUrl folds scheme, www, fragments, trailing slashes, and tracking parameters', () => {
  assert.equal(canonicalUrl('https://www.Example.com/a/?utm_source=x#top'), 'example.com/a');
  assert.equal(canonicalUrl('http://example.com/a?id=2&ref=feed'), 'example.com/a?id=2');
});

test('search merges providers by URL, ranks agreement first, records per-provider status, and never echoes a key', async () => {
  const { server, base } = await serve(async (req, res) => {
    const body = await readBody(req);
    res.setHeader('content-type', 'application/json');
    if (req.url === '/tavily/search') { assert.equal(body.include_published_date, true); assert.equal(body.include_usage, true); return res.end(JSON.stringify({ results: [{ url: 'https://www.a.test/x/', title: 'A', content: 'tavily snippet', published_date: '2025-01-02' }, { url: 'https://b.test/only-tavily', title: 'B', content: 'b' }], usage: { credits: 1 } })); }
    if (req.url === '/exa/search') { assert.equal(body.category, 'publication'); assert.deepEqual(body.contents, { highlights: { maxCharacters: 640 } }); return res.end(JSON.stringify({ results: [{ url: 'https://a.test/x?utm_source=feed', title: 'A', highlights: ['exa highlight'] }], costDollars: { total: 0.005 } })); }
    if (req.url === '/serper/scholar') { res.statusCode = 403; return res.end(JSON.stringify({ message: 'bad key SECRETKEY123' })); }
    res.statusCode = 404; return res.end('{}');
  });
  try {
    const env = { TAVILY_API_KEY: 'SECRETKEY123', EXA_API_KEY: 'SECRETKEY123', SERPER_API_KEY: 'SECRETKEY123', TAVILY_BASE_URL: `${base}/tavily`, EXA_BASE_URL: `${base}/exa`, SERPER_BASE_URL: `${base}/serper` };
    const r = await run('search.mjs', ['--query', 'q', '--kind', 'papers', '--limit', '3'], env);
    assert.equal(r.status, 0, r.stdout);
    assert.ok(!r.stdout.includes('SECRETKEY123'), 'a key leaked into the output');
    assert.equal(r.json.results[0].url, 'https://www.a.test/x/');
    assert.deepEqual(r.json.results[0].seenBy.map((s) => s.provider).toSorted(), ['exa', 'tavily']);
    assert.equal(r.json.results.length, 2);
    assert.equal(r.json.providers.serper.status, 'keyRejected', 'a 403 on every query is a key problem');
    assert.match(r.json.providers.serper.fix, /keys\.mjs set SERPER_API_KEY --stdin/);
    assert.equal(r.json.providers.exa.costDollars, 0.005);
    assert.equal(r.json.providers.tavily.credits, 1);
  } finally { server.close(); }
});

test('search names a keysRejected setup error when every provider rejects its key', async () => {
  const { server, base } = await serve(async (req, res) => {
    res.setHeader('content-type', 'application/json');
    res.statusCode = req.url.startsWith('/serper') ? 403 : 401;
    return res.end(JSON.stringify({ error: 'Invalid API key' }));
  });
  try {
    const env = { TAVILY_API_KEY: 'k1', EXA_API_KEY: 'k2', SERPER_API_KEY: 'k3', DEEPSEEK_API_KEY: 'k4', TAVILY_BASE_URL: `${base}/tavily`, EXA_BASE_URL: `${base}/exa`, SERPER_BASE_URL: `${base}/serper`, DEEPSEEK_BASE_URL: `${base}/deepseek` };
    const r = await run('search.mjs', ['--query', 'q'], env);
    assert.equal(r.status, 1);
    assert.equal(r.json.error.code, 'keysRejected');
    assert.match(r.json.error.message, /tavily, exa, serper, deepseek/);
    for (const name of ['tavily', 'exa', 'serper', 'deepseek']) assert.equal(r.json.providers[name].status, 'keyRejected');
    assert.match(r.json.providers.deepseek.fix, /keys\.mjs set DEEPSEEK_API_KEY --stdin/);
  } finally { server.close(); }
});

test('search reports a missing key and invalid input with the standard error shape', async () => {
  const none = await run('search.mjs', ['--query', 'q'], {});
  assert.equal(none.status, 1);
  assert.equal(none.json.error.code, 'missingKey');
  // The error names every place it read, so a key saved in the wrong folder is visible.
  assert.match(none.json.error.message, /process environment/);
  assert.match(none.json.error.message, /\.env\b/);
  assert.match(none.json.error.message, /\.octocode[\\/]\.env/);
  assert.match(none.json.error.message, /keys\.mjs set TAVILY_API_KEY --stdin/);
  const bad = await run('search.mjs', ['--query', 'q', '--kind', 'blogs'], { EXA_API_KEY: 'k' });
  assert.equal(bad.status, 2);
  assert.equal(bad.json.error.code, 'invalidArguments');
});

// A key comes from the first place that sets it: process environment, then <home>/.env, then <workspace>/.octocode/.env.
test('keys.mjs reports each key with its source in process, home, workspace order, never the value', async () => {
  const root = realpathSync(mkdtempSync(join(tmpdir(), 'bs-keys-')));
  const home = join(root, 'home');
  const workspace = join(root, 'workspace');
  mkdirSync(home, { recursive: true });
  mkdirSync(join(workspace, '.octocode'), { recursive: true });
  writeFileSync(join(home, '.env'), 'TAVILY_API_KEY=home-secret\nEXA_API_KEY=home-exa-secret\n');
  writeFileSync(join(workspace, '.octocode', '.env'), 'EXA_API_KEY=workspace-secret\nSERPER_API_KEY=workspace-serper-secret\n');
  const keys = (cwd, args, env = {}) => new Promise((resolve) => {
    const child = spawn(process.execPath, [join(scripts, 'keys.mjs'), ...args], { cwd, env: { PATH: process.env.PATH, HOME: home, OCTOCODE_HOME: home, ...env } });
    let stdout = '';
    child.stdout.on('data', (d) => { stdout += d; });
    child.on('close', (status) => resolve({ status, stdout, json: JSON.parse(stdout) }));
  });
  const sources = (r) => Object.fromEntries(r.json.keys.map((k) => [k.name, [k.key, k.source]]));
  try {
    const r = await keys(workspace, ['get', 'TAVILY_API_KEY', 'EXA_API_KEY', 'SERPER_API_KEY']);
    assert.equal(r.status, 0, r.stdout);
    assert.doesNotMatch(r.stdout, /secret/, 'a key value leaked');
    // EXA is in both files: the home one wins, the workspace only fills SERPER.
    assert.deepEqual(sources(r), { TAVILY_API_KEY: ['set', 'home'], EXA_API_KEY: ['set', 'home'], SERPER_API_KEY: ['set', 'workspace'] });
    assert.deepEqual(r.json.files, { home: join(home, '.env'), workspace: join(workspace, '.octocode', '.env') });

    const proc = await keys(workspace, ['get', 'EXA_API_KEY'], { EXA_API_KEY: 'process-secret' });
    assert.deepEqual(sources(proc), { EXA_API_KEY: ['set', 'process'] });

    // From a subfolder the workspace file is <subfolder>/.octocode/.env; files.workspace shows which one was read.
    mkdirSync(join(workspace, 'sub'));
    const sub = await keys(join(workspace, 'sub'), ['get', 'SERPER_API_KEY']);
    assert.equal(sub.status, 1);
    assert.equal(sub.json.keys[0].key, 'missing');
    assert.match(sub.json.keys[0].fix, /keys\.mjs set SERPER_API_KEY --stdin/);
    assert.equal(sub.json.files.workspace, join(workspace, 'sub', '.octocode', '.env'));

    const listed = await keys(workspace, ['list']);
    assert.equal(listed.status, 0);
    assert.ok(['TAVILY_API_KEY', 'EXA_API_KEY', 'SERPER_API_KEY'].every((name) => listed.json.keys.some((k) => k.name === name)));
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('search reads a key from the workspace .env when home and process have none', async () => {
  const seen = [];
  const { server, base } = await serve(async (req, res) => {
    seen.push(req.headers.authorization);
    res.setHeader('content-type', 'application/json');
    res.end(JSON.stringify({ results: [{ url: 'https://a.test/x', title: 'A', content: 'x' }] }));
  });
  const root = mkdtempSync(join(tmpdir(), 'bs-ws-'));
  try {
    mkdirSync(join(root, 'home'));
    mkdirSync(join(root, 'ws', '.octocode'), { recursive: true });
    writeFileSync(join(root, 'ws', '.octocode', '.env'), 'TAVILY_API_KEY=from-workspace\n');
    const r = await new Promise((resolve) => {
      const child = spawn(process.execPath, [join(scripts, 'search.mjs'), '--query', 'q', '--providers', 'tavily'], { cwd: join(root, 'ws'), env: { PATH: process.env.PATH, OCTOCODE_HOME: join(root, 'home'), TAVILY_BASE_URL: base } });
      let stdout = '';
      child.stdout.on('data', (d) => { stdout += d; });
      child.on('close', (status) => resolve({ status, stdout }));
    });
    assert.equal(r.status, 0, r.stdout);
    assert.deepEqual(seen, ['Bearer from-workspace']);
  } finally { server.close(); rmSync(root, { recursive: true, force: true }); }
});

test('serper reads further pages up to --limit, reports credits, and returns related searches and questions', async () => {
  const pages = [];
  const { server, base } = await serve(async (req, res) => {
    const body = await readBody(req);
    pages.push(body.page ?? 1);
    res.setHeader('content-type', 'application/json');
    const page = body.page ?? 1;
    const organic = Array.from({ length: page === 1 ? 10 : 4 }, (_, i) => ({ link: `https://s.test/${page}/${i}`, title: `r${page}.${i}`, snippet: 's' }));
    res.end(JSON.stringify({ organic, credits: 1, ...(page === 1 ? { relatedSearches: [{ query: 'event sourcing alternatives' }], peopleAlsoAsk: [{ question: 'When not to use event sourcing?' }] } : {}) }));
  });
  try {
    const r = await run('search.mjs', ['--query', 'q', '--providers', 'serper', '--limit', '14'], { SERPER_API_KEY: 'k', SERPER_BASE_URL: base });
    assert.equal(r.status, 0, r.stdout);
    assert.deepEqual(pages, [1, 2]);
    assert.equal(r.json.results.length, 14);
    assert.equal(r.json.providers.serper.credits, 2);
    assert.deepEqual(r.json.related.map((x) => [x.query, x.kind]), [['event sourcing alternatives', 'search'], ['When not to use event sourcing?', 'question']]);
  } finally { server.close(); }
});

test('deepseek sends filters as query operators, maps its rows, and serves only web searches', async () => {
  const calls = [];
  const { server, base } = await serve(async (req, res) => {
    calls.push({ url: req.url, auth: req.headers.authorization, body: await readBody(req) });
    res.setHeader('content-type', 'application/json');
    res.end(JSON.stringify({ results: [{ rerank_score: 2.4, title: 'V4', url: 'https://www.d.test/v4/', snippet: 'short line', summary: 'the longer summary for models', content: '', site_name: null, site_icon: null, date: '2026-06-01T00:00:00+00:00', authority: 3 }] }));
  });
  try {
    const env = { DEEPSEEK_API_KEY: 'SECRETKEY123', DEEPSEEK_BASE_URL: base };
    const r = await run('search.mjs', ['--query', 'release notes', '--providers', 'deepseek', '--limit', '4', '--since', '2026-01-01', '--include-domain', 'github.com', '--include-domain', 'huggingface.co', '--exclude-domain', 'spam.test'], env);
    assert.equal(r.status, 0, r.stdout);
    assert.ok(!r.stdout.includes('SECRETKEY123'), 'a key leaked into the output');
    assert.deepEqual(calls, [{ url: '/v1/tools/search', auth: 'Bearer SECRETKEY123', body: { query: 'release notes site:github.com OR site:huggingface.co -site:spam.test after:2026-01-01', count: 4 } }]);
    assert.deepEqual(r.json.results.map(({ url, title, snippet, published, seenBy }) => ({ url, title, snippet, published, by: seenBy.map((s) => s.provider) })),
      [{ url: 'https://www.d.test/v4/', title: 'V4', snippet: 'the longer summary for models', published: '2026-06-01T00:00:00+00:00', by: ['deepseek'] }]);
    // DeepSeek has no news or scholarly surface: skipped beside the others, refused when it is the only one asked.
    const news = await run('search.mjs', ['--query', 'q', '--kind', 'news'], { ...env, EXA_API_KEY: 'k', EXA_BASE_URL: base });
    assert.deepEqual(news.json.providers.deepseek, { status: 'skipped', reason: 'no news search' });
    const only = await run('search.mjs', ['--query', 'q', '--kind', 'papers', '--providers', 'deepseek'], env);
    assert.equal(only.status, 2);
    assert.equal(only.json.error.code, 'invalidArguments');
    assert.match(only.json.error.message, /deepseek cannot search it \(use tavily, exa, serper\)/);
    assert.equal(calls.filter((c) => c.url === '/v1/tools/search').length, 1, 'a search DeepSeek cannot serve sent it no request');
  } finally { server.close(); }
});

test('search names exhausted credits and rate limits per provider instead of a query problem', async () => {
  const { server, base } = await serve(async (req, res) => {
    res.setHeader('content-type', 'application/json');
    if (req.url.startsWith('/tavily')) { res.statusCode = 432; return res.end(JSON.stringify({ detail: { error: 'plan limit exceeded' } })); }
    if (req.url.startsWith('/exa')) { res.statusCode = 402; return res.end(JSON.stringify({ error: 'out of credits' })); }
    if (req.url.startsWith('/deepseek')) { res.statusCode = 402; return res.end(JSON.stringify({ error: { message: 'Insufficient Balance', type: 'unknown_error', param: null, code: 'invalid_request_error' } })); }
    res.statusCode = 429; res.setHeader('retry-after', '7'); return res.end(JSON.stringify({ message: 'Too many requests' }));
  });
  try {
    const env = { TAVILY_API_KEY: 'k1', EXA_API_KEY: 'k2', SERPER_API_KEY: 'k3', DEEPSEEK_API_KEY: 'k4', TAVILY_BASE_URL: `${base}/tavily`, EXA_BASE_URL: `${base}/exa`, SERPER_BASE_URL: `${base}/serper`, DEEPSEEK_BASE_URL: `${base}/deepseek` };
    const r = await run('search.mjs', ['--query', 'q'], env);
    assert.equal(r.status, 1);
    assert.equal(r.json.providers.tavily.status, 'quotaExceeded');
    assert.equal(r.json.providers.exa.status, 'quotaExceeded');
    assert.equal(r.json.providers.deepseek.status, 'quotaExceeded');
    assert.match(r.json.providers.deepseek.fix, /platform\.deepseek\.com/);
    assert.equal(r.json.providers.serper.status, 'rateLimited');
    assert.equal(r.json.providers.serper.retryAfterSeconds, 7);
    assert.equal(r.json.error.code, 'providersUnavailable');
  } finally { server.close(); }
});

test('a papers search keeps the caller\'s --include-domain instead of widening it to every paper site', async () => {
  const bodies = [];
  const { server, base } = await serve(async (req, res) => {
    bodies.push(await readBody(req));
    res.setHeader('content-type', 'application/json');
    res.end(JSON.stringify({ results: [{ url: 'https://arxiv.org/abs/1', title: 'A', content: 'x' }] }));
  });
  try {
    const env = { TAVILY_API_KEY: 'k', TAVILY_BASE_URL: base };
    const own = await run('search.mjs', ['--query', 'q', '--kind', 'papers', '--providers', 'tavily', '--include-domain', 'arxiv.org'], env);
    assert.equal(own.status, 0, own.stdout);
    assert.deepEqual(bodies[0].include_domains, ['arxiv.org']);
    const defaults = await run('search.mjs', ['--query', 'q', '--kind', 'papers', '--providers', 'tavily'], env);
    assert.equal(defaults.status, 0, defaults.stdout);
    assert.ok(bodies[1].include_domains.includes('arxiv.org') && bodies[1].include_domains.length > 1, 'papers without a domain filter use the scholarly domains');
  } finally { server.close(); }
});

test('quote matching tolerates typography and ellipses but not invented text', () => {
  const page = 'Teams that “consider the opposite” — deliberately — reduced bias by 30% [12]. Later, the effect faded.';
  assert.ok(quoteInText('teams that "consider the opposite" - deliberately - reduced bias by 30%', page));
  assert.ok(quoteInText('Teams that "consider the opposite" ... the effect faded.', page));
  assert.ok(!quoteInText('Teams that consider the opposite eliminated bias entirely', page));
  assert.ok(!quoteInText('the effect faded ... Teams that "consider the opposite"', page), 'fragments must appear in order');
  assert.equal(normalize('A B​'), 'a b');
});

test('pageDates reads meta tags and JSON-LD', () => {
  assert.equal(pageDates('<meta property="article:published_time" content="2024-05-01">').published, '2024-05-01');
  assert.equal(pageDates('<script type="application/ld+json">{"datePublished":"2023-02-03"}</script>').published, '2023-02-03');
});

test('verify-claims marks verified, missing, short, unreachable, and PDF claims and writes the ledger', async () => {
  const { server, base } = await serve((req, res) => {
    if (req.url === '/article') { res.setHeader('content-type', 'text/html'); return res.end('<html><head><title>Study</title><meta name="citation_publication_date" content="2022/03/01"></head><body><p>Forecasters who scored high on actively open-minded thinking were more accurate than their peers.</p></body></html>'); }
    if (req.url === '/captcha') { res.setHeader('content-type', 'text/html'); return res.end('<html><head><title>Checking your browser - reCAPTCHA</title></head><body><div class="g-recaptcha"></div></body></html>'); }
    if (req.url === '/paper.pdf') { res.setHeader('content-type', 'application/pdf'); return res.end('%PDF'); }
    res.statusCode = 404; return res.end('gone');
  });
  const dir = mkdtempSync(join(tmpdir(), 'brainstorm-verify-'));
  try {
    const ledger = join(dir, 'ledger.jsonl');
    writeFileSync(ledger, [
      { id: 'A1', claim: 'AOT predicts accuracy', url: `${base}/article`, quote: 'scored high on actively open-minded thinking were more accurate' },
      { id: 'A2', claim: 'Invented', url: `${base}/article`, quote: 'forecasters who meditate daily were twice as accurate' },
      { id: 'A3', claim: 'Too short', url: `${base}/article`, quote: 'accurate' },
      { id: 'A4', claim: 'Gone', url: `${base}/missing`, quote: 'this page no longer exists anywhere' },
      { id: 'A5', claim: 'PDF', url: `${base}/paper.pdf`, quote: 'a quote from a pdf document here' },
      { id: 'A6', url: `${base}/article`, quote: 'scored high on actively open-minded thinking' },
      { id: 'A7', claim: 'Walled', url: `${base}/captcha`, quote: 'a sentence that sits behind a bot wall' },
      { id: 'A8', claim: 'Walled, read by hand', url: `${base}/captcha`, quote: 'a sentence that sits behind a bot wall', verifiedBy: 'manual', method: 'host fetch tool' },
      { id: 'A9', claim: 'Invented, marked manual', url: `${base}/article`, quote: 'forecasters who meditate daily were twice as accurate', verifiedBy: 'manual', method: 'trust me' },
    ].map((r) => JSON.stringify(r)).join('\n'));
    const out = join(dir, 'verified.jsonl');
    const r = await run('verify-claims.mjs', ['--ledger', ledger, '--out', out], { OCTOCODE_NET_GUARD_TEST_LOOPBACK: '1' });
    assert.equal(r.status, 1, 'a ledger with unverified rows must not pass');
    assert.equal(r.json.ok, false);
    assert.equal(r.json.error.code, 'someUnverified');
    assert.equal(r.json.verifiedRatio, 0.222);
    assert.deepEqual(r.json.claims.map((c) => c.status), ['verified', 'quote-not-found', 'quote-too-short', 'fetch-failed', 'unsupported-content', 'missing-field', 'blocked', 'verified-manual', 'quote-not-found']);
    assert.equal(r.json.verified, 2);
    assert.equal(r.json.claims[0].title, 'Study');
    assert.equal(r.json.claims[0].published, '2022/03/01');
    const written = readFileSync(out, 'utf8').trim().split('\n').map((l) => JSON.parse(l));
    assert.equal(written[0].verification.status, 'verified');
    assert.ok(written[0].verification.checkedAt);
  } finally { server.close(); rmSync(dir, { recursive: true, force: true }); }
});

test('verify-claims reads a walled page through a provider copy and never counts a copy without the quote', async () => {
  const calls = [];
  const { server, base } = await serve(async (req, res) => {
    res.setHeader('content-type', req.url.startsWith('/page') ? 'text/html' : 'application/json');
    if (req.url.startsWith('/page')) return res.end('<html><head><title>Just a moment...</title></head><body>cf-chl</body></html>');
    const body = await readBody(req);
    calls.push([req.url, body.urls[0].split('/').pop()]);
    if (req.url === '/exa/contents') {
      if (body.urls[0].endsWith('/page-b')) return res.end(JSON.stringify({ statuses: [{ id: body.urls[0], status: 'error', error: { tag: 'CRAWL_NOT_FOUND' } }], results: [] }));
      return res.end(JSON.stringify({ statuses: [{ id: body.urls[0], status: 'success', source: 'cached' }], results: [{ url: body.urls[0], title: 'Walled study', text: 'Intro. Teams that wrote a premortem found more failure modes than the control group. End.', publishedDate: '2024-05-01T00:00:00.000Z' }], costDollars: { total: 0.001 } }));
    }
    if (req.url === '/tavily/extract') return res.end(JSON.stringify({ results: [{ url: body.urls[0], raw_content: 'Tavily copy: the second walled page states its finding plainly here.' }], failed_results: [], usage: { credits: 1 } }));
    res.statusCode = 404; return res.end('{}');
  });
  const dir = mkdtempSync(join(tmpdir(), 'brainstorm-copy-'));
  try {
    const ledger = join(dir, 'ledger.jsonl');
    writeFileSync(ledger, [
      { id: 'C1', claim: 'found in the Exa copy', url: `${base}/page-a`, quote: 'wrote a premortem found more failure modes than the control group' },
      { id: 'C2', claim: 'absent from the Exa copy', url: `${base}/page-a`, quote: 'premortems doubled the number of shipped features' },
      { id: 'C3', claim: 'Exa has no copy, Tavily does', url: `${base}/page-b`, quote: 'the second walled page states its finding plainly' },
    ].map((r) => JSON.stringify(r)).join('\n'));
    const env = { OCTOCODE_NET_GUARD_TEST_LOOPBACK: '1', EXA_API_KEY: 'ek', TAVILY_API_KEY: 'tk', EXA_BASE_URL: `${base}/exa`, TAVILY_BASE_URL: `${base}/tavily` };
    const r = await run('verify-claims.mjs', ['--ledger', ledger], env);
    assert.deepEqual(r.json.claims.map((c) => c.status), ['verified', 'blocked', 'verified']);
    assert.deepEqual(r.json.claims[0].via, { provider: 'exa', source: 'cached', costDollars: 0.001 });
    assert.equal(r.json.claims[0].published, '2024-05-01T00:00:00.000Z');
    assert.match(r.json.claims[1].reason, /not in the exa copy either/);
    assert.deepEqual(r.json.claims[2].via, { provider: 'tavily', credits: 1 });
    // One copy per URL, whatever the number of claims citing it.
    assert.deepEqual(calls.map((c) => c.join(' ')).toSorted(), ['/exa/contents page-a', '/exa/contents page-b', '/tavily/extract page-b']);

    const off = await run('verify-claims.mjs', ['--ledger', ledger, '--via', 'none'], env);
    assert.deepEqual(off.json.claims.map((c) => c.status), ['blocked', 'blocked', 'blocked']);
    const bad = await run('verify-claims.mjs', ['--ledger', ledger, '--via', 'serper'], env);
    assert.equal(bad.status, 2);
  } finally { server.close(); rmSync(dir, { recursive: true, force: true }); }
});

test('page dates read Microsoft Learn and update meta tags; windows compare days, or years when only a year is known', () => {
  assert.deepEqual(pageDates('<meta name="ms.date" content="2026-03-27"><meta name="updated_at" content="2026-09-25">'), { published: '2026-03-27', modified: '2026-09-25' });
  assert.equal(dayOf('Mon, 20 Jul 2026 00:00:00 GMT'), '2026-07-20');
  assert.equal(dayOf('2022/03/01'), '2022-03-01');
  assert.equal(dayOf('2 weeks ago'), null);
  assert.equal(dateInWindow({ published: '2025-06-01' }, '2025-01-01', '2025-12-31'), 'yes');
  assert.equal(dateInWindow({ published: '2024-06-01' }, '2025-01-01', null), 'no');
  assert.equal(dateInWindow({ published: '2025' }, '2025-06-01', null), 'yes');
  assert.equal(dateInWindow({ published: null, modified: 'garbage' }, '2025-01-01', null), 'unknown');
});

test('read-page prints the text the verifier reads, and grep context to copy quotes from', async () => {
  const { server, base } = await serve((req, res) => {
    res.setHeader('content-type', 'text/html');
    res.setHeader('last-modified', 'Tue, 01 Jul 2025 10:00:00 GMT');
    res.end('<html><head><title>Guide</title><meta name="article:published_time" content="2025-02-01"></head><body><p>Intro text.</p><p>The outbox row is written in the same transaction as the business change.</p></body></html>');
  });
  try {
    const env = { OCTOCODE_NET_GUARD_TEST_LOOPBACK: '1' };
    const hit = await run('read-page.mjs', ['--url', `${base}/g`, '--grep', 'SAME TRANSACTION', '--context', '20'], env);
    assert.equal(hit.status, 0, hit.stdout);
    assert.equal(hit.json.matches.length, 1);
    assert.match(hit.json.matches[0].text, /written in the same transaction as the/);
    assert.equal(hit.json.published, '2025-02-01');
    assert.equal(hit.json.modified, 'Tue, 01 Jul 2025 10:00:00 GMT');
    const capped = await run('read-page.mjs', ['--url', `${base}/g`, '--grep', 't', '--max-matches', '2'], env);
    assert.equal(capped.json.matches.length, 2);
    assert.ok(capped.json.counts.t > 2, 'the total per phrase is reported even when the shown matches are capped');
    assert.equal(capped.json.truncated, true);
    const miss = await run('read-page.mjs', ['--url', `${base}/g`, '--grep', 'event sourcing'], env);
    assert.equal(miss.status, 1);
    assert.deepEqual(miss.json.missing, ['event sourcing']);
    const text = await run('read-page.mjs', ['--url', `${base}/g`, '--max-chars', '10'], env);
    assert.equal(text.json.text.length, 10);
    assert.ok(text.json.chars > 10);
    assert.equal(text.json.truncated, true);
    // A quote copied from read-page output verifies.
    const dir = mkdtempSync(join(tmpdir(), 'bs-read-'));
    try {
      const ledger = join(dir, 'l.jsonl');
      const quote = hit.json.matches[0].text.trim();
      writeFileSync(ledger, `${JSON.stringify({ claim: 'outbox shares the transaction', url: `${base}/g`, quote })}\n`);
      const verified = await run('verify-claims.mjs', ['--ledger', ledger], env);
      assert.equal(verified.status, 0, verified.stdout);
    } finally { rmSync(dir, { recursive: true, force: true }); }
  } finally { server.close(); }
  assert.equal(findMatches('A  b\nC', 'b c', 0)[0].text, 'b C');
  assert.equal(findMatches('x a x a x a', 'a', 0).length, 3);
  // The snippet is the page's own text (case, underscores, curly quotes kept), so a copied quote is verbatim.
  const page = 'Set max_slot_wal_keep_size to cap WAL. The “limit” [3] applies per slot.';
  assert.equal(findMatches(page, 'MAX_SLOT_WAL_KEEP_SIZE', 0)[0].text, 'max_slot_wal_keep_size');
  assert.equal(findMatches(page, '"limit" applies', 0)[0].text, '“limit” [3] applies');
  assert.equal(findMatches(page, 'cap wal', 4)[0].text, ' to cap WAL. Th');
  assert.ok(quoteInText(findMatches(page, 'the "limit" applies per', 0)[0].text, page), 'a snippet verifies against its page');
});

test('verify-claims counts several ledgers, reuses earlier results, checks the date window and links rows citing one source', async () => {
  let fetches = 0;
  const { server, base } = await serve((req, res) => {
    fetches += 1;
    if (req.url === '/moved') { res.statusCode = 301; res.setHeader('location', '/article'); return res.end(); }
    res.setHeader('content-type', 'text/html');
    if (req.url === '/old') return res.end('<html><head><meta name="article:published_time" content="2019-05-01"></head><body><p>Old results showed the effect held for small teams only.</p></body></html>');
    return res.end('<html><head><meta name="article:published_time" content="2025-03-01"></head><body><p>Teams that wrote a premortem found more failure modes than the control group.</p></body></html>');
  });
  const dir = mkdtempSync(join(tmpdir(), 'bs-ledgers-'));
  try {
    const a = join(dir, 'a.jsonl');
    const b = join(dir, 'b.jsonl');
    writeFileSync(a, [
      { id: 'A1', claim: 'premortems find more failure modes', url: `${base}/article`, quote: 'wrote a premortem found more failure modes', claimStatus: 'proven', primary: true },
      { id: 'A2', claim: 'premortems find more failure modes', url: `${base}/moved`, quote: 'found more failure modes than the control group', claimStatus: 'proven', primary: false },
    ].map((r) => JSON.stringify(r)).join('\n'));
    writeFileSync(b, `${JSON.stringify({ id: 'B1', claim: 'the effect is limited', url: `${base}/old`, quote: 'the effect held for small teams only', claimStatus: 'maybe' })}\n`);
    const env = { OCTOCODE_NET_GUARD_TEST_LOOPBACK: '1' };
    const r = await run('verify-claims.mjs', ['--ledger', a, '--ledger', b, '--since', '2024-01-01'], env);
    assert.equal(r.status, 0, r.stdout);
    assert.deepEqual(r.json.ledgers.map((l) => [l.total, l.verified]), [[2, 2], [1, 1]]);
    assert.deepEqual(r.json.claims.map((c) => c.dateInWindow), ['yes', 'yes', 'no']);
    assert.deepEqual(r.json.claims[0].sameSourceAs, ['A2']);
    assert.equal(r.json.claims[0].distinctSources, 1, 'a redirect to the same page is not a second source');
    assert.deepEqual(r.json.claimStatus, { counts: { proven: 2, maybe: 1 }, invalid: ['maybe'] });
    assert.deepEqual(r.json.primary.counts, { true: 1, false: 1 });
    assert.ok('modified' in r.json.claims[0]);
    const outFlag = await run('verify-claims.mjs', ['--ledger', a, '--ledger', b, '--out', join(dir, 'x.jsonl')], env);
    assert.equal(outFlag.status, 2);

    const out = join(dir, 'a.verified.jsonl');
    await run('verify-claims.mjs', ['--ledger', a, '--out', out], env);
    writeFileSync(a, `${readFileSync(a, 'utf8')}\n${JSON.stringify({ id: 'A3', claim: 'new row', url: `${base}/old`, quote: 'Old results showed the effect held' })}`);
    fetches = 0;
    const again = await run('verify-claims.mjs', ['--ledger', a, '--reuse', out], env);
    assert.equal(again.status, 0, again.stdout);
    assert.equal(again.json.reused, 2);
    assert.equal(fetches, 1, 'only the new row is fetched');
  } finally { server.close(); rmSync(dir, { recursive: true, force: true }); }
});

test('page dates come from the article, not a deploy header, and fall back to a dated sentence in the text', () => {
  assert.equal(textDate('Query support. Last tested on 2026-10-05 against Postgres 17.'), '2026-10-05');
  assert.equal(textDate('Updated: October 5, 2026. Body.'), '2026-10-05');
  assert.equal(textDate('No date here, only 1999 people.'), null);
  // A server's Last-Modified is when the site was deployed: it never decides the window.
  assert.equal(dateInWindow({ published: null, modified: 'Tue, 01 Jul 2025 10:00:00 GMT', modifiedFrom: 'http' }, '2025-01-01', null), 'unknown');
  assert.equal(dateInWindow({ published: null, modified: null, textDate: '2026-10-05' }, '2026-01-01', null), 'yes');
});

test('verify-claims groups rows by the deciding claim they serve and summarizes the date window', async () => {
  const { server, base } = await serve((req, res) => {
    res.setHeader('content-type', 'text/html');
    if (req.url === '/b') return res.end('<html><body><p>Independent tests found the pooler halved tail latency for short transactions.</p></body></html>');
    if (req.url === '/c') return res.end('<html><head><meta name="article:published_time" content="2025-05-01"></head><body><p>Transaction pooling breaks session-level prepared statements in older versions.</p></body></html>');
    return res.end('<html><head><meta name="article:published_time" content="2025-03-01"></head><body><p>The pooler halved tail latency for short transactions in our benchmark.</p></body></html>');
  });
  const dir = mkdtempSync(join(tmpdir(), 'bs-decides-'));
  try {
    const ledger = join(dir, 'l.jsonl');
    writeFileSync(ledger, [
      { id: 'A', decides: 'O3', direction: 'supports', claim: 'vendor benchmark: pooler halves tail latency', url: `${base}/a`, quote: 'The pooler halved tail latency for short transactions', claimStatus: 'unresolved', stake: 'vendor' },
      { id: 'B', decides: 'O3', direction: 'supports', claim: 'independent test: pooler halves tail latency', url: `${base}/b`, quote: 'found the pooler halved tail latency for short transactions', claimStatus: 'unresolved' },
      { id: 'C', decides: 'O3', direction: 'refutes', claim: 'transaction pooling breaks prepared statements', url: `${base}/c`, quote: 'Transaction pooling breaks session-level prepared statements', claimStatus: 'proven' },
    ].map((r) => JSON.stringify(r)).join('\n'));
    const r = await run('verify-claims.mjs', ['--ledger', ledger, '--since', '2025-01-01'], { OCTOCODE_NET_GUARD_TEST_LOOPBACK: '1' });
    assert.equal(r.status, 0, r.stdout);
    assert.deepEqual(r.json.decidingClaims.O3, { rows: 3, verified: 3, distinctSources: 3, supports: 2, refutes: 1, claimStatus: ['proven', 'unresolved'], conflictingStatus: true });
    assert.deepEqual(r.json.claims.map((c) => c.distinctSources), [3, 3, 3], 'sources are counted per deciding claim, not per row text');
    assert.deepEqual(r.json.dateWindow, { since: '2025-01-01', until: null, yes: 2, no: 0, unknown: 1 });
  } finally { server.close(); rmSync(dir, { recursive: true, force: true }); }
});

test('a timestamp several pages of one site share is a build stamp, not an article date', async () => {
  const stamp = '2026-11-12T14:05:59.542114';
  const { server, base } = await serve((req, res) => {
    res.setHeader('content-type', 'text/html');
    const own = req.url === '/blog' ? '2025-02-01' : stamp;
    res.end(`<html><head><meta property="article:published_time" content="${own}"></head><body><p>Hot standby cancels conflicting queries after the configured delay on ${req.url}.</p></body></html>`);
  });
  const dir = mkdtempSync(join(tmpdir(), 'bs-build-'));
  try {
    const ledger = join(dir, 'l.jsonl');
    writeFileSync(ledger, ['/docs/a', '/docs/b', '/blog'].map((path, i) => JSON.stringify({ id: `D${i}`, claim: `page ${path}`, url: `${base}${path}`, quote: `cancels conflicting queries after the configured delay on ${path}` })).join('\n'));
    const r = await run('verify-claims.mjs', ['--ledger', ledger, '--since', '2025-01-01'], { OCTOCODE_NET_GUARD_TEST_LOOPBACK: '1' });
    assert.equal(r.status, 0, r.stdout);
    assert.deepEqual(r.json.claims.map((c) => [c.publishedFrom ?? null, c.dateInWindow]), [['site-build', 'unknown'], ['site-build', 'unknown'], [null, 'yes']]);
  } finally { server.close(); rmSync(dir, { recursive: true, force: true }); }
});

test('verify-claims exits 1 when nothing verifies and 2 on a malformed ledger', async () => {
  const dir = mkdtempSync(join(tmpdir(), 'brainstorm-verify-'));
  try {
    const empty = join(dir, 'none.jsonl');
    writeFileSync(empty, JSON.stringify({ id: 1, claim: 'x', url: 'ftp://x', quote: 'q' }));
    const none = await run('verify-claims.mjs', ['--ledger', empty]);
    assert.equal(none.status, 1);
    assert.equal(none.json.error.code, 'noneVerified');
    const broken = join(dir, 'broken.jsonl');
    writeFileSync(broken, '{not json');
    const bad = await run('verify-claims.mjs', ['--ledger', broken]);
    assert.equal(bad.status, 2);
    assert.equal(bad.json.error.code, 'invalidArguments');
    writeFileSync(broken, 'null\n42\n[]\n{"claim":"x","quote":"long enough source words to test"}');
    const invalidRows = await run('verify-claims.mjs', ['--ledger', broken]);
    assert.equal(invalidRows.status, 1);
    assert.deepEqual(invalidRows.json.claims.map((c) => c.status), ['missing-field', 'missing-field', 'missing-field', 'missing-field']);
  } finally { rmSync(dir, { recursive: true, force: true }); }
});

test('the network guard refuses private hosts, IPv4-mapped IPv6, and private redirect targets', async () => {
  const { privateAddress, urlProblem, guardedFetch } = await import('../lib/net-guard.mjs');
  for (const ip of ['127.0.0.1', '10.0.0.1', '169.254.169.254', '100.64.0.1', '::1', '::ffff:127.0.0.1', '::ffff:7f00:1', '::ffff:10.0.0.1', 'fe80::1', 'fd00::1']) assert.ok(privateAddress(ip), ip);
  for (const ip of ['8.8.8.8', '2606:4700::1111']) assert.ok(!privateAddress(ip), ip);
  assert.match(urlProblem('https://[::ffff:7f00:1]/x'), /private/);
  assert.match(urlProblem('http://metadata.internal/x'), /private/);
  assert.equal(urlProblem('https://example.com/x'), null);
  process.env.OCTOCODE_NET_GUARD_TEST_LOOPBACK = '1';
  const { server, base } = await serve((req, res) => { res.statusCode = 302; res.setHeader('location', 'http://10.0.0.1/secret'); res.end(); });
  try {
    await assert.rejects(guardedFetch(`${base}/hop`), (error) => error.guard === 'http://10.0.0.1/secret');
  } finally { server.close(); delete process.env.OCTOCODE_NET_GUARD_TEST_LOOPBACK; }
});

test('the connect-time lookup refuses a private DNS answer, which is what stops DNS rebinding', async () => {
  const { connectLookup } = await import('../lib/net-guard.mjs');
  const refused = await new Promise((resolve) => connectLookup(false)('localhost', {}, (error) => resolve(error)));
  assert.equal(refused?.code, 'EPRIVATEADDRESS');
  const allowed = await new Promise((resolve) => connectLookup(true)('localhost', { all: true }, (error, addresses) => resolve(error || addresses)));
  assert.ok(Array.isArray(allowed) && allowed.length, 'an exempt origin still resolves');
});

test('verify-claims refuses private hosts and keeps manual checks only where the script could not read', async () => {
  const dir = mkdtempSync(join(tmpdir(), 'brainstorm-verify-'));
  try {
    const ledger = join(dir, 'ledger.jsonl');
    writeFileSync(ledger, [
      { id: 'P1', claim: 'internal', url: 'http://169.254.169.254/latest/meta-data/', quote: 'a quote that should never be fetched at all' },
      { id: 'M1', claim: 'manual pdf', url: 'https://[::ffff:7f00:1]/paper.pdf', quote: 'a quote that a person read in the pdf', verifiedBy: 'manual', method: 'read in the host PDF viewer' },
    ].map((r) => JSON.stringify(r)).join('\n'));
    const r = await run('verify-claims.mjs', ['--ledger', ledger]);
    assert.equal(r.status, 1);
    assert.deepEqual(r.json.claims.map((c) => c.status), ['refused', 'refused'], 'a manual mark must not override a refused host');
  } finally { rmSync(dir, { recursive: true, force: true }); }
});

test('fetch-asset accepts only https public hosts and document or image types', async () => {
  const { urlProblem, allowedType } = await import('../fetch-asset.mjs');
  assert.equal(urlProblem('https://arxiv.org/pdf/1234'), null);
  assert.match(urlProblem('http://example.com/a.pdf'), /https/);
  assert.match(urlProblem('https://192.168.1.4/a.pdf'), /private/);
  assert.match(urlProblem('https://localhost/a.pdf'), /private/);
  assert.match(urlProblem('https://user:pw@example.com/a.pdf'), /credentials/);
  assert.match(urlProblem('https://[::ffff:127.0.0.1]/a.pdf'), /private/);
  assert.equal(allowedType('application/pdf; charset=binary'), 'application/pdf');
  assert.equal(allowedType('application/zip'), null);
  assert.equal(allowedType('application/x-msdownload'), null);
});

test('fetch-asset plans without saving, downloads only with --approve, and records a manifest', async () => {
  const pdf = Buffer.from('%PDF-1.4 fixture');
  const { server, base } = await serve((req, res) => {
    if (req.url === '/paper.pdf' || req.url === '/paper-copy.pdf') { res.setHeader('content-type', 'application/pdf'); res.setHeader('content-length', pdf.length); return res.end(req.method === 'HEAD' ? undefined : pdf); }
    if (req.url === '/notes.txt' || req.url === '/notes-copy.txt') { res.setHeader('content-type', 'text/plain'); return res.end(req.method === 'HEAD' ? undefined : 'Teams that consider the opposite reduced their overconfidence in the study.'); }
    if (req.url === '/bundle.zip') { res.setHeader('content-type', 'application/zip'); return res.end(); }
    if (req.url === '/big.pdf') { res.setHeader('content-type', 'application/pdf'); res.setHeader('content-length', String(3_000_000)); return res.end(req.method === 'HEAD' ? undefined : Buffer.alloc(3_000_000)); }
    res.statusCode = 404; return res.end();
  });
  const dir = mkdtempSync(join(tmpdir(), 'brainstorm-assets-'));
  const runDir = join(dir, '.octocode', 'octocode-brainstorming', 'tmp', 'run');
  const env = { OCTOCODE_NET_GUARD_TEST_LOOPBACK: '1' };
  try {
    const plan = await run('fetch-asset.mjs', ['--run-dir', runDir, '--url', `${base}/paper.pdf`, '--url', `${base}/bundle.zip`, '--url', `${base}/big.pdf`, '--max-mb', '2'], env);
    assert.equal(plan.status, 1, plan.stdout);
    assert.equal(plan.json.approved, false);
    assert.deepEqual(plan.json.plan.map((p) => p.accepted), [true, false, false]);
    assert.match(plan.json.plan[1].reason, /not an allowed/);
    assert.match(plan.json.plan[2].reason, /exceeds/);
    assert.ok(!existsSync(join(runDir, 'assets')), 'planning must not save anything');

    const got = await run('fetch-asset.mjs', ['--run-dir', runDir, '--url', `${base}/paper.pdf`, '--url', `${base}/notes.txt`, '--approve'], env);
    assert.equal(got.status, 0, got.stdout);
    assert.deepEqual(got.json.results.map((r) => r.status), ['downloaded', 'downloaded']);
    assert.ok(readFileSync(got.json.results[0].path).equals(pdf));
    const again = await run('fetch-asset.mjs', ['--run-dir', runDir, '--url', `${base}/paper.pdf`, '--approve'], env);
    assert.equal(again.json.results[0].status, 'already-downloaded');
    assert.equal(readFileSync(join(runDir, 'assets', 'manifest.jsonl'), 'utf8').trim().split('\n').length, 2);

    const ledger = join(runDir, 'ledger.jsonl');
    const txt = got.json.results[1].file;
    writeFileSync(ledger, [
      { id: 'T1', claim: 'Opposite reduces overconfidence', url: `${base}/notes.txt`, asset: txt, quote: 'consider the opposite reduced their overconfidence' },
      { id: 'T2', claim: 'Invented', url: `${base}/notes.txt`, asset: txt, quote: 'consider the opposite eliminated every bias completely' },
    ].map((r) => JSON.stringify(r)).join('\n'));
    const verified = await run('verify-claims.mjs', ['--ledger', ledger], env);
    assert.deepEqual(verified.json.claims.map((c) => c.status), ['verified', 'quote-not-found'], verified.stdout);

    writeFileSync(ledger, [
      { id: 'B1', claim: 'Wrong origin', url: `${base}/different.txt`, asset: txt, quote: 'consider the opposite reduced their overconfidence' },
      { id: 'B2', claim: 'Outside run', url: `${base}/notes.txt`, asset: '../ledger.jsonl', quote: 'consider the opposite reduced their overconfidence' },
    ].map((r) => JSON.stringify(r)).join('\n'));
    const unbound = await run('verify-claims.mjs', ['--ledger', ledger], env);
    assert.deepEqual(unbound.json.claims.map((c) => c.status), ['refused', 'refused']);

    const sameBytes = await run('fetch-asset.mjs', ['--run-dir', runDir, '--url', `${base}/notes-copy.txt`, '--approve'], env);
    assert.equal(sameBytes.status, 0);
    assert.equal(sameBytes.json.results[0].status, 'already-downloaded');
    assert.equal(readFileSync(join(runDir, 'assets', 'manifest.jsonl'), 'utf8').trim().split('\n').length, 3);
    writeFileSync(ledger, JSON.stringify({ id: 'B4', claim: 'Second origin', url: `${base}/notes-copy.txt`, asset: sameBytes.json.results[0].file, quote: 'consider the opposite reduced their overconfidence' }));
    const secondOrigin = await run('verify-claims.mjs', ['--ledger', ledger], env);
    assert.equal(secondOrigin.json.claims[0].status, 'verified');

    const manifestPath = join(runDir, 'assets', 'manifest.jsonl');
    const manifest = readFileSync(manifestPath, 'utf8');
    writeFileSync(manifestPath, `null\n${manifest}`);
    const nullEntry = await run('verify-claims.mjs', ['--ledger', ledger], env);
    assert.equal(nullEntry.json.claims[0].status, 'verified');
    writeFileSync(manifestPath, '{broken json\n');
    const malformedManifest = await run('verify-claims.mjs', ['--ledger', ledger], env);
    assert.equal(malformedManifest.json.claims[0].status, 'missing-field');
    writeFileSync(manifestPath, manifest);

    writeFileSync(got.json.results[1].path, 'consider the opposite reduced their overconfidence in a changed file');
    const changed = await run('verify-claims.mjs', ['--ledger', ledger], env);
    assert.deepEqual(changed.json.claims.map((c) => c.status), ['refused']);

    const outside = await run('fetch-asset.mjs', ['--run-dir', join(dir, 'elsewhere'), '--url', `${base}/paper.pdf`], env);
    assert.equal(outside.status, 2);
    assert.equal(outside.json.error.code, 'invalidArguments');
  } finally { server.close(); rmSync(dir, { recursive: true, force: true }); }
});

test('every script runs when called through a symlink, as installed skills are', async () => {
  const dir = mkdtempSync(join(tmpdir(), 'bs-link-'));
  try {
    for (const script of ['search.mjs', 'verify-claims.mjs', 'fetch-asset.mjs', 'keys.mjs', 'read-page.mjs']) {
      const link = join(dir, script);
      symlinkSync(join(scripts, script), link);
      const r = await new Promise((resolve) => {
        const child = spawn(process.execPath, [link, '--help']);
        let stdout = '';
        child.stdout.on('data', (d) => { stdout += d; });
        child.on('close', (status) => resolve({ status, stdout }));
      });
      assert.equal(r.status, 0, script);
      assert.match(r.stdout, /^Usage: /, `${script} printed nothing through a symlink`);
    }
  } finally { rmSync(dir, { recursive: true, force: true }); }
});

// A repository's `.octocode/.env` must never choose where the user's key goes:
// a home API key sent to a repository-chosen host is a stolen key.
test('a workspace .env cannot redirect a home search key to its own host', async () => {
  const script = fileURLToPath(new URL('../search.mjs', import.meta.url));
  for (const [provider, keyName, baseName] of [
    ['tavily', 'TAVILY_API_KEY', 'TAVILY_BASE_URL'],
    ['exa', 'EXA_API_KEY', 'EXA_BASE_URL'],
    ['serper', 'SERPER_API_KEY', 'SERPER_BASE_URL'],
    ['deepseek', 'DEEPSEEK_API_KEY', 'DEEPSEEK_BASE_URL'],
  ]) {
    const seen = [];
    const attacker = createServer((request, response) => {
      seen.push(request.url);
      response.setHeader('content-type', 'application/json');
      response.end('{"results":[],"organic":[]}');
    });
    await new Promise(resolve => attacker.listen(0, '127.0.0.1', resolve));
    const base = `http://127.0.0.1:${attacker.address().port}`;
    const home = mkdtempSync(join(tmpdir(), 'search-home-'));
    const workspace = mkdtempSync(join(tmpdir(), 'search-workspace-'));
    writeFileSync(join(home, '.env'), `${keyName}=home-secret-key\n`);
    mkdirSync(join(workspace, '.octocode'));
    writeFileSync(join(workspace, '.octocode', '.env'), `${baseName}=${base}\n`);
    const child = spawn(process.execPath, [script, '--query', 'hello', '--providers', provider], {
      cwd: workspace,
      env: { PATH: process.env.PATH, HOME: home, OCTOCODE_HOME: home },
      stdio: 'ignore',
    });
    // The request to the real provider may fail offline; only its target matters.
    const stop = setTimeout(() => child.kill(), 20_000);
    await new Promise(resolve => child.on('exit', resolve));
    clearTimeout(stop);
    attacker.close();
    assert.deepEqual(seen, [], `${provider}: the repository's host received a request`);
  }
});
