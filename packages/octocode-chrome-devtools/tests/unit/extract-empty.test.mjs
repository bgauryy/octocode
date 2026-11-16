import test from 'node:test';
import assert from 'node:assert/strict';
import { run } from '../../dist/engine/cdp-checks/browser-execute.mjs';
import { call, fixture } from './plan-fixture.mjs';

const page = rows => ({
  result: { value: { rows, url: 'https://example.test/', title: 't' } },
});
const withRows = (cdp, rows) => {
  cdp.send = async method => (method === 'Runtime.evaluate' ? page(rows) : {});
};

test('an extract matching zero elements returns an empty result with a note and the plan continues', async () =>
  fixture(async ({ cdp, read }) => {
    withRows(cdp, []);
    await run(cdp, {
      steps: [{ op: 'extract', selector: '.missing' }, call('Runtime.enable')],
    });
    const result = read('browser-result.json');
    assert.equal(result.ok, true, JSON.stringify(result.failure));
    assert.equal(result.steps.length, 2);
    assert.equal(result.steps[0].status, 'complete');
    assert.equal(result.steps[0].count, 0);
    assert.match(result.steps[0].note, /\.missing/);
    assert.deepEqual(read('extract-1.json'), []);
  }));

test('a non-empty extract carries no note', async () =>
  fixture(async ({ cdp, read }) => {
    withRows(cdp, [{ text: 'a', href: null }]);
    await run(cdp, { steps: [{ op: 'extract', selector: 'a' }] });
    const [step] = read('browser-result.json').steps;
    assert.equal(step.count, 1);
    assert.equal(step.note, undefined);
  }));

test('allowEmpty: false still fails the plan on zero matches', async () =>
  fixture(async ({ cdp, read }) => {
    withRows(cdp, []);
    await run(cdp, {
      steps: [
        { op: 'extract', selector: '.missing', allowEmpty: false },
        call('Runtime.enable'),
      ],
    });
    const result = read('browser-result.json');
    assert.equal(result.ok, false);
    assert.match(result.failure.error, /zero elements/);
    assert.equal(result.steps.length, 1);
  }));
