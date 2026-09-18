import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { Client, InMemoryTransport } from '@modelcontextprotocol/client';
import { Server } from '@modelcontextprotocol/server';
import { ModelRegistry, ModelRuntime } from '@earendil-works/pi-coding-agent';
import { createAssistantMessageEventStream } from '@earendil-works/pi-ai';
import { test } from 'vitest';
import { __test__ as mcp } from '../src/tools/mcp-tool.js';
import type { PiContext } from '../src/types.js';

test("MCP client capability handlers expose only trusted roots and deny headless sampling/input", async (context) => {
  const requests = new Map<
    string,
    (request: { params: Record<string, unknown> }, extra: { mcpReq: { signal: AbortSignal } }) => Promise<unknown>
  >();
  const notifications = new Map<
    string,
    (notification: { params: Record<string, unknown> }) => Promise<void>
  >();
  const client = {
    setRequestHandler: (
      method: string,
      handler: (request: {
        params: Record<string, unknown>;
      }, extra: { mcpReq: { signal: AbortSignal } }) => Promise<unknown>,
    ) => requests.set(method, handler),
    setNotificationHandler: (
      method: string,
      handler: (notification: {
        params: Record<string, unknown>;
      }) => Promise<void>,
    ) => notifications.set(method, handler),
  };
  const cwd = fs.mkdtempSync(path.join(os.tmpdir(), "octo-mcp-roots-"));
  context.onTestFinished(() => fs.rmSync(cwd, { recursive: true, force: true }));
  mcp.registerMcpClientHandlers(client as never, "docs", {
    cwd,
    hasUI: false,
    mode: "rpc",
    isProjectTrusted: () => true,
  } as import("../src/types.js").PiContext, () => undefined);
  const extra = { mcpReq: { signal: new AbortController().signal } };
  const roots = (await requests.get("roots/list")!({ params: {} }, extra)) as {
    roots: Array<{ uri: string }>;
  };
  assert.equal(roots.roots.length, 1);
  assert.match(roots.roots[0]!.uri, /^file:/);
  await assert.rejects(
    () =>
      requests.get("sampling/createMessage")!({
        params: { messages: [], maxTokens: 10 },
      }, extra),
    /interactive model session is required/,
  );
  assert.deepEqual(
    await requests.get("elicitation/create")!({
      params: { message: "secret?", mode: "form" },
    }, extra),
    { action: "decline" },
  );
  assert.ok(notifications.has("notifications/message"));
  assert.ok(notifications.has("notifications/progress"));
});

function samplingHandler(ctx: PiContext, method = 'sampling/createMessage') {
  type Handler = (request: { params: Record<string, unknown> }, extra: { mcpReq: { signal: AbortSignal } }) => Promise<unknown>;
  const handlers = new Map<string, Handler>();
  mcp.registerMcpClientHandlers({
    setRequestHandler: (method: string, handler: Handler) => handlers.set(method, handler),
    setNotificationHandler: () => undefined,
  } as never, 'sampling-fixture', ctx, () => undefined);
  return handlers.get(method)!;
}

test('MCP sampling honors the approved token ceiling and reports length-limited output exactly', async () => {
  let options: unknown;
  const handler = samplingHandler({
    hasUI: true,
    model: { id: 'test-model' },
    ui: { confirm: async () => true },
    modelRegistry: { complete: async (_model: unknown, _context: unknown, received: unknown) => {
      options = received;
      return { content: [{ type: 'text', text: '  partial output\n' }], stopReason: 'length' };
    } },
  } as unknown as PiContext);
  const signal = new AbortController().signal;
  const response = await handler({ params: { messages: [], maxTokens: 17, temperature: 0.2 } }, { mcpReq: { signal } });
  assert.deepEqual(options, { signal, maxTokens: 17, temperature: 0.2 });
  assert.deepEqual(response, { role: 'assistant', content: { type: 'text', text: '  partial output\n' }, model: 'test-model', stopReason: 'maxTokens' });
});

test.each(['error', 'aborted', 'toolUse', 'pending', 'deferred'])('MCP sampling never reports %s as a successful completion', async (stopReason) => {
  const handler = samplingHandler({
    hasUI: true,
    model: { id: 'test-model' },
    ui: { confirm: async () => true },
    modelRegistry: { complete: async () => ({ content: [{ type: 'text', text: 'partial' }], stopReason }) },
  } as unknown as PiContext);
  await assert.rejects(handler({ params: { messages: [], maxTokens: 10 } }, { mcpReq: { signal: new AbortController().signal } }), /sampling.*(error|aborted|toolUse|pending|deferred)/);
});

test('MCP sampling rejects a late completion after request cancellation', async () => {
  const request = new AbortController();
  const handler = samplingHandler({
    hasUI: true,
    model: { id: 'test-model' },
    ui: { confirm: async () => true },
    modelRegistry: { complete: async () => {
      request.abort(new Error('cancelled during completion'));
      return { content: [{ type: 'text', text: 'late result' }], stopReason: 'stop' };
    } },
  } as unknown as PiContext);
  await assert.rejects(handler({ params: { messages: [], maxTokens: 10 } }, { mcpReq: { signal: request.signal } }), /cancelled during completion/);
});

const inputSchema = {
  type: 'object',
  properties: { count: { type: 'integer', minimum: 1, maximum: 3 }, choice: { type: 'string', enum: ['one', 'two'] } },
  required: ['count', 'choice'],
};

test('MCP form elicitation presents its exact schema and accepts valid input', async () => {
  let prompt = '';
  const handler = samplingHandler({
    hasUI: true,
    ui: {
      confirm: async (_title: string, message: string) => { prompt = message; return true; },
      editor: async () => '{"count":2,"choice":"two"}',
    },
  } as unknown as PiContext, 'elicitation/create');
  const response = await handler({ params: { message: 'Choose input', requestedSchema: inputSchema } }, { mcpReq: { signal: new AbortController().signal } });
  assert.ok(prompt.includes(JSON.stringify(inputSchema, null, 2)));
  assert.deepEqual(response, { action: 'accept', content: { count: 2, choice: 'two' } });
});

test('MCP URL elicitation discloses the exact destination before approval', async () => {
  const url = 'https://example.com/authorize?scope=repo';
  let prompt = '';
  let notice = '';
  const handler = samplingHandler({
    hasUI: true,
    ui: {
      confirm: async (_title: string, message: string) => { prompt = message; assert.equal(notice, ''); return true; },
      notify: (message: string) => { notice = message; },
    },
  } as unknown as PiContext, 'elicitation/create');
  const response = await handler({ params: { mode: 'url', message: 'Authorize access', url } }, { mcpReq: { signal: new AbortController().signal } });
  assert.ok(prompt.includes(url));
  assert.ok(notice.includes(url));
  assert.deepEqual(response, { action: 'accept' });
});

test.each(['{}', '{"count":4,"choice":"one"}', '{"count":2,"choice":"other"}'])('MCP form elicitation does not accept schema-invalid input %s', async (value) => {
  let warning = '';
  const handler = samplingHandler({
    hasUI: true,
    ui: { confirm: async () => true, editor: async () => value, notify: (message: string) => { warning = message; } },
  } as unknown as PiContext, 'elicitation/create');
  const response = await handler({ params: { message: 'Choose input', requestedSchema: inputSchema } }, { mcpReq: { signal: new AbortController().signal } });
  assert.deepEqual(response, { action: 'cancel' });
  assert.match(warning, /MCP input rejected/);
});

test('MCP sampling preserves the complete message payload and uses the request signal', async () => {
  const messages = [{ role: 'user', content: { type: 'text', text: 'exact  spacing\n'.repeat(4_000) + 'TAIL' } }];
  const request = new AbortController();
  let observed: unknown;
  let observedSignal: AbortSignal | undefined;
  let approvalSignal: AbortSignal | undefined;
  const handler = samplingHandler({
    hasUI: true,
    model: { id: 'test-model' },
    ui: { confirm: async (_title: string, _message: string, options: { signal?: AbortSignal }) => {
      approvalSignal = options.signal;
      return true;
    } },
    modelRegistry: { complete: async (_model: unknown, context: { messages: Array<{ content: string }> }, options: { signal?: AbortSignal }) => {
      observed = JSON.parse(context.messages[0]!.content);
      observedSignal = options.signal;
      return { content: [{ type: 'text', text: 'complete' }] };
    } },
  } as unknown as PiContext);
  await handler({ params: { messages, maxTokens: 20 } }, { mcpReq: { signal: request.signal } });
  assert.deepEqual(observed, messages);
  assert.equal(approvalSignal, request.signal);
  assert.equal(observedSignal, request.signal);
});

test('MCP sampling does not invoke the model when cancelled during approval', async () => {
  const request = new AbortController();
  let calls = 0;
  const handler = samplingHandler({
    hasUI: true,
    model: { id: 'test-model' },
    ui: { confirm: async () => { request.abort(new Error('request cancelled')); return true; } },
    modelRegistry: { complete: async () => { calls++; return { content: [{ type: 'text', text: 'must not run' }] }; } },
  } as unknown as PiContext);
  await assert.rejects(handler({ params: { messages: [], maxTokens: 20 } }, { mcpReq: { signal: request.signal } }), /request cancelled/);
  assert.equal(calls, 0);
});

test('MCP sampling crosses real SDK and Pi provider boundaries without clipping', async (context) => {
  const text = 'transport evidence\n'.repeat(3_000) + 'END';
  const client = new Client({ name: 'sampling-client', version: '1.0.0' }, {
    capabilities: { roots: {}, sampling: {}, elicitation: { form: {}, url: {} } },
    supportedProtocolVersions: ['2025-11-25'],
  });
  const server = new Server({ name: 'sampling-server', version: '1.0.0' }, {
    supportedProtocolVersions: ['2025-11-25'],
  });
  let received = '';
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'octo-mcp-provider-'));
  context.onTestFinished(() => fs.rmSync(root, { recursive: true, force: true }));
  const runtime = await ModelRuntime.create({ authPath: path.join(root, 'auth.json'), modelsPath: null, allowModelNetwork: false });
  const registry = new ModelRegistry(runtime);
  registry.registerProvider('mcp-fixture', {
    api: 'mcp-fixture-api',
    baseUrl: 'http://127.0.0.1:0',
    apiKey: 'fixture-key',
    models: [{
      id: 'test-model', name: 'MCP test model', reasoning: false, input: ['text'],
      cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0 }, contextWindow: 32_768, maxTokens: 1_024,
    }],
    streamSimple: (model, input, options) => {
      assert.ok(options?.signal instanceof AbortSignal);
      assert.equal(options.maxTokens, 20);
      const message = input.messages[0];
      assert.equal(message?.role, 'user');
      assert.equal(typeof message?.content, 'string');
      received = JSON.parse(message!.content as string)[0].content.text;
      const stream = createAssistantMessageEventStream();
      stream.push({ type: 'done', reason: 'stop', message: {
        role: 'assistant', content: [{ type: 'text', text: 'full response' }],
        api: model.api, provider: model.provider, model: model.id,
        stopReason: 'stop', timestamp: Date.now(),
        usage: { input: 1, output: 1, cacheRead: 0, cacheWrite: 0, totalTokens: 2, cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0, total: 0 } },
      } });
      return stream;
    },
  });
  const model = registry.find('mcp-fixture', 'test-model');
  assert.ok(model);
  mcp.registerMcpClientHandlers(client, 'sdk-fixture', {
    hasUI: true,
    model,
    ui: { confirm: async () => true },
    modelRegistry: registry,
  } as unknown as PiContext, () => undefined);
  const [clientTransport, serverTransport] = InMemoryTransport.createLinkedPair();
  try {
    await Promise.all([client.connect(clientTransport), server.connect(serverTransport)]);
    const result = await server.request({
      method: 'sampling/createMessage',
      params: { messages: [{ role: 'user', content: { type: 'text', text } }], maxTokens: 20 },
    });
    assert.equal(received, text);
    assert.deepEqual(result.content, { type: 'text', text: 'full response' });
    assert.equal(result.stopReason, 'endTurn');
  } finally {
    await Promise.all([client.close(), server.close()]);
  }
});

test('MCP form elicitation crosses the real SDK boundary with schema-valid input', async () => {
  const client = new Client({ name: 'form-client', version: '1.0.0' }, {
    capabilities: { roots: {}, sampling: {}, elicitation: { form: {}, url: {} } },
    supportedProtocolVersions: ['2025-11-25'],
  });
  const server = new Server({ name: 'form-server', version: '1.0.0' }, {
    supportedProtocolVersions: ['2025-11-25'],
  });
  mcp.registerMcpClientHandlers(client, 'form-fixture', {
    hasUI: true,
    ui: { confirm: async () => true, editor: async () => '{"count":2,"choice":"one"}' },
  } as unknown as PiContext, () => undefined);
  const [clientTransport, serverTransport] = InMemoryTransport.createLinkedPair();
  try {
    await Promise.all([client.connect(clientTransport), server.connect(serverTransport)]);
    const response = await server.request({
      method: 'elicitation/create',
      params: { mode: 'form', message: 'Choose input', requestedSchema: inputSchema },
    });
    assert.deepEqual(response, { action: 'accept', content: { count: 2, choice: 'one' } });
  } finally {
    await Promise.all([client.close(), server.close()]);
  }
});
