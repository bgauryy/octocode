// The har-pager and har-redact helpers run for real through the `helper` MCP tool.
import test from 'node:test';
import assert from 'node:assert/strict';
import { TMP, harEntry, har, withHelpers } from './helper-fixture.mjs';

const SECRETS = [
  'SECRETBEARER',
  'SECRETCOOKIE',
  'SECRETSETCOOKIE',
  'SECRETQUERY',
  'hunter2',
];
const source = har(
  harEntry({
    url: 'https://a.test/ok?token=SECRETQUERY&page=1',
    status: 200,
    headers: [
      { name: 'Authorization', value: 'Bearer SECRETBEARER' },
      { name: 'Cookie', value: 'sid=SECRETCOOKIE' },
    ],
    post: '{"password":"hunter2"}',
    body: '{"password":"hunter2"}',
  }),
  harEntry({ url: 'https://a.test/broken', status: 500, body: 'oops' })
);

test('har-pager filter failures returns only the failing entry', () =>
  withHelpers(async ({ call, put }) => {
    put('in.har', source);
    const { data } = await call('har-pager', [
      'in.har',
      '--filter',
      'failures',
      '--format',
      'json',
    ]);
    assert.equal(data.allEntries, 2);
    assert.equal(data.totalRows, 1);
    assert.equal(data.pageRows.length, 1);
    assert.equal(data.pageRows[0].status, 500);
    assert.match(data.pageRows[0].url, /\/broken$/);
    const all = (await call('har-pager', ['in.har', '--format', 'json'])).data;
    assert.equal(all.totalRows, 2);
  }));

test('har-pager pages rows without dropping any and refuses malformed paging', () =>
  withHelpers(async ({ call, put }) => {
    put('in.har', source);
    const first = (
      await call('har-pager', [
        'in.har',
        '--page-size',
        '1',
        '--format',
        'json',
      ])
    ).data;
    const second = (
      await call('har-pager', [
        'in.har',
        '--page-size',
        '1',
        '--page',
        '2',
        '--format',
        'json',
      ])
    ).data;
    assert.equal(first.totalPages, 2);
    assert.deepEqual(
      [...first.pageRows, ...second.pageRows].map(row => row.status),
      [200, 500]
    );
    for (const bad of [
      ['--page', '0'],
      ['--page', 'abc'],
      ['--page-size', '0'],
      ['--page-size', 'x'],
      ['--filter', 'bogus'],
    ]) {
      const run = await call('har-pager', ['in.har', ...bad]);
      assert.ok(run.error, `${bad.join(' ')} is refused`);
      assert.match(run.error, new RegExp(bad[0]));
    }
  }));

test('har-redact writes a redacted copy and leaves the source untouched', () =>
  withHelpers(async ({ call, put, read, exists }) => {
    put('in.har', source);
    const before = read('in.har');
    const out = `${TMP}/redacted/out.har`;
    const run = await call('har-redact', ['in.har', '--out', out]);
    assert.equal(run.result.isError, undefined, run.text);
    assert.ok(exists(out));
    const text = read(out);
    for (const secret of SECRETS)
      assert.ok(!text.includes(secret), `${secret} survived redaction`);
    const copy = JSON.parse(text);
    const [first] = copy.log.entries;
    assert.deepEqual(
      first.request.headers.map(header => header.value),
      ['[REDACTED]', '[REDACTED]']
    );
    assert.match(first.request.url, /token=%5BREDACTED%5D|token=\[REDACTED\]/);
    assert.equal(copy.log.entries[1].request.url, 'https://a.test/broken');
    assert.equal(read('in.har'), before, 'the source file is unchanged');
  }));

test('har-redact refuses an output outside the workspace output folder', () =>
  withHelpers(async ({ call, put, exists }) => {
    put('in.har', source);
    const run = await call('har-redact', ['in.har', '--out', '../escaped.har']);
    assert.ok(run.error, 'an outside --out is refused');
    assert.ok(!exists('../escaped.har'));
  }));
