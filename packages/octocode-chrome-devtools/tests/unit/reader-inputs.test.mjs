import test from 'node:test';
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

const engine = join(import.meta.dirname, '../../dist/engine');
const cwd = mkdtempSync(join(tmpdir(), 'octo-cdp-readers-'));
test.after(() => rmSync(cwd, { recursive: true, force: true }));
const run = (script, args) =>
  spawnSync(process.execPath, [join(engine, script), ...args], {
    cwd,
    encoding: 'utf8',
    timeout: 15000,
  });

const entries = [
  { request: { url: 'https://a.test/login' }, response: { status: 0 } },
  { request: { url: 'https://a.test/app' }, response: { status: 200 } },
  { request: { url: 'https://b.test/login' }, response: { status: 500 } },
];
const rowsFile = join(cwd, 'rows.json');
writeFileSync(rowsFile, JSON.stringify({ log: { entries } }));
const query = extra =>
  run('evidence-query.mjs', [
    '--file',
    rowsFile,
    '--pointer',
    '/log/entries',
    ...extra,
  ]);
const values = result => {
  assert.equal(result.status, 0, result.stderr);
  return JSON.parse(result.stdout);
};

test('query where accepts simple conditions, dotted paths and JSON alike', () => {
  const json = values(
    query(['--where', '[{"path":"/response/status","op":"eq","value":0}]'])
  );
  assert.equal(json.matched, 1);
  for (const [where, matched] of [
    ['response.status=0', 1],
    ['response.status!=0', 2],
    ['response.status>=200', 2],
    ['response.status<=200', 2],
    ['request.url~login', 2],
    ['request.url~login && response.status=500', 1],
    ['response.status', 3],
    ['!response.nothing', 3],
    ['/response/status=0', 1],
  ])
    assert.equal(values(query(['--where', where])).matched, matched, where);
});

test('query select takes dotted and comma-separated paths, keyed by pointer', () => {
  const page = values(query(['--select', 'request.url,response.status']));
  assert.deepEqual(Object.keys(page.rows[0].value), [
    '/request/url',
    '/response/status',
  ]);
  const array = values(query(['--select', '["request.url"]']));
  assert.equal(array.rows[0].value['/request/url'], 'https://a.test/login');
});

test('query accepts a dotted --pointer and a parse error shows a valid example', () => {
  const dotted = values(
    run('evidence-query.mjs', ['--file', rowsFile, '--pointer', 'log.entries'])
  );
  assert.equal(dotted.matched, 3);
  const bad = query(['--where', '[{broken']);
  assert.notEqual(bad.status, 0);
  assert.match(bad.stderr, /Invalid --where/);
  assert.match(bad.stderr, /Example: response\.status=0/);
  const noPath = query(['--where', 'a b c']);
  assert.notEqual(noPath.status, 0);
  assert.match(noPath.stderr, /Example:/);
});

test('every helper prints its help on stdout, and schema returns the helper options', () => {
  for (const script of [
    'prune-artifacts.mjs',
    'cdp-runner.mjs',
    'cdp-checks/har-pager.mjs',
    'cdp-checks/measure-query.mjs',
    'cookie-bridge.mjs',
    'cdp-checks/snapshot-query.mjs',
  ]) {
    const result = run(script, ['--help']);
    assert.equal(result.status, 0, script);
    assert.match(result.stdout, /Usage/, script);
  }
  const schema = JSON.parse(
    run('cli.mjs', ['schema', 'snapshot-query']).stdout
  );
  assert.match(schema.usage, /--text/);
  assert.match(schema.usage, /--view/);
  const prune = JSON.parse(run('cli.mjs', ['schema', 'prune']).stdout);
  assert.match(prune.usage, /--max-age-days/);
});

test('har_pager accepts --file as well as the positional path', () => {
  const har = join(cwd, 'input.har');
  writeFileSync(
    har,
    JSON.stringify({
      log: {
        entries: [
          {
            request: { method: 'GET', url: 'https://a.test/' },
            response: { status: 200, content: {}, headers: [] },
            time: 1,
            startedDateTime: '2026-01-01T00:00:00Z',
          },
        ],
      },
    })
  );
  const positional = run('cdp-checks/har-pager.mjs', [har]);
  const flagged = run('cdp-checks/har-pager.mjs', ['--file', har]);
  assert.equal(flagged.status, 0, flagged.stderr);
  assert.equal(flagged.stdout, positional.stdout);
  const both = run('cdp-checks/har-pager.mjs', [har, '--file', har]);
  assert.notEqual(both.status, 0);
});

test('snapshot-query filters refs by text or regex and keeps the outline line', () => {
  const file = join(cwd, 'page-snapshot.json');
  writeFileSync(
    file,
    JSON.stringify({
      url: 'https://a.test/',
      title: 'T',
      refs: {
        e1: { role: 'button', name: 'Sign in' },
        e2: { role: 'link', name: 'Pricing', frame: 'https://pay.test/' },
        e3: { role: 'button', name: 'Sign out' },
      },
      outline: [{ line: 'r1 main — 3 refs e1-e3' }],
    })
  );
  const read = args => {
    const result = run('cdp-checks/snapshot-query.mjs', [
      '--file',
      file,
      ...args,
    ]);
    assert.equal(result.status, 0, result.stderr);
    return JSON.parse(result.stdout);
  };
  const text = read(['--text', 'SIGN']);
  assert.deepEqual(
    text.rows.map(row => row.ref),
    ['e1', 'e3']
  );
  assert.equal(text.rows[0].line, '[e1] button "Sign in"');
  assert.deepEqual(
    read(['--regex', 'pay\\.test']).rows.map(row => row.ref),
    ['e2']
  );
  assert.equal(read(['--text', 'nothing']).totalRows, 0);
  assert.equal(read(['--view', 'outline', '--text', 'main']).totalRows, 1);
  assert.notEqual(
    run('cdp-checks/snapshot-query.mjs', ['--file', file, '--regex', '('])
      .status,
    0
  );
  assert.equal(readFileSync(file, 'utf8').includes('Sign in'), true);
});
