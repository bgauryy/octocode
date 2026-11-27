// The snapshot-query and measure-query helpers read saved evidence through the `helper` MCP tool.
import test from 'node:test';
import assert from 'node:assert/strict';
import { withHelpers } from './helper-fixture.mjs';

const snapshot = {
  refs: {
    e1: { role: 'button', name: 'Save' },
    e2: { role: 'link', name: 'Home' },
    e3: { role: 'textbox', name: 'Email' },
  },
  outline: ['[e1] button "Save"', '[e2] link "Home"', '[e3] textbox "Email"'],
};

test('snapshot-query view refs returns the known refs, and filters by text', () =>
  withHelpers(async ({ call, put }) => {
    put('page-snapshot.json', snapshot);
    const { data } = await call('snapshot-query', [
      '--file',
      'page-snapshot.json',
      '--view',
      'refs',
    ]);
    assert.equal(data.totalRows, 3);
    assert.deepEqual(
      data.rows.map(row => [row.ref, row.role, row.name]),
      [
        ['e1', 'button', 'Save'],
        ['e2', 'link', 'Home'],
        ['e3', 'textbox', 'Email'],
      ]
    );
    const filtered = (
      await call('snapshot-query', [
        '--file',
        'page-snapshot.json',
        '--text',
        'MAIL',
      ])
    ).data;
    assert.deepEqual(
      filtered.rows.map(row => row.ref),
      ['e3']
    );
    const outline = (
      await call('snapshot-query', [
        '--file',
        'page-snapshot.json',
        '--view',
        'outline',
        '--limit',
        '2',
      ])
    ).data;
    assert.equal(outline.rows.length, 2);
    assert.equal(outline.totalRows, 3);
  }));

const finding = (code, message) => ({ code, message });
const measures = {
  'perf.json': {
    score: { health: 90, findings: [finding('SLOW_FCP', 'fcp 4s')] },
  },
  'net.json': {
    health: 60,
    findings: [
      finding('HTTP_FAILURES', '1 request failed'),
      finding('SLOW_API', 'one slow api'),
    ],
    failures: [{ url: 'https://a.test/broken', status: 500, kind: 'api' }],
    slow: [],
    sample: [],
    counts: { failed: 1, slow: 0, requests: 2 },
  },
  'storage.json': {
    score: {
      health: 100,
      findings: [finding('SENSITIVE_COOKIE_NOT_HTTPONLY', 'sid')],
    },
  },
};
const files = [
  '--perf',
  'perf.json',
  '--net',
  'net.json',
  '--storage',
  'storage.json',
];

test('measure-query --code keeps only that finding across the measures', () =>
  withHelpers(async ({ call, put }) => {
    for (const [name, value] of Object.entries(measures)) put(name, value);
    const all = (await call('measure-query', [...files, '--view', 'findings']))
      .data;
    assert.deepEqual(all.findings.map(row => row.code).sort(), [
      'HTTP_FAILURES',
      'SENSITIVE_COOKIE_NOT_HTTPONLY',
      'SLOW_API',
      'SLOW_FCP',
    ]);
    const one = (
      await call('measure-query', [
        ...files,
        '--view',
        'findings',
        '--code',
        'HTTP_FAILURES',
      ])
    ).data;
    assert.equal(one.findings.length, 1);
    assert.equal(one.findings[0].code, 'HTTP_FAILURES');
    assert.equal(one.findings[0].source, 'network');
    const failures = (
      await call('measure-query', [...files, '--view', 'failures'])
    ).data;
    assert.equal(JSON.stringify(failures).includes('a.test/broken'), true);
  }));

test('measure-query refuses a missing artifact with its path', () =>
  withHelpers(async ({ call }) => {
    const run = await call('measure-query', ['--net', 'absent.json']);
    assert.ok(run.error);
    assert.match(run.error, /absent\.json/);
  }));
