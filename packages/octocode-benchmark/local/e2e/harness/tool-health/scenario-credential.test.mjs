import assert from 'node:assert/strict';
import test from 'node:test';
import path from 'node:path';
import { FIXTURES } from '../paths.mjs';
import { startSurfaces } from './surfaces.mjs';

test('a fresh scenario credential can read smart HTTP only during its owned scope', async () => {
  const surfaces = await startSurfaces(path.join(FIXTURES, 'tool-health'));
  const { mock } = surfaces.ctx;
  const get = token => fetch(`${mock.url}/octo/demo.git/info/refs?service=git-upload-pack`, {
    headers: { authorization: `Basic ${Buffer.from(`x-access-token:${token}`).toString('base64')}` }, signal: AbortSignal.timeout(5000),
  });
  let token;
  try {
    assert.equal(mock.git, true, mock.gitUnavailable);
    await surfaces.scenario({ tool: 'ghCloneRepo', name: 'smart-http', surface: 'cli' }, async scope => {
      token = scope.token;
      const response = await get(token);
      assert.equal(response.status, 200);
      assert.match(await response.text(), /git-upload-pack/);
      assert.equal((await get('unregistered-credential')).status, 403);
    });
    assert.equal((await get(token)).status, 403);
    assert.deepEqual(mock.gitAuth.map(entry => entry.tokenMatches), [true, false, false]);
  } finally { await surfaces.close(); }
});
