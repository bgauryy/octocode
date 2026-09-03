import assert from 'node:assert/strict';
import { spawn, spawnSync } from 'node:child_process';
import { createServer } from 'node:http';
import { test } from 'vitest';
import { allowLocalFixtureProcesses } from '../../../test-utils/external-effects-guard.js';

test('default tests cannot inherit live-integration flags', () => {
  assert.equal(process.env['OCTOCODE_CHROME_DEBUG_E2E'], undefined);
  assert.equal(process.env['RUN_CHROME_LIVE'], undefined);
  assert.equal(process.env['RUN_MCP_LIVE'], undefined);
});

test('default tests block browser and system-opener processes', () => {
  assert.throws(
    () => spawn('/Applications/Google Chrome.app/Contents/MacOS/Google Chrome', ['https://example.com']),
    /TEST_EXTERNAL_EFFECT_BLOCKED.*inject a process\/browser mock/i,
  );
  assert.throws(
    () => spawnSync('open', ['https://example.com']),
    /TEST_EXTERNAL_EFFECT_BLOCKED.*inject a process\/browser mock/i,
  );
  assert.throws(
    () => spawnSync('npx', ['some-remote-package']),
    /TEST_EXTERNAL_EFFECT_BLOCKED.*inject a process\/browser mock/i,
  );
});

test('default tests block outbound network and allow deterministic Node subprocesses', async () => {
  await assert.rejects(
    () => fetch('https://example.com/'),
    /TEST_EXTERNAL_EFFECT_BLOCKED.*inject a fetch mock/i,
  );

  const child = spawnSync(process.execPath, ['--eval', 'process.stdout.write("ok")'], {
    encoding: 'utf8',
  });
  assert.equal(child.status, 0);
  assert.equal(child.stdout, 'ok');
});

test('loopback HTTP fixtures remain available without opening outbound network', async () => {
  const server = createServer((_request, response) => response.end('fixture-ok'));
  await new Promise<void>((resolve) => server.listen(0, '127.0.0.1', resolve));
  try {
    const address = server.address();
    assert.ok(address && typeof address !== 'string');
    const response = await fetch(`http://127.0.0.1:${address.port}/health`);
    assert.equal(await response.text(), 'fixture-ok');
  } finally {
    await new Promise<void>((resolve, reject) => server.close((error) => error ? reject(error) : resolve()));
  }
});

test('local fixture process permission is explicit and revocable', () => {
  assert.throws(() => spawnSync('git', ['--version']), /TEST_EXTERNAL_EFFECT_BLOCKED/);
  const restore = allowLocalFixtureProcesses();
  try {
    const child = spawnSync('git', ['--version'], { encoding: 'utf8' });
    assert.equal(child.status, 0);
    assert.match(child.stdout, /^git version /);
  } finally {
    restore();
  }
  assert.throws(() => spawnSync('git', ['--version']), /TEST_EXTERNAL_EFFECT_BLOCKED/);
});
