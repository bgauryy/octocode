import assert from 'node:assert/strict';
import test from 'node:test';
import path from 'node:path';
import { FIXTURES } from '../paths.mjs';
import { startMock } from './mock-server.mjs';

test('classification controls isolate credentials and model discovery performs no inference', async () => {
  const mock = await startMock({ fixtureDir: path.join(FIXTURES, 'tool-health') });
  try {
    mock.classificationResponders.set('fixture-refused', () => ({ status: 401, body: { error: 'refused' } }));
    const request = { model: 'jev-1.14.0', state: 'evidence', questions: { located: { type: 'box' } } };
    const send = async token => fetch(`${mock.url}/v1/systemone`, { method: 'POST', headers: { authorization: `Bearer ${token}`, 'content-type': 'application/json' }, body: JSON.stringify(request), signal: AbortSignal.timeout(5000) });
    const [refused, accepted] = await Promise.all([send('fixture-refused'), send('fixture-accepted')]);
    assert.equal(refused.status, 401);
    const result = await accepted.json();
    assert.deepEqual(result.answers.located, { type: 'box', box: { top: 0.1, bottom: 0.9, left: 0.2, right: 0.8 }, confidence: null });
    const models = await fetch(`${mock.url}/v1/models`, { signal: AbortSignal.timeout(5000) }).then(response => response.json());
    assert.ok(models.models.some(model => model.name === 'jev-1.14.0'));
    assert.equal(mock.classificationRequests.length, 2);
    assert.deepEqual(new Set(mock.requestsFrom.filter(entry => entry.request.startsWith('POST')).map(entry => entry.token)), new Set(['fixture-refused', 'fixture-accepted']));
    assert.deepEqual(mock.unhandled, []);
  } finally { await mock.close(); }
});
