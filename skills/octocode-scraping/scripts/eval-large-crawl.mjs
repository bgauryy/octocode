#!/usr/bin/env node
import { mkdir, readFile, rm, writeFile } from 'node:fs/promises';
import { existsSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { spawnSync } from 'node:child_process';

const here = new URL('.', import.meta.url).pathname;
const fetchScript = resolve(here, 'fetch.mjs');
const root = resolve(process.cwd());
const outBase = join(root, '.octocode/tmp/scrape-large-crawl-eval');
const checks = [];
const assert = (name, condition, detail = '') => checks.push({ name, ok: Boolean(condition), detail });

await rm(outBase, { recursive: true, force: true });
await mkdir(outBase, { recursive: true });

const PAGE_COUNT = 30;
const links = Array.from({ length: PAGE_COUNT }, (_, i) => `<a href="/page-${i + 1}">Page ${i + 1} docs</a>`).join(' ');
const html = `<!doctype html><html><head><title>Hub</title></head><body><h1>Hub</h1><nav>${links}</nav></body></html>`;
const mockFile = join(outBase, 'mock-hub.html');
await writeFile(mockFile, html);

const res = spawnSync(process.execPath, [fetchScript, '--url', 'https://example.com', '--mode', 'html', '--provider', 'direct', '--session', 'large-crawl', '--mock-status', '200', '--mock-content-type', 'text/html', '--mock-body-file', mockFile, '--crawl', '--same-domain', '--max-pages', String(PAGE_COUNT), '--out', outBase], { cwd: root, encoding: 'utf8' });
checks.push({ name: 'large crawl runs', ok: res.status === 0, status: res.status, stderr: res.stderr.slice(0, 500) });

const dir = join(outBase, 'large-crawl');
assert('pages-001.json exists', existsSync(join(dir, 'indexes/pages-001.json')));
assert('pages-002.json exists (>20 pages paginates)', existsSync(join(dir, 'indexes/pages-002.json')));

const agentIndex = JSON.parse(await readFile(join(dir, 'AGENT_INDEX.json'), 'utf8'));
assert('totals.pages matches crawl size', agentIndex.totals.pages === PAGE_COUNT, agentIndex.totals.pages);
assert('pagination reports multiple pages', agentIndex.pagination.pages.totalPages === 2, agentIndex.pagination.pages.totalPages);
assert('embedded pages array stays capped at pageSize', agentIndex.pages.length === agentIndex.pagination.pages.pageSize, agentIndex.pages.length);

const page2 = JSON.parse(await readFile(join(dir, 'indexes/pages-002.json'), 'utf8'));
assert('remaining pages present in page 2 index', page2.totalItems === PAGE_COUNT && page2.rows.length === PAGE_COUNT - agentIndex.pagination.pages.pageSize, page2.rows.length);

const indexBytes = Buffer.byteLength(await readFile(join(dir, 'AGENT_INDEX.json'), 'utf8'));
assert('AGENT_INDEX.json stays small regardless of crawl size', indexBytes < 20_000, indexBytes);

const failed = checks.filter((c) => !c.ok);
console.log(JSON.stringify({ ok: failed.length === 0, checks }, null, 2));
process.exit(failed.length === 0 ? 0 : 1);
