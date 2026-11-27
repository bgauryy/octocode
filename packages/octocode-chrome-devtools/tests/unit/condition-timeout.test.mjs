import test from 'node:test';
import assert from 'node:assert/strict';
import { waitCondition } from '../../dist/engine/cdp-checks/plan-condition.mjs';

const page = value => ({
  send: async () => ({ result: { value } }),
});

test('a condition timeout names the page URL, title and readiness', async () => {
  await assert.rejects(
    waitCondition(
      page({
        ready: true,
        url: 'https://fixture/login',
        title: 'Sign in',
        matched: false,
      }),
      { selector: '#dashboard' },
      150,
      () => {}
    ),
    error => {
      assert.match(error.message, /Condition timed out/);
      assert.match(error.message, /#dashboard/);
      assert.match(error.message, /"https:\/\/fixture\/login"/);
      assert.match(error.message, /"Sign in"/);
      assert.match(error.message, /ready/);
      return true;
    }
  );
});

test('a condition that holds reports the page state without the title', async () => {
  const result = await waitCondition(
    page({
      ready: true,
      url: 'https://fixture/',
      title: 'Home',
      matched: true,
    }),
    { url: 'https://fixture/' },
    150,
    () => {}
  );
  assert.deepEqual(Object.keys(result).sort(), [
    'elapsedMs',
    'matched',
    'ready',
    'url',
  ]);
});

test('a condition timeout before any page read has no page state to name', async () => {
  await assert.rejects(
    waitCondition(
      { send: async () => undefined },
      { text: 'Done' },
      120,
      () => {}
    ),
    /^Error: Condition timed out: \{"text":"Done"\}$/
  );
});
