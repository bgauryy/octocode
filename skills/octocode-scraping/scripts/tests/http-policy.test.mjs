import { after, before, test } from 'node:test';
import assert from 'node:assert/strict';
import { createServer } from 'node:http';
import { discoverSitemap, fetchDirect, fetchRobotsPolicy } from '../lib/client.mjs';
import { spawn } from 'node:child_process';
import { readFileSync, readdirSync } from 'node:fs';
import { join } from 'node:path';
import { scripts, withDir } from './helpers.mjs';

// Fixtures run on plain-HTTP 127.0.0.1; the network guard allows exactly that under this test flag.
process.env.OCTOCODE_NET_GUARD_TEST_LOOPBACK = '1';

let server;
let origin;
let retryRequests = 0;

before(async () => {
  server = createServer((req, res) => {
    if (req.url === '/robots.txt') {
      res.writeHead(200, { 'content-type': 'text/plain' });
      res.end('User-agent: *\nDisallow: /private\nAllow: /private/public\n');
      return;
    }
    if (req.url === '/retry') {
      retryRequests += 1;
      if (retryRequests === 1) {
        res.writeHead(503, { 'retry-after': '0' });
        res.end('wait');
      } else {
        res.writeHead(200, { 'content-type': 'text/html' });
        res.end('<h1>ready</h1>');
      }
      return;
    }
    if (req.url === '/sitemap.xml') {
      res.writeHead(200, { 'content-type': 'application/xml' });
      res.end(`<urlset><url><loc>${origin}/one</loc></url><url><loc>https://outside.invalid/two</loc></url></urlset>`);
      return;
    }
    if (req.url === '/large') {
      res.writeHead(200, { 'content-type': 'text/html' });
      res.end('x'.repeat(100_000));
      return;
    }
    res.writeHead(200, { 'content-type': 'text/html' });
    res.end('<h1>ok</h1>');
  });
  await new Promise((resolve) => server.listen(0, '127.0.0.1', resolve));
  origin = `http://127.0.0.1:${server.address().port}`;
});

after(async () => {
  await new Promise((resolve, reject) => server.close((error) => error ? reject(error) : resolve()));
});

test('robots uses longest matching rule and allow wins the more specific path', async () => {
  const policy = await fetchRobotsPolicy(`${origin}/docs`);
  assert.equal(policy.check(`${origin}/docs`).allowed, true);
  assert.equal(policy.check(`${origin}/private/a`).allowed, false);
  assert.equal(policy.check(`${origin}/private/public/a`).allowed, true);
});

test('direct fetch honors a short Retry-After once', async () => {
  const result = await fetchDirect({ url: `${origin}/retry`, pageId: 'page-001', config: { maxRawBytes: 10_000, maxTextBytes: 10_000 } });
  assert.equal(result.status, 200);
  assert.equal(retryRequests, 2);
  assert.match(result.body, /ready/);
});

test('direct fetch stops reading at the configured response bound', async () => {
  const result = await fetchDirect({ url: `${origin}/large`, pageId: 'page-001', config: { maxRawBytes: 64_000, maxTextBytes: 1_000 } });
  assert.equal(Buffer.byteLength(result.body), 64_000);
  assert.equal(result.bodyTruncated, true);
});

test('sitemap discovery stays on the requested domain', async () => {
  const result = await discoverSitemap({ targetUrl: `${origin}/docs`, maxPages: 5 });
  assert.equal(result.error, null);
  assert.deepEqual(result.discovered, [`${origin}/one`]);
});

test('a sitemap index is followed to its child sitemaps', async () => {
  const indexServer = createServer((req, res) => {
    const base = `http://127.0.0.1:${indexServer.address().port}`;
    if (req.url === '/sitemap.xml') res.end(`<?xml version="1.0"?><sitemapindex><sitemap><loc>${base}/sm1.xml</loc></sitemap></sitemapindex>`);
    else if (req.url === '/sm1.xml') res.end(`<urlset><url><loc>${base}/a</loc></url><url><loc>${base}/b</loc></url></urlset>`);
    else { res.statusCode = 404; res.end(); }
  });
  await new Promise((r) => indexServer.listen(0, '127.0.0.1', r));
  try {
    const base = `http://127.0.0.1:${indexServer.address().port}`;
    const { discovered, error } = await discoverSitemap({ targetUrl: `${base}/`, maxPages: 5 });
    assert.equal(error, null);
    assert.deepEqual(discovered, [`${base}/a`, `${base}/b`]);
  } finally { indexServer.close(); }
});

test('a crawl refuses private hosts it is linked to or redirected to, and the start URL needs --allow-private-host', async () => withDir(async (dir) => {
  const internal = createServer((req, res) => { res.setHeader('content-type', 'text/html'); res.end('<h1>internal secret</h1>'); });
  await new Promise((r) => internal.listen(0, '127.0.0.1', r));
  const internalPort = internal.address().port;
  const site = createServer((req, res) => {
    if (req.url === '/robots.txt') { res.statusCode = 404; return res.end(); }
    if (req.url === '/hop') { res.statusCode = 302; res.setHeader('location', `http://localhost:${internalPort}/via-redirect`); return res.end(); }
    res.setHeader('content-type', 'text/html');
    return res.end(`<a href="http://localhost:${internalPort}/internal">a</a> <a href="http://169.254.169.254/latest/meta-data/">b</a> <a href="/hop">c</a>`);
  });
  await new Promise((r) => site.listen(0, '127.0.0.1', r));
  const start = `http://127.0.0.1:${site.address().port}/`;
  // The fixture servers live in this process, so the child must run without blocking the event loop.
  const nodeAsync = (args) => new Promise((resolve) => {
    const child = spawn(process.execPath, [join(scripts, 'fetch.mjs'), ...args], { cwd: dir, env: { PATH: process.env.PATH } });
    let stdout = '';
    child.stdout.on('data', (d) => { stdout += d; });
    child.on('close', (status) => resolve({ status, stdout }));
  });
  try {
    const denied = await nodeAsync(['--url', start, '--provider', 'direct']);
    assert.equal(denied.status, 2, denied.stdout);
    assert.equal(JSON.parse(denied.stdout).error.code, 'refusedHost');

    const crawl = await nodeAsync(['--url', start, '--provider', 'direct', '--allow-private-host', '--crawl', '--span-hosts', '--max-pages', '5', '--delay-ms', '0']);
    const out = JSON.parse(crawl.stdout);
    const refusedUrls = out.warnings.filter((w) => w.warning.startsWith('refused ')).map((w) => w.warning.split(' ')[1].replace(/:$/, ''));
    assert.ok(refusedUrls.includes(`http://localhost:${internalPort}/internal`), crawl.stdout);
    assert.ok(refusedUrls.includes('http://169.254.169.254/latest/meta-data/'), crawl.stdout);
    const corpus = readdirSync(join(out.sessionDir, 'text')).map((f) => readFileSync(join(out.sessionDir, 'text', f), 'utf8')).join('\n');
    assert.ok(!corpus.includes('internal secret'), 'an internal page reached the corpus');
    const sources = readFileSync(join(out.sessionDir, 'sources.jsonl'), 'utf8');
    assert.match(sources, /redirect target refused/, sources);
  } finally { site.close(); internal.close(); }
}));
