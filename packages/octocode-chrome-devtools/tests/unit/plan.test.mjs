import test from 'node:test';
import assert from 'node:assert/strict';
import {
  mkdtempSync,
  writeFileSync,
  readFileSync,
  existsSync,
  rmSync,
} from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import {
  validatePlan,
  run,
} from '../../dist/engine/cdp-checks/browser-execute.mjs';

const call = (method, params = {}, extra = {}) => ({
  op: 'cdp',
  method,
  params,
  ...extra,
});
async function fixture(fn) {
  const dir = mkdtempSync(join(tmpdir(), 'octo-plan-')),
    handlers = new Map(),
    sent = [];
  const cdp = {
    outputDir: dir,
    targetInfo: { id: 'root', type: 'browser' },
    on(event, handler) {
      if (!handlers.has(event)) handlers.set(event, new Set());
      handlers.get(event).add(handler);
    },
    off(event, handler) {
      handlers.get(event)?.delete(handler);
    },
    send: async (method, params, sessionId) => {
      sent.push({ method, params, sessionId });
      return {};
    },
    protocol: async () => ({
      domains: [
        {
          domain: 'Runtime',
          commands: [
            { name: 'evaluate', returns: [{ $ref: 'Debugger.ScriptId' }] },
          ],
          types: [],
        },
        { domain: 'Debugger', types: [{ id: 'ScriptId', type: 'string' }] },
      ],
    }),
    saveArtifact(name, data) {
      const file = join(dir, name);
      writeFileSync(file, JSON.stringify(data));
      return { file };
    },
  };
  const emit = (event, params, meta = {}) => {
    for (const handler of handlers.get(event) ?? []) handler(params, meta);
  };
  const read = name => JSON.parse(readFileSync(join(dir, name), 'utf8'));
  const previous = process.exitCode;
  try {
    await fn({ cdp, dir, emit, sent, read });
  } finally {
    process.exitCode = previous;
    rmSync(dir, { recursive: true, force: true });
  }
}

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
