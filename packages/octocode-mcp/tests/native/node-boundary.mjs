import assert from 'node:assert/strict';
import { Client } from '@modelcontextprotocol/sdk/client/index.js';
import { InMemoryTransport } from '@modelcontextprotocol/server';
import { getNativeContractFingerprint } from '@octocodeai/config/schema';
// Raw-node boundary smoke test: exercises the SHIPPED artifact under plain node
// (no vitest transform). Requires a prior `yarn build` so dist/public.js exists.
import { createNativeMcp, loadNativeBinding } from '../../dist/public.js';

class FakeRuntime {
  static instance;
  abiVersion = 2;
  executions = [];
  closed = false;
  constructor() {
    FakeRuntime.instance = this;
  }
  catalog() {
    return {
      fingerprint: getNativeContractFingerprint(),
      tools: [
        { name: 'localFetch', available: true },
        { name: 'ghSearch', available: false },
      ],
    };
  }
  cancel() {
    return true;
  }
  async executeMcp(requestId, tool, input) {
    this.executions.push({ requestId, tool, input });
    return { content: [], isError: true };
  }
  async close() {
    this.closed = true;
  }
}

assert.equal(typeof loadNativeBinding({}).NativeRuntime, 'function');
const instance = createNativeMcp({
  binding: { NativeRuntime: FakeRuntime },
  env: {},
});
const client = new Client({ name: 'native-boundary', version: '1' });
const [serverTransport, clientTransport] = InMemoryTransport.createLinkedPair();
await Promise.all([
  instance.server.connect(serverTransport),
  client.connect(clientTransport),
]);

const list = await client.listTools();
assert.deepEqual(
  list.tools.map(tool => tool.name),
  ['localFetch']
);
assert.equal(typeof list.tools[0].description, 'string');
assert.equal(Object.hasOwn(list.tools[0], 'outputSchema'), false);

const result = await client.callTool({
  name: 'localFetch',
  arguments: {
    queries: [{ reasoning: 'boundary test', path: '.', fullContent: true }],
  },
});
assert.deepEqual(result, { content: [], isError: true });
assert.equal(FakeRuntime.instance.executions.length, 1);
assert.equal(FakeRuntime.instance.executions[0].tool, 'localFetch');

await client.close();
await instance.close();
assert.equal(FakeRuntime.instance.closed, true);
console.log(JSON.stringify({ filtering: true, execution: true, close: true }));
