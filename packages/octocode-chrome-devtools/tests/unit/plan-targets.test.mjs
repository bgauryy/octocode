import test from 'node:test';
import assert from 'node:assert/strict';
import {
  validatePlan,
  run,
} from '../../dist/engine/cdp-checks/browser-execute.mjs';
import { call, fixture } from './plan-fixture.mjs';

test('generic flattened sessions route commands and filter child events', async () =>
  fixture(async ({ cdp, emit, read, sent }) => {
    cdp.send = async (method, params, sessionId) => {
      sent.push({ method, params, sessionId });
      if (method === 'Target.attachToTarget') return { sessionId: 'child' };
      if (method === 'Runtime.evaluate') {
        emit(
          'Runtime.consoleAPICalled',
          { type: 'log' },
          { sessionId: 'other' }
        );
        emit('Runtime.consoleAPICalled', { type: 'log' }, { sessionId });
        return { result: { value: 42 } };
      }
      return {};
    };
    const session = { $step: 1, pointer: '/sessionId' };
    await run(cdp, {
      steps: [
        call('Target.attachToTarget', { targetId: 'worker', flatten: true }),
        { op: 'listen', id: 'log', event: 'Runtime.consoleAPICalled', session },
        call('Runtime.enable', {}, { session }),
        call(
          'Runtime.evaluate',
          { expression: 'console.log(42)' },
          { session }
        ),
        { op: 'waitEvent', listener: 'log' },
      ],
    });
    const result = read('browser-result.json');
    assert.equal(result.ok, true);
    assert.equal(result.eventCoverage[0].observed, 1);
    assert.equal(
      sent.find(s => s.method === 'Runtime.evaluate').sessionId,
      'child'
    );
    assert.equal(result.steps[3].sessionId, 'child');
  }));
test('selector-frame listeners reject the whole plan before any earlier mutation', async () =>
  fixture(async ({ cdp, sent }) => {
    await assert.rejects(
      run(cdp, {
        steps: [
          call('Runtime.evaluate', { expression: 'window.mutated = true' }),
          {
            op: 'listen',
            id: 'child',
            event: 'Runtime.consoleAPICalled',
            frame: { selector: '#child' },
          },
        ],
      }),
      /Selector-frame event listeners are unsupported/
    );
    assert.equal(sent.length, 0);
    for (const frame of [{ id: 'child-target' }, { url: 'child.example' }])
      assert.doesNotThrow(() =>
        validatePlan({
          steps: [
            {
              op: 'listen',
              id: 'child',
              event: 'Runtime.consoleAPICalled',
              frame,
            },
          ],
        })
      );
  }));
test('ambiguous iframe selection preserves candidates and identifies an alternative', async () =>
  fixture(async ({ cdp, read, sent }) => {
    const candidates = [
      {
        targetId: 'child-one',
        type: 'iframe',
        parentId: 'root',
        url: 'https://child.example/one',
      },
      {
        targetId: 'child-two',
        type: 'iframe',
        parentId: 'root',
        url: 'https://child.example/two',
      },
    ];
    cdp.send = async (method, params) => {
      sent.push({ method, params });
      if (method === 'Target.getTargets') return { targetInfos: candidates };
      return {};
    };
    await run(cdp, {
      steps: [call('Runtime.enable', {}, { frame: { url: 'child.example' } })],
    });
    const result = read('browser-result.json');
    assert.equal(result.ok, false);
    const error = result.failure.error;
    for (const candidate of candidates) {
      assert.ok(error.includes(candidate.targetId), error);
      assert.ok(error.includes(candidate.url), error);
    }
    assert.match(error, /frame\.id/);
    assert.ok(!sent.some(({ method }) => method === 'Target.attachToTarget'));
  }));

test('trusted actions foreground the parent while explicit session DOM calls stay scoped', async () =>
  fixture(async ({ cdp, sent }) => {
    cdp.targetInfo.type = 'page';
    cdp.send = async (method, params, sessionId) => {
      sent.push({ method, params, sessionId });
      if (method === 'Runtime.evaluate')
        throw new Error('Stop after foreground routing observation');
      return {};
    };
    await run(cdp, {
      steps: [
        {
          op: 'act',
          action: 'click',
          selector: '#advance',
          session: 'child-session',
        },
      ],
    });
    assert.equal(
      sent.find(row => row.method === 'Page.bringToFront')?.sessionId,
      undefined
    );
    assert.equal(
      sent.find(row => row.method === 'DOM.enable')?.sessionId,
      'child-session'
    );
    assert.equal(
      sent.find(row => row.method === 'Runtime.enable')?.sessionId,
      'child-session'
    );
    assert.equal(
      sent.filter(row => row.method === 'Page.bringToFront').length,
      1
    );
  }));

test('browser-owned child actions activate the exact page owner and reject unresolved or cyclic owners', async () => {
  for (const topology of ['valid', 'missing', 'cycle'])
    await fixture(async ({ cdp, sent, read }) => {
      cdp.send = async (method, params, sessionId) => {
        sent.push({ method, params, sessionId });
        if (method === 'Target.getTargetInfo')
          return {
            targetInfo: {
              targetId: 'child',
              type: 'iframe',
              parentId: 'parent',
            },
          };
        if (method === 'Target.getTargets')
          return {
            targetInfos:
              topology === 'valid'
                ? [{ targetId: 'parent', type: 'page' }]
                : topology === 'cycle'
                  ? [
                      { targetId: 'parent', type: 'iframe', parentId: 'child' },
                      { targetId: 'child', type: 'iframe', parentId: 'parent' },
                    ]
                  : [],
          };
        if (method === 'Runtime.evaluate')
          throw new Error('Stop before mutation');
        return {};
      };
      await run(cdp, {
        steps: [
          {
            op: 'act',
            action: 'click',
            selector: '#advance',
            session: 'child-session',
          },
        ],
      });
      assert.ok(!sent.some(row => row.method === 'Page.bringToFront'));
      assert.equal(
        sent.find(row => row.method === 'DOM.enable')?.sessionId,
        'child-session'
      );
      if (topology === 'valid') {
        assert.deepEqual(
          sent.find(row => row.method === 'Target.activateTarget')?.params,
          { targetId: 'parent' }
        );
        assert.equal(
          sent.find(row => row.method === 'Target.activateTarget')?.sessionId,
          undefined
        );
      } else {
        assert.ok(!sent.some(row => row.method === 'Target.activateTarget'));
        assert.match(
          read('browser-result.json').failure.error,
          /top-level page owner.*select.*page/i
        );
      }
    });
});
