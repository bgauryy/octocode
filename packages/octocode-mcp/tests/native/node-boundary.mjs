import assert from 'node:assert/strict';
import { createNativeMcp, loadNativeBinding } from '../../src/native/index.mjs';

class FakeRuntime {
  static instance;
  cancelled = [];
  executions = 0;
  closed = false;
  constructor() { FakeRuntime.instance = this; }
  catalog() {
    return {
      mcpInstructions: 'local-only',
      tools: [
        { name: 'localFetch', title: 'Local', available: true, inputSchema: {}, outputSchema: {}, annotations: {} },
        { name: 'ghSearch', available: false, inputSchema: {} },
      ],
    };
  }
  cancel(id) { this.cancelled.push(id); return true; }
  executeMcp() { this.executions += 1; return new Promise(resolve => { this.resolve = resolve; }); }
  async close() { this.closed = true; }
}

assert.throws(() => loadNativeBinding({}), /OCTOCODE_NATIVE_BINDING/);
const instance = createNativeMcp({ binding: { NativeRuntime: FakeRuntime }, env: {} });
assert.equal(instance.server._instructions, 'local-only');
const list = await instance.server._requestHandlers.get('tools/list')(
  { method: 'tools/list', params: {} },
  {},
);
assert.deepEqual(list.tools.map(tool => tool.name), ['localFetch']);

const controller = new AbortController();
const pending = instance.server._requestHandlers.get('tools/call')(
  { method: 'tools/call', params: { name: 'localFetch', arguments: {} } },
  {
    requestId: 42,
    signal: controller.signal,
    mcpReq: { id: 42, signal: controller.signal, requestState: () => undefined },
  },
);
await new Promise(resolve => setImmediate(resolve));
controller.abort();
assert.deepEqual(FakeRuntime.instance.cancelled, ['42']);
FakeRuntime.instance.resolve({ content: [], isError: true });
assert.deepEqual(await pending, { content: [], isError: true });
const alreadyAborted = new AbortController();
alreadyAborted.abort();
await assert.rejects(instance.server._requestHandlers.get('tools/call')(
  { method: 'tools/call', params: { name: 'localFetch', arguments: {} } },
  { requestId: 43, signal: alreadyAborted.signal,
    mcpReq: { id: 43, signal: alreadyAborted.signal, requestState: () => undefined } },
), { name: 'AbortError' });
assert.equal(FakeRuntime.instance.executions, 1, 'pre-aborted requests must never reach native execution');
await instance.close();
assert.equal(FakeRuntime.instance.closed, true);
console.log(JSON.stringify({ filtering: true, cancellation: true, close: true }));
