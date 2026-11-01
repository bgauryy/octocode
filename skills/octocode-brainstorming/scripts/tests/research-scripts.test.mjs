import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createServer } from 'node:http';
import { mkdtempSync, rmSync, writeFileSync, readFileSync, existsSync, symlinkSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawn } from 'node:child_process';
import { canonicalUrl } from '../search.mjs';
import { normalize, quoteInText, pageDates } from '../verify-claims.mjs';

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
    if (req.url === '/tavily/search') return res.end(JSON.stringify({ results: [{ url: 'https://www.a.test/x/', title: 'A', content: 'tavily snippet', published_date: '2025-01-02' }, { url: 'https://b.test/only-tavily', title: 'B', content: 'b' }] }));
    if (req.url === '/exa/search') { assert.equal(body.category, 'research paper'); return res.end(JSON.stringify({ results: [{ url: 'https://a.test/x?utm_source=feed', title: 'A', highlights: ['exa highlight'] }], costDollars: { total: 0.005 } })); }
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
    assert.match(r.json.providers.serper.fix, /octocode config set SERPER_API_KEY --stdin/);
    assert.equal(r.json.providers.exa.costDollars, 0.005);
  } finally { server.close(); }
});

test('search names a keysRejected setup error when every provider rejects its key', async () => {
  const { server, base } = await serve(async (req, res) => {
    res.setHeader('content-type', 'application/json');
    res.statusCode = req.url.startsWith('/serper') ? 403 : 401;
    return res.end(JSON.stringify({ error: 'Invalid API key' }));
  });
  try {
    const env = { TAVILY_API_KEY: 'k1', EXA_API_KEY: 'k2', SERPER_API_KEY: 'k3', TAVILY_BASE_URL: `${base}/tavily`, EXA_BASE_URL: `${base}/exa`, SERPER_BASE_URL: `${base}/serper` };
    const r = await run('search.mjs', ['--query', 'q'], env);
    assert.equal(r.status, 1);
    assert.equal(r.json.error.code, 'keysRejected');
    assert.match(r.json.error.message, /tavily, exa, serper/);
    for (const name of ['tavily', 'exa', 'serper']) assert.equal(r.json.providers[name].status, 'keyRejected');
  } finally { server.close(); }
});

test('search reports a missing key and invalid input with the standard error shape', async () => {
  const none = await run('search.mjs', ['--query', 'q'], {});
  assert.equal(none.status, 1);
  assert.equal(none.json.error.code, 'missingKey');
  const bad = await run('search.mjs', ['--query', 'q', '--kind', 'blogs'], { EXA_API_KEY: 'k' });
  assert.equal(bad.status, 2);
  assert.equal(bad.json.error.code, 'invalidArguments');
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
    writeFileSync(manifestPath, 'null\n' + manifest);
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
    for (const script of ['search.mjs', 'verify-claims.mjs', 'fetch-asset.mjs']) {
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
