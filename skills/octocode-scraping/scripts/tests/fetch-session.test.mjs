import { test, beforeEach, afterEach } from 'node:test';
import assert from 'node:assert/strict';
import { mkdtempSync, rmSync, mkdirSync, writeFileSync, readFileSync, existsSync, realpathSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawn, spawnSync } from 'node:child_process';
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
  assert.equal(clobber.parsed.error.code, 'sessionExists');

  const append = run(mockArgs('https://ex.test/b', ['--session', 'sess1', '--append']));
  assert.equal(append.status, 0, append.stderr);
  const rows = readFileSync(join(sessionDir, 'sources.jsonl'), 'utf8').trim().split('\n').map((l) => JSON.parse(l));
  assert.deepEqual(rows.map((r) => r.pageId), ['page-001', 'page-002']);
  assert.deepEqual(rows.map((r) => r.url), ['https://ex.test/a', 'https://ex.test/b']);
  assert.ok(existsSync(join(sessionDir, 'text', 'page-001.clean.part-001.md')), 'prior page text must survive append');
  assert.ok(existsSync(join(sessionDir, 'text', 'page-002.clean.part-001.md')));
});

test('default output climbs out of a .octocode tree instead of nesting sessions', () => {
  const drift = join(root, '.octocode', 'octocode-scraping', 'tmp', 'old-session');
  mkdirSync(drift, { recursive: true });
  const res = run(mockArgs('https://ex.test/a'), drift);
  assert.equal(res.status, 0, res.stderr);
  assert.ok(res.parsed.sessionDir.startsWith(join(realpathSync(root), '.octocode', 'octocode-scraping', 'tmp')), res.parsed.sessionDir);
  assert.ok(!res.parsed.sessionDir.includes('old-session'), `nested session: ${res.parsed.sessionDir}`);
});

test('workspaceRootFor anchors above the first .octocode segment', () => {
  assert.equal(workspaceRootFor('/a/b/.octocode/octocode-scraping/tmp/x'), '/a/b');
  assert.equal(workspaceRootFor('/a/b'), '/a/b');
  assert.equal(workspaceRootFor('/a/.octocode/x/.octocode/y'), '/a');
});

test('auto-selected html route is direct even when chrome-devtools is installed', () => {
  // Installing the live-browser skill must not make static HTML fetches launch Chrome.
  const res = run(['--url', 'https://ex.test/a', '--mock-status', '0', '--mock-body-file', join(root, 'body.html'), '--mock-content-type', 'text/html']);
  assert.ok(res.parsed, res.stderr);
  assert.equal(res.parsed.route, 'direct:html', JSON.stringify(res.parsed));
});

test('thin application shell recommends one Chrome escalation without launching it', () => {
  writeFileSync(join(root, 'body.html'), '<html><body><div id="app"></div><script type="module" src="/app.js"></script></body></html>');
  const res = run(['--url', 'https://ex.test/app', '--mock-status', '200', '--mock-body-file', join(root, 'body.html'), '--mock-content-type', 'text/html']);
  assert.equal(res.status, 0, res.stderr);
  assert.equal(res.parsed.route, 'direct:html');
  assert.equal(res.parsed.next.route, 'octocode-chrome-devtools');
  assert.match(res.parsed.next.reason, /application shell/);
});

test('crawl treats fragment and trailing-slash variants as one page and skips non-http links', () => {
  writeFileSync(join(root, 'body.html'), '<html><body><h1>Doc</h1><p>one</p><p>two</p><a href="#top">t</a><a href="/a/">a</a><a href="/b">b</a><a href="/b#s">bs</a><a href="mailto:x@ex.test">m</a></body></html>');
  const res = run(mockArgs('https://ex.test/a', ['--crawl', '--max-pages', '10', '--delay-ms', '0']));
  assert.equal(res.status, 0, res.stderr);
  const rows = readFileSync(join(res.parsed.sessionDir, 'sources.jsonl'), 'utf8').trim().split('\n').map((l) => JSON.parse(l));
  assert.deepEqual(rows.map((r) => r.url), ['https://ex.test/a', 'https://ex.test/b']);
  const text = readFileSync(join(res.parsed.sessionDir, 'text', 'page-001.clean.part-001.md'), 'utf8');
  assert.match(text, /^# Doc\n\none\n\ntwo/, 'HTML text keeps block structure as lines');
});

test('--append --crawl resumes from the saved link frontier and reports when nothing is new', () => {
  const links = ['a', 'b', 'c'].map((p) => `<a href="https://ex.test/${p}">${p}</a>`).join(' ');
  const linked = join(root, 'linked.html');
  writeFileSync(linked, `<html><head><title>Root</title></head><body><h1>Root</h1><p>${links}</p></body></html>`);
  const crawl = (extra) => run(['--url', 'https://ex.test/', '--provider', 'direct', '--mock-status', '200', '--mock-body-file', linked,
    '--mock-content-type', 'text/html', '--crawl', '--delay-ms', '0', '--session', 'crawl1', ...extra]);

  const first = crawl(['--max-pages', '1']);
  assert.equal(first.status, 0, first.stderr);
  assert.equal(first.parsed.pages, 1);

  const resumed = crawl(['--append', '--max-pages', '2']);
  assert.equal(resumed.status, 0, resumed.stderr || resumed.stdout);
  assert.equal(resumed.parsed.newPages, 2);
  assert.equal(resumed.parsed.pages, 3);

  const rest = crawl(['--append', '--max-pages', '5']);
  assert.equal(rest.status, 0, rest.stderr || rest.stdout);
  assert.equal(rest.parsed.newPages, 1);

  const exhausted = crawl(['--append', '--max-pages', '5']);
  assert.equal(exhausted.status, 1, 'an append that fetches nothing must not report success');
  assert.equal(exhausted.parsed.newPages, 0);
  assert.ok(exhausted.parsed.warnings.some((w) => /no new pages/.test(w.warning)));
});

test('an unknown or retired flag is rejected instead of silently ignored', () => {
  for (const flag of ['--extract-links', '--max-page', '--same-domain']) {
    const res = run(mockArgs('https://ex.test/a', [flag, ...(flag === '--max-page' ? ['3'] : [])]));
    assert.equal(res.status, 2, `${flag}: ${res.stdout}`);
    assert.match(res.parsed.error.message, new RegExp(`Unknown option: ${flag}`));
  }
});

test('a crawl stays on the --url host unless --span-hosts is passed', () => {
  writeFileSync(join(root, 'body.html'), '<html><body><a href="https://ex.test/b">b</a><a href="https://other.test/c">c</a></body></html>');
  const urls = (extra) => {
    const res = run(mockArgs('https://ex.test/a', ['--crawl', '--max-pages', '5', '--delay-ms', '0', ...extra]));
    assert.equal(res.status, 0, res.stderr || res.stdout);
    return readFileSync(join(res.parsed.sessionDir, 'sources.jsonl'), 'utf8').trim().split('\n').map((l) => JSON.parse(l).url);
  };
  assert.deepEqual(urls(['--session', 'host-only']), ['https://ex.test/a', 'https://ex.test/b']);
  assert.deepEqual(urls(['--session', 'span', '--span-hosts']), ['https://ex.test/a', 'https://ex.test/b', 'https://other.test/c']);
});


test('an interrupted crawl resumes committed pages without overwriting evidence', async () => {
  const fixture = join(root, 'body.html');
  writeFileSync(fixture, '<html><title>Original</title><body><p>Original evidence</p><a href="https://ex.test/next">Next</a></body></html>');
  const sessionDir = join(realpathSync(root), '.octocode/octocode-scraping/tmp/interrupted');
  const child = spawn(process.execPath, [script, ...mockArgs('https://ex.test/start', ['--session', 'interrupted', '--crawl', '--max-pages', '2', '--delay-ms', '5000'])], { cwd: root, stdio: 'ignore' });
  const closed = new Promise(resolve => child.once('close', resolve));
  try {
    const checkpoint = join(sessionDir, '.pending/page-001.json');
    const deadline = Date.now() + 5000;
    while (!existsSync(checkpoint) && Date.now() < deadline) await new Promise(resolve => setTimeout(resolve, 20));
    assert(existsSync(checkpoint), 'first page must be committed before interruption');
    child.kill('SIGKILL');
    await closed;
    const original = readFileSync(join(sessionDir, 'pages/page-001.json'), 'utf8');
    const refused = run(mockArgs('https://ex.test/start', ['--session', 'interrupted']));
    assert.equal(refused.parsed.error.code, 'sessionExists');
    writeFileSync(fixture, '<html><title>Next page</title><body><p>New evidence</p></body></html>');
    const resumed = run(mockArgs('https://ex.test/start', ['--session', 'interrupted', '--append', '--crawl', '--max-pages', '2', '--delay-ms', '0']));
    assert.equal(resumed.status, 0, resumed.stderr || resumed.stdout);
    assert.equal(resumed.parsed.pages, 2);
    assert.equal(readFileSync(join(sessionDir, 'pages/page-001.json'), 'utf8'), original);
    const rows = readFileSync(join(sessionDir, 'sources.jsonl'), 'utf8').trim().split('\n').map(JSON.parse);
    assert.deepEqual(rows.map(row => row.url), ['https://ex.test/start', 'https://ex.test/next']);
    assert(!existsSync(join(sessionDir, '.pending')));
  } finally { child.kill('SIGKILL'); await closed; }
});

test('uncommitted page files are preserved and never reused by append', () => {
  const sessionDir = join(root, '.octocode/octocode-scraping/tmp/incomplete');
  mkdirSync(join(sessionDir, 'raw'), { recursive: true });
  writeFileSync(join(sessionDir, 'raw/page-001.html'), 'partial original');
  const result = run(mockArgs('https://ex.test/start', ['--session', 'incomplete', '--append']));
  assert.equal(result.parsed.error.code, 'sessionIncomplete');
  assert.equal(readFileSync(join(sessionDir, 'raw/page-001.html'), 'utf8'), 'partial original');
});
