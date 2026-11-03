import assert from 'node:assert/strict';
import test from 'node:test';
import path from 'node:path';
import { FIXTURES } from '../paths.mjs';
import { startMock } from './mock-server.mjs';

const headersOf = response => Object.fromEntries(['resource', 'limit', 'remaining', 'reset'].map(name => [name, response.headers.get(`x-ratelimit-${name}`)]));

test('the mock reports the rate-limit resource each route draws from, as GitHub does', async () => {
  const mock = await startMock({ fixtureDir: path.join(FIXTURES, 'tool-health') });
  try {
    const get = (route, token = 'fixture-a') => fetch(`${mock.url}${route}`, { headers: { authorization: `Bearer ${token}` }, signal: AbortSignal.timeout(5000) });
    const code = headersOf(await get('/search/code?q=demo'));
    assert.deepEqual([code.resource, code.limit, code.remaining], ['code_search', '10', '9']);
    const repositories = headersOf(await get('/search/repositories?q=demo'));
    assert.deepEqual([repositories.resource, repositories.limit, repositories.remaining], ['search', '30', '29']);
    const core = headersOf(await get('/repos/octo/demo'));
    assert.deepEqual([core.resource, core.limit], ['core', '5000']);
    assert.ok(Number(code.reset) > Date.now() / 1000 && Number(code.reset) <= Date.now() / 1000 + 60, 'a search window is a minute');
    // The quota falls per credential and resource, within one window.
    assert.equal(headersOf(await get('/search/code?q=demo')).remaining, '8');
    assert.equal(headersOf(await get('/search/code?q=demo', 'fixture-b')).remaining, '9');
    assert.equal(headersOf(await get('/search/issues?q=demo')).remaining, '28');
  } finally { await mock.close(); }
});
