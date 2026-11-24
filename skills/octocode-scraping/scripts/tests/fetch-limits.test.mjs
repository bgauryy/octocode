import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { node, withDir } from './helpers.mjs';

const fetchIn = (dir, args) => {
  const res = node('fetch.mjs', args, { cwd: dir, env: { ...process.env, SCRAPING_ANT: '' } });
  return { status: res.status, out: JSON.parse(res.stdout) };
};
const fetchedUrls = (sessionDir) => readFileSync(join(sessionDir, 'sources.jsonl'), 'utf8').trim().split('\n').map((line) => JSON.parse(line).url);
const links = (count) => Array.from({ length: count }, (_, i) => `<a href="https://ex.test/p${i}">p${i}</a>`).join('');

test('--max-credits stops before a request that could pass the cap and lists every unfetched URL', () => withDir((dir) => {
  const body = join(dir, 'body.html');
  writeFileSync(body, `<html><body><h1>Root</h1>${links(6)}</body></html>`);
  const hosted = ['--url', 'https://ex.test/', '--provider', 'scrapingant', '--mock-status', '200', '--mock-content-type', 'text/html',
    '--mock-body-file', body, '--crawl', '--max-pages', '7', '--delay-ms', '0'];
  const capped = fetchIn(dir, [...hosted, '--mock-credit-cost', '10', '--max-credits', '35', '--session', 'capped']);
  assert.equal(capped.status, 0, JSON.stringify(capped.out));
  const cap = capped.out.creditCap;
  // 10 credits a request: after 3 requests (30), a fourth could reach 40 > 35.
  assert.equal(cap.spentThisRun, 30);
  assert.ok(cap.spentThisRun + cap.largestRequest > cap.maxCredits);
  assert.ok(cap.spentThisRun <= cap.maxCredits);
  assert.equal(capped.out.newPages, 3);
  assert.equal(cap.stopped, true);
  assert.equal(cap.unfetched, cap.unfetchedUrls.length);
  const fetched = new Set(fetchedUrls(capped.out.sessionDir));
  assert.equal(fetched.size + cap.unfetched, 7, 'fetched plus unfetched covers the whole queue');
  assert.ok(cap.unfetchedUrls.every((url) => !fetched.has(url)));
  assert.ok(capped.out.warnings.some((w) => /credit cap stopped the run/.test(w.warning)));

  const unknown = fetchIn(dir, [...hosted, '--max-credits', '100', '--session', 'unknown']);
  assert.equal(unknown.out.newPages, 1, 'a reply without a credit header stops the capped run');
  assert.equal(unknown.out.creditCap.costUnknownAt, 'page-001');
  assert.equal(unknown.out.creditCap.stopped, true);

  const uncapped = fetchIn(dir, [...hosted, '--mock-credit-cost', '10', '--session', 'uncapped']);
  assert.equal(uncapped.out.creditCap, null);
  assert.equal(uncapped.out.newPages, 7);
  for (const value of ['0', '-1', 'ten']) assert.equal(fetchIn(dir, [...hosted, '--max-credits', value]).status, 2, value);
}));

test('--follow pagination crawls only rel=next/prev and pagination-class links', () => withDir((dir) => {
  const body = join(dir, 'listing.html');
  writeFileSync(body, '<html><body><h1>Results</h1><a href="https://ex.test/about">About</a><a href="https://ex.test/docs">Docs</a>'
    + '<a rel="next" href="https://ex.test/list?page=2">Next</a><nav><a class="pagination-link" href="https://ex.test/list?page=3">3</a></nav></body></html>');
  const direct = ['--url', 'https://ex.test/list', '--provider', 'direct', '--mock-status', '200', '--mock-content-type', 'text/html',
    '--mock-body-file', body, '--crawl', '--max-pages', '10', '--delay-ms', '0'];
  const paged = fetchIn(dir, [...direct, '--follow', 'pagination', '--session', 'paged']);
  assert.equal(paged.status, 0, JSON.stringify(paged.out));
  assert.deepEqual(fetchedUrls(paged.out.sessionDir), ['https://ex.test/list', 'https://ex.test/list?page=2', 'https://ex.test/list?page=3']);
  const all = fetchIn(dir, [...direct, '--session', 'all']);
  assert.ok(fetchedUrls(all.out.sessionDir).includes('https://ex.test/about'), 'without --follow every same-host link is crawled');

  writeFileSync(body, '<html><body><h1>One page</h1><a href="https://ex.test/about">About</a></body></html>');
  const single = fetchIn(dir, [...direct, '--follow', 'pagination', '--session', 'single']);
  assert.equal(single.out.newPages, 1);
  assert.ok(single.out.warnings.some((w) => /no pagination link/.test(w.warning)));
  for (const extra of [['--follow', 'links'], ['--follow', 'pagination', '--sitemap']]) assert.equal(fetchIn(dir, [...direct, ...extra]).status, 2, extra.join(' '));
  const noCrawl = fetchIn(dir, ['--url', 'https://ex.test/list', '--provider', 'direct', '--follow', 'pagination']);
  assert.equal(noCrawl.status, 2);
  assert.match(noCrawl.out.error.message, /requires --crawl/);
}));

test('hosted runs report the options they sent without the key or page URL', () => withDir((dir) => {
  const res = node('fetch.mjs', ['--url', 'https://ex.test/', '--provider', 'scrapingant', '--mock-status', '422', '--browser', '--proxy-country', 'de',
    '--block-resource', 'image', '--block-resource', 'font', '--param', 'js_snippet=x', '--session', 'hosted'], { cwd: dir, env: { ...process.env, SCRAPING_ANT: 'sk-secret-value-123' } });
  const out = JSON.parse(res.stdout);
  assert.equal(out.providerRequest.endpoint, 'https://api.scrapingant.com/v2/general');
  assert.deepEqual(out.providerRequest.params, { browser: 'true', proxy_country: 'de', block_resource: ['image', 'font'], js_snippet: 'x' });
  assert.ok(!res.stdout.includes('sk-secret-value-123'));
  const direct = node('fetch.mjs', ['--url', 'https://ex.test/', '--provider', 'direct', '--mock-status', '200', '--session', 'direct'], { cwd: dir });
  assert.equal(JSON.parse(direct.stdout).providerRequest, null);
}));
