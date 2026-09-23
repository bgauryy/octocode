import { test, beforeEach, afterEach } from 'node:test';
import assert from 'node:assert/strict';
import { mkdtempSync, rmSync, mkdirSync, writeFileSync, readFileSync, existsSync, realpathSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';
import { workspaceRootFor } from '../lib/args.mjs';

const here = dirname(fileURLToPath(import.meta.url));
const script = join(here, '..', 'fetch.mjs');

let root;
beforeEach(() => {
  root = mkdtempSync(join(tmpdir(), 'fetch-session-test-'));
  writeFileSync(join(root, 'body.html'), '<html><head><title>T</title></head><body><p>hello corpus</p></body></html>');
});
afterEach(() => rmSync(root, { recursive: true, force: true }));

function run(args, cwd = root) {
  const res = spawnSync(process.execPath, [script, ...args], { encoding: 'utf8', cwd });
  let parsed = null;
  const text = (res.stdout || '').trim() || (res.stderr || '').trim();
  try { parsed = JSON.parse(text); } catch {}
  return { ...res, parsed };
}

const mockArgs = (url, extra = []) => [
  '--url', url, '--provider', 'direct',
  '--mock-status', '200', '--mock-body-file', join(root, 'body.html'), '--mock-content-type', 'text/html',
  ...extra,
];

test('reusing a populated --session without --append is refused; --append continues numbering', () => {
  const first = run(mockArgs('https://ex.test/a', ['--session', 'sess1']));
  assert.equal(first.status, 0, first.stderr);
  const sessionDir = first.parsed.sessionDir;
  assert.ok(existsSync(join(sessionDir, 'text', 'page-001.clean.part-001.md')));

  const clobber = run(mockArgs('https://ex.test/b', ['--session', 'sess1']));
  assert.equal(clobber.status, 2);
  assert.equal(clobber.parsed.code, 'SESSION_EXISTS');

  const append = run(mockArgs('https://ex.test/b', ['--session', 'sess1', '--append']));
  assert.equal(append.status, 0, append.stderr);
  const rows = readFileSync(join(sessionDir, 'sources.jsonl'), 'utf8').trim().split('\n').map((l) => JSON.parse(l));
  assert.deepEqual(rows.map((r) => r.pageId), ['page-001', 'page-002']);
  assert.deepEqual(rows.map((r) => r.url), ['https://ex.test/a', 'https://ex.test/b']);
  assert.ok(existsSync(join(sessionDir, 'text', 'page-001.clean.part-001.md')), 'prior page text must survive append');
  assert.ok(existsSync(join(sessionDir, 'text', 'page-002.clean.part-001.md')));
});

test('default output climbs out of a .octocode tree instead of nesting sessions', () => {
  const drift = join(root, '.octocode', 'tmp', 'scrape', 'old-session');
  mkdirSync(drift, { recursive: true });
  const res = run(mockArgs('https://ex.test/a'), drift);
  assert.equal(res.status, 0, res.stderr);
  assert.ok(res.parsed.sessionDir.startsWith(join(realpathSync(root), '.octocode', 'tmp', 'scrape')), res.parsed.sessionDir);
  assert.ok(!res.parsed.sessionDir.includes('old-session'), `nested session: ${res.parsed.sessionDir}`);
});

test('workspaceRootFor anchors above the first .octocode segment', () => {
  assert.equal(workspaceRootFor('/a/b/.octocode/tmp/scrape/x'), '/a/b');
  assert.equal(workspaceRootFor('/a/b'), '/a/b');
  assert.equal(workspaceRootFor('/a/.octocode/x/.octocode/y'), '/a');
});

test('auto-selected cdp transport failure falls back to direct', () => {
  // No --provider: auto picks cdp (chrome-devtools sibling exists in-repo);
  // mock status 0 simulates a CDP client failure with no HTTP answer.
  const res = run(['--url', 'https://ex.test/a', '--mock-status', '0', '--mock-body-file', join(root, 'body.html'), '--mock-content-type', 'text/html']);
  assert.ok(res.parsed, res.stderr);
  assert.equal(res.parsed.route, 'direct:html', JSON.stringify(res.parsed));
});

test('crawl treats fragment and trailing-slash variants as one page and skips non-http links', () => {
  writeFileSync(join(root, 'body.html'), '<html><body><h1>Doc</h1><p>one</p><p>two</p><a href="#top">t</a><a href="/a/">a</a><a href="/b">b</a><a href="/b#s">bs</a><a href="mailto:x@ex.test">m</a></body></html>');
  const res = run(mockArgs('https://ex.test/a', ['--crawl', '--same-domain', '--max-pages', '10', '--delay-ms', '0']));
  assert.equal(res.status, 0, res.stderr);
  const rows = readFileSync(join(res.parsed.sessionDir, 'sources.jsonl'), 'utf8').trim().split('\n').map((l) => JSON.parse(l));
  assert.deepEqual(rows.map((r) => r.url), ['https://ex.test/a', 'https://ex.test/b']);
  const text = readFileSync(join(res.parsed.sessionDir, 'text', 'page-001.clean.part-001.md'), 'utf8');
  assert.match(text, /^# Doc\n\none\n\ntwo/, 'HTML text keeps block structure as lines');
});
