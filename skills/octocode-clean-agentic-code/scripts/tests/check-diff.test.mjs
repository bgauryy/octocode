import { test } from 'node:test';
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { compare, parseJson, parseJunit, parseList, parseTap } from '../check-diff.mjs';

const script = fileURLToPath(new URL('../check-diff.mjs', import.meta.url));
const fixtures = fileURLToPath(new URL('fixtures/check-diff/', import.meta.url));

function run(args, cwd = fixtures) {
  const result = spawnSync(process.execPath, [script, ...args], { cwd, encoding: 'utf8', env: { PATH: process.env.PATH } });
  return { status: result.status, json: JSON.parse(result.stdout) };
}

function withTemp(files, body) {
  const dir = mkdtempSync(join(tmpdir(), 'check-diff-'));
  try {
    for (const [name, text] of Object.entries(files)) writeFileSync(join(dir, name), text);
    return body(dir);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
}

const side = rows => ({ checks: new Map(rows.map(({ name, status }) => [name, status])), listOnly: false });

test('splits JUnit runs into introduced, pre-existing, fixed and missing checks and exits 1 on an introduced failure', () => {
  const { status, json } = run(['before', 'after']);
  assert.equal(status, 1);
  assert.equal(json.ok, false);
  const api = name => `pytest > tests.test_api > ${name}`;
  assert.deepEqual(json.introduced, [{ name: api('test_list'), before: 'pass' }, { name: 'pytest > tests.test_db > test_rollback', before: 'absent' }]);
  assert.deepEqual(json.preExisting, [api('test_flaky_network')]);
  assert.deepEqual(json.fixed, ['pytest > tests.test_db > test_migrate']);
  assert.deepEqual(json.missing, [
    { name: api('test_create'), before: 'pass', after: 'skip' },
    { name: api('test_delete'), before: 'pass', after: 'absent' },
  ]);
  assert.deepEqual(json.counts, { introduced: 2, preExisting: 1, fixed: 1, missing: 2 });
  assert.equal(json.before.checks, 5);
});

test('exits 0 when the after run introduces no failure', () => {
  const { status, json } = run(['before', 'before']);
  assert.equal(status, 0);
  assert.equal(json.ok, true);
  assert.equal(json.introduced.length, 0);
  assert.equal(json.preExisting.length, json.before.failing);
});

test('CDATA and entities inside a failure do not create checks', () => {
  const rows = parseJunit(readFileSync(join(fixtures, 'before/junit.xml'), 'utf8'));
  assert.equal(rows.length, 5);
  assert.ok(rows.every(row => !row.name.includes('not a real case')));
});

test('a comment or CDATA section ends where XML ends it, whichever opens first', () => {
  const commentFirst = '<testsuite name="s"><!-- quotes <![CDATA[ in a comment --><testcase name="kept"/>'
    + '<testcase name="out"><system-out><![CDATA[ done ]]></system-out></testcase></testsuite>';
  assert.deepEqual(parseJunit(commentFirst), [{ name: 's > kept', status: 'pass' }, { name: 's > out', status: 'pass' }]);
  const cdataFirst = '<testsuite name="s"><testcase name="a"><system-out><![CDATA[ <!-- not a comment ]]></system-out></testcase>'
    + '<testcase name="b"><failure/></testcase><!-- end --></testsuite>';
  assert.deepEqual(parseJunit(cdataFirst), [{ name: 's > a', status: 'pass' }, { name: 's > b', status: 'fail' }]);
});

test('TAP keeps subtest paths, drops parents their children explain and keeps a parent-only failure', () => {
  const rows = parseTap(readFileSync(join(fixtures, 'node-test.tap'), 'utf8'));
  assert.deepEqual(rows, [
    { name: 'parser > reads a header', status: 'pass' },
    { name: 'parser > rejects # comments', status: 'fail' },
    { name: 'server > starts', status: 'pass' },
    { name: 'server', status: 'fail' },
    { name: 'skipped case', status: 'skip' },
    { name: 'known bug', status: 'skip' },
  ]);
});

test('reads Jest, Mocha, Playwright, go test, cargo and generic JSON reports with files relative to the root', () => {
  const root = '/repo';
  const jest = { testResults: [
    { name: '/repo/src/a.test.js', status: 'failed', assertionResults: [{ fullName: 'adds', status: 'passed' }, { ancestorTitles: ['math'], title: 'divides', status: 'failed' }] },
    { name: '/repo/src/b.test.js', status: 'failed', message: 'SyntaxError', assertionResults: [] },
  ] };
  assert.deepEqual(parseJson(JSON.stringify(jest), root, 'jest'), [
    { name: 'src/a.test.js > adds', status: 'pass' },
    { name: 'src/a.test.js > math > divides', status: 'fail' },
    { name: 'src/b.test.js', status: 'fail' },
  ]);
  const mocha = {
    stats: {},
    tests: [{ fullTitle: 'suite x', file: '/repo/t.js', err: {} }, { fullTitle: 'suite y', file: '/repo/t.js', err: { message: 'boom' } }, { fullTitle: 'suite z', file: '/repo/t.js', err: {} }],
    pending: [{ fullTitle: 'suite z', file: '/repo/t.js' }],
    failures: [{ fullTitle: 'suite y', file: '/repo/t.js' }, { fullTitle: '"before all" hook in "suite"', file: '/repo/t.js' }],
  };
  assert.deepEqual(parseJson(JSON.stringify(mocha), root, 'mocha'), [
    { name: 't.js > suite x', status: 'pass' },
    { name: 't.js > suite y', status: 'fail' },
    { name: 't.js > suite z', status: 'skip' },
    { name: 't.js > "before all" hook in "suite"', status: 'fail' },
  ]);
  const playwright = { config: {}, suites: [{ title: 'login.spec.ts', file: 'login.spec.ts', specs: [{ title: 'logs in', tests: [{ projectName: 'chromium', status: 'expected', expectedStatus: 'passed' }, { projectName: 'webkit', status: 'unexpected' }] }], suites: [{ title: 'logout', specs: [{ title: 'logs out', tests: [{ projectName: 'chromium', status: 'expected', expectedStatus: 'skipped' }] }] }] }] };
  assert.deepEqual(parseJson(JSON.stringify(playwright), root, 'playwright'), [
    { name: '[chromium] > login.spec.ts > logs in', status: 'pass' },
    { name: '[webkit] > login.spec.ts > logs in', status: 'fail' },
    { name: '[chromium] > login.spec.ts > logout > logs out', status: 'skip' },
  ]);
  const go = [
    { Action: 'run', Package: 'p', Test: 'TestA' },
    { Action: 'fail', Package: 'p', Test: 'TestA/sub' },
    { Action: 'fail', Package: 'p', Test: 'TestA' },
    { Action: 'pass', Package: 'p', Test: 'TestB' },
    { Action: 'fail', Package: 'p' },
    { Action: 'fail', Package: 'q' },
  ].map(event => JSON.stringify(event)).join('\nnot json output from a test\n');
  assert.deepEqual(parseJson(go, root, 'go'), [
    { name: 'p > TestA/sub', status: 'fail' },
    { name: 'p > TestB', status: 'pass' },
    { name: 'q', status: 'fail' },
  ]);
  const cargo = ['{"type":"suite","event":"started"}', '{"type":"test","event":"ok","name":"a::b"}', '{"type":"test","event":"failed","name":"a::c"}', '{"type":"test","event":"ignored","name":"a::d"}'].join('\n');
  assert.deepEqual(parseJson(cargo, root, 'cargo'), [{ name: 'a::b', status: 'pass' }, { name: 'a::c', status: 'fail' }, { name: 'a::d', status: 'skip' }]);
  assert.deepEqual(parseJson('[{"name":"lint","status":"success"},{"name":"build","conclusion":"failure"}]', root, 'checks'), [{ name: 'lint', status: 'pass' }, { name: 'build', status: 'fail' }]);
  assert.deepEqual(parseJson('{"tests":[{"nodeid":"t.py::test_a","outcome":"passed"}]}', root, 'pytest-json'), [{ name: 't.py::test_a', status: 'pass' }]);
  assert.deepEqual(parseJson('{"lint":"pass","types":"failed"}', root, 'map'), [{ name: 'lint', status: 'pass' }, { name: 'types', status: 'fail' }]);
});

test('an unknown status is invalid input, never guessed', () => {
  assert.throws(() => parseJson('[{"name":"x","status":"weird"}]', '/', 'r'), /unknown status "weird"/);
  withTemp({ 'a.json': '[{"name":"x","status":"weird"}]' }, dir => {
    const { status, json } = run(['a.json', 'a.json'], dir);
    assert.equal(status, 2);
    assert.equal(json.error.code, 'unknown-status');
  });
});

test('failure lists name every failure: an omitted name is not failing, and lists report no missing checks', () => {
  assert.deepEqual(parseList('a\n# note\n\nb\n'), [{ name: 'a', status: 'fail' }, { name: 'b', status: 'fail' }]);
  withTemp({ 'before.txt': 'a\nb\n', 'after.txt': 'b\nc\n' }, dir => {
    const { status, json } = run(['before.txt', 'after.txt'], dir);
    assert.equal(status, 1);
    assert.deepEqual(json.introduced, [{ name: 'c', before: 'notFailing' }]);
    assert.deepEqual(json.preExisting, ['b']);
    assert.deepEqual(json.fixed, ['a']);
    assert.deepEqual(json.missing, []);
  });
});

test('a failure that is skipped after the change is missing, not fixed', () => {
  const result = compare(side([{ name: 't', status: 'fail' }]), side([{ name: 't', status: 'skip' }]));
  assert.deepEqual(result.fixed, []);
  assert.deepEqual(result.missing, [{ name: 't', before: 'fail', after: 'skip' }]);
});

test('a structured report with no checks is invalid input', () => {
  withTemp({ 'empty.xml': '<testsuites></testsuites>' }, dir => {
    const { status, json } = run(['empty.xml', 'empty.xml'], dir);
    assert.equal(status, 2);
    assert.equal(json.error.code, 'no-checks');
  });
});
