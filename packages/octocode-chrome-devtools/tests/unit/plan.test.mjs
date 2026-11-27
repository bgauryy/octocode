import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync, existsSync } from 'node:fs';
import { join } from 'node:path';
import {
  validatePlan,
  run,
} from '../../dist/engine/cdp-checks/browser-execute.mjs';
import { call, fixture } from './plan-fixture.mjs';

test('step journal is present before commands and retains failure without replaying later steps', async () =>
  fixture(async ({ cdp, dir, read }) => {
    const journal = () =>
      readFileSync(join(dir, 'browser-steps.jsonl'), 'utf8')
        .trim()
        .split('\n')
        .map(JSON.parse);
    let calls = 0;
    cdp.send = async () => {
      calls += 1;
      assert.equal(journal().at(-1).status, 'running');
      assert.equal(journal().at(-1).index, calls);
      if (calls === 2) throw new Error('CDP timeout after possible mutation');
      return { acknowledged: true };
    };
    await run(cdp, {
      steps: [
        call('Runtime.enable'),
        call('Runtime.evaluate'),
        call('Runtime.disable'),
      ],
    });
    assert.equal(calls, 2);
    assert.deepEqual(
      journal().map(row => [row.index, row.status]),
      [
        [1, 'running'],
        [1, 'complete'],
        [2, 'running'],
        [2, 'failed'],
      ]
    );
    const result = read('browser-result.json');
    assert.equal(result.failure.outcomeUncertain, true);
    assert.equal(result.journalArtifact, join(dir, 'browser-steps.jsonl'));
  }));

test('whole-plan validation rejects malformed optional fields before mutations', () => {
  const badSteps = [
    { op: 'act', action: 'fill', selector: 17, value: 'x' },
    { op: 'act', action: 'fill', selector: '#q', value: {} },
    { op: 'extract', selector: 'a', fields: null },
    { op: 'protocol', domain: 'Runtime', member: 12 },
    { op: 'cdp', method: 'Runtime.evaluate', after: null },
    { op: 'cdp', method: 'Runtime.evaluate', frame: null },
    { op: 'listen', id: 'log', event: 'Runtime.consoleAPICalled', where: null },
  ];
  for (const step of badSteps)
    assert.throws(
      () =>
        validatePlan({
          steps: [
            call('Runtime.evaluate', { expression: 'window.mutated=true' }),
            step,
          ],
        }),
      JSON.stringify(step)
    );
  for (const observe of [null, false, [], 1])
    assert.throws(() =>
      validatePlan({ steps: [call('Runtime.enable')], observe })
    );
});
test('failed Runtime calls retain complete exception evidence', async () =>
  fixture(async ({ cdp, read, dir }) => {
    const payload = {
      result: { type: 'undefined' },
      exceptionDetails: {
        text: 'Uncaught',
        exception: { description: 'Error: evidence', objectId: 'object-1' },
        stackTrace: {
          callFrames: [
            {
              functionName: 'test',
              url: 'fixture',
              lineNumber: 4,
              columnNumber: 2,
            },
          ],
        },
      },
    };
    cdp.send = async () => payload;
    await run(cdp, {
      steps: [
        call('Runtime.evaluate', { expression: 'throw Error("evidence")' }),
      ],
    });
    assert(existsSync(join(dir, 'cdp-1.json')));
    assert.deepEqual(read('cdp-1.json'), payload);
    assert.equal(read('browser-result.json').ok, false);
  }));
test('result pointers cannot resolve inherited properties', async () =>
  fixture(async ({ cdp, read }) => {
    await run(cdp, {
      steps: [
        call('Runtime.evaluate'),
        call('Runtime.callFunctionOn', {
          objectId: { $step: 1, pointer: '/constructor' },
        }),
      ],
    });
    assert.equal(read('browser-result.json').ok, false);
    assert.match(read('browser-result.json').failure.error, /unavailable/);
  }));
test('protocol failures retain wire details and the complete input plan', async () =>
  fixture(async ({ cdp, read }) => {
    const payload = {
      code: -32602,
      message: 'Invalid parameters',
      data: 'required field missing',
    };
    const plan = {
      steps: [
        call('Runtime.evaluate', { expression: '42' }),
        call('Runtime.enable'),
      ],
    };
    let calls = 0;
    cdp.send = async () => {
      calls++;
      throw Object.assign(Error(payload.message), { protocolError: payload });
    };
    await run(cdp, plan);
    assert.equal(calls, 1);
    assert.deepEqual(read('protocol-error-1.json'), payload);
    assert.deepEqual(read('browser-plan.json'), plan);
    assert.equal(read('browser-result.json').ok, false);
  }));
test('failed stream reads still close handles and disclose partial capture', async () =>
  fixture(async ({ cdp, read, sent }) => {
    cdp.send = async (method, params) => {
      sent.push({ method, params });
      if (method === 'IO.read') throw Error('read failed');
      return {};
    };
    await run(cdp, { steps: [{ op: 'readStream', handle: 'stream' }] });
    assert(sent.some(row => row.method === 'IO.close'));
    assert.equal(read('browser-result.json').steps[0].complete, false);
    assert.equal(read('browser-result.json').ok, false);
  }));
test('selected protocol members retain the full installed schema for external types', async () =>
  fixture(async ({ cdp, read, dir }) => {
    await run(cdp, {
      steps: [{ op: 'protocol', domain: 'Runtime', member: 'evaluate' }],
    });
    assert(existsSync(join(dir, 'installed-protocol.json')));
    assert.equal(read('installed-protocol.json').domains[1].domain, 'Debugger');
    assert.equal(read('protocol-1.json').commands[0].name, 'evaluate');
  }));
