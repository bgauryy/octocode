import { after, before, test } from 'node:test';
import assert from 'node:assert/strict';
import { createServer } from 'node:http';
import { discoverSitemap, fetchDirect, fetchRobotsPolicy } from '../lib/client.mjs';

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
  const result = await discoverSitemap({ targetUrl: `${origin}/docs`, maxPages: 5, sameDomain: true });
  assert.equal(result.error, null);
  assert.deepEqual(result.discovered, [`${origin}/one`]);
});
