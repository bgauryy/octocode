import assert from 'node:assert/strict';
import { existsSync, mkdtempSync, realpathSync, rmSync } from 'node:fs';
import { createServer } from 'node:http';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import test from 'node:test';

const engine = resolve(import.meta.dirname, '../../dist/engine');
const { flagArgs } = await import(join(engine, 'cli-flags.mjs'));
const { cdpHttp, cdpRequest, octocodeOutputBase, octocodeWorkspace } =
  await import(join(engine, 'engine-io.mjs'));

test('flag readers treat a missing or empty value as the default', () => {
  const { getArg, hasFlag, getLowerArg } = flagArgs([
    '--page',
    '',
    '--kind',
    'XHR',
    '--dry-run',
    '--last',
  ]);
  assert.equal(getArg('--kind'), 'XHR');
  assert.equal(getArg('--page', '1'), '1', 'empty value');
  assert.equal(getArg('--last', 'x'), 'x', 'no value after the flag');
  assert.equal(getArg('--absent', 'd'), 'd');
  assert.equal(getArg('--absent'), undefined);
  assert.equal(getLowerArg('--kind'), 'xhr');
  assert.equal(getLowerArg('--absent'), '');
  assert(hasFlag('--dry-run'));
  assert(!hasFlag('--absent'));
});

test('the workspace folder is created once and an existing one is left alone', () => {
  const previous = process.cwd();
  const cwd = realpathSync(mkdtempSync(join(tmpdir(), 'chrome-engine-io-')));
  try {
    process.chdir(cwd);
    assert.equal(octocodeWorkspace(), join(cwd, '.octocode'));
    assert(!existsSync(join(cwd, '.octocode')), 'resolving does not create');
    assert.equal(octocodeOutputBase(), join(cwd, '.octocode'));
    assert(existsSync(join(cwd, '.octocode')));
    assert.equal(octocodeOutputBase(), join(cwd, '.octocode'));
  } finally {
    process.chdir(previous);
    rmSync(cwd, { recursive: true, force: true });
  }
});

test('DevTools HTTP requests reach the loopback address, parse JSON and fail with the status and path', async () => {
  const seen = [];
  const server = createServer((req, res) => {
    seen.push(`${req.method} ${req.url}`);
    if (req.url === '/json/version') return res.end('{"Browser":"Fake/1"}');
    if (req.url === '/json/close/t1') return res.end('Target is closing');
    res.writeHead(404).end();
  });
  await new Promise(done => server.listen(0, '127.0.0.1', done));
  const { port } = server.address();
  try {
    assert.deepEqual(await cdpHttp(port, '/json/version'), {
      Browser: 'Fake/1',
    });
    assert.equal(
      await (await cdpRequest(port, '/json/close/t1')).text(),
      'Target is closing',
      'a plain-text endpoint is reachable without JSON parsing'
    );
    await cdpHttp(port, '/json/version', { method: 'PUT' });
    assert.equal(seen.at(-1), 'PUT /json/version');
    await assert.rejects(
      cdpHttp(port, '/json/missing'),
      /CDP HTTP 404 for \/json\/missing/
    );
  } finally {
    await new Promise(done => server.close(done));
  }
});
