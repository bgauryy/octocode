import assert from 'node:assert/strict';
import { beforeAll, test } from 'vitest';
import { validateToolArguments } from '@earendil-works/pi-ai';
import extension from '../src/index.js';
import { OCTOCODE_SUPPORT_TOOL_NAMES } from '../src/constants.js';
import type { PiInstance, ToolDefinition } from '../src/types.js';

const tools = new Map<string, ToolDefinition>();
beforeAll(async () => {
  await extension({
    registerTool: (tool: ToolDefinition) => tools.set(tool.name, tool),
    registerCommand() {}, registerFlag() {}, on() {},
    getFlag: () => undefined, getCommands: () => [],
    getActiveTools: () => [], setActiveTools() {},
  } as unknown as PiInstance);
});

function validate(name: string, args: Record<string, unknown>) {
  const tool = tools.get(name)!;
  const prepared = tool.prepareArguments?.(args) ?? args;
  return validateToolArguments(
    { name, description: tool.description, parameters: tool.parameters },
    { type: 'toolCall', id: 'boundary-check', name, arguments: prepared as Record<string, unknown> },
  );
}

const coreExamples: Record<string, Record<string, unknown>> = {
  file: { type: 'write', path: '/tmp/example.txt', content: 'example' },
  bash: { command: 'true' },
  inspectMedia: { type: 'image', path: '/tmp/example.png' },
  media: { type: 'image', svg: '<svg/>', dest: '/tmp/example.png' },
  runFfmpeg: { args: ['-version'] },
  web: { query: 'Pi extension documentation' },
  chromeDebug: { scheme: 'debug' },
  agent: { type: 'inspect', agentId: 'example' },
  callTool: { toolType: 'example', mode: 'list' },
  skill: { type: 'load', action: 'list' },
  plan: { action: 'show' },
  localServer: { action: 'status' },
  askUser: { question: 'Choose a color', options: [{ value: 'blue', label: 'Blue' }, { value: 'green', label: 'Green' }] },
  MCPTool: { action: 'status' },
};

const communicationExamples: Record<string, Record<string, unknown>> = {
  peers: { after: 'peer-id' },
  send_message: { to: 'peer-id', body: 'Lease released', reasoning: 'Unblock the peer', wake: 'passive' },
  notify_all: { body: 'Shared work completed', reasoning: 'Share the result', wake: 'passive' },
  inbox: { after: 1 },
  ack: { message: 1 },
  subscribe: { topics: ['build'] },
  lock: { path: 'src/module.ts', reasoning: 'Reserve the edit path' },
  renew: { lease: 1 },
  unlock: { lease: 1 },
  lock_many: { paths: [{ path: 'src/old.ts' }, { path: 'src/new.ts' }], reasoning: 'Reserve both rename endpoints' },
  share_document: { name: 'handoff.md', content: 'Verified result and next action.' },
  read_document: { name: 'handoff.md', offset: 0, limit: 4096 },
  activity: { view: 'files', path: 'src', limit: 10 },
};

test('every registered tool has a representative input for its owning contract', () => {
  assert.deepEqual([...tools.keys()].sort(), [...OCTOCODE_SUPPORT_TOOL_NAMES, 'bash'].sort());
  assert.deepEqual([...Object.keys(coreExamples), ...Object.keys(communicationExamples)].sort(), [...tools.keys()].sort());
});

test('core support tools retain queries envelopes through the real Pi validator', () => {
  for (const [name, query] of Object.entries(coreExamples)) {
    const args = { queries: [{ reasoning: 'Check the host input boundary.', ...query }] };
    assert.deepEqual(validate(name, args), args, name);
    assert.throws(() => validate(name, { queries: [] }), /Validation failed/, name);
    assert.throws(() => validate(name, { ...args, unsupported: true }), /Validation failed/, name);
  }
});

test('communication tools preserve their native plain inputs through the real Pi validator', () => {
  for (const [name, args] of Object.entries(communicationExamples)) {
    assert.equal(tools.get(name)!.prepareArguments, undefined, `${name} must not add an envelope conversion`);
    assert.deepEqual(validate(name, args), args, name);
    assert.throws(() => validate(name, { queries: [args] }), /Validation failed/, name);
    assert.throws(() => validate(name, { ...args, unsupported: true }), /Validation failed/, name);
  }
});

test('native communication schemas retain meaningful field constraints', () => {
  const invalid: Record<string, Record<string, unknown>> = {
    peers: { after: {} },
    send_message: { to: 'peer-id', body: 'message', reasoning: ' ', wake: 'action' },
    notify_all: { body: 'message' },
    inbox: { after: -1 },
    ack: { message: 0 },
    subscribe: { topics: [{}] },
    lock: { path: {}, reasoning: 'Reserve a path' },
    renew: { lease: 0 },
    unlock: { lease: 0 },
    lock_many: { paths: [], reasoning: 'Reserve paths' },
    share_document: { name: '../handoff.md', content: 'Evidence' },
    read_document: { name: 'handoff.md', limit: 1 },
    activity: { snapshot: 'a'.repeat(40) },
  };
  assert.deepEqual(Object.keys(invalid).sort(), Object.keys(communicationExamples).sort());
  for (const [name, args] of Object.entries(invalid)) assert.throws(() => validate(name, args), /Validation failed/, name);
});

test('MCPTool preserves nested localSearch inputs through Pi validation', () => {
  const args = { queries: [{
    reasoning: 'Orient the package and locate its guidance.', action: 'call', server: 'octocode', tool: 'localSearch',
    arguments: { queries: [
      { operation: 'tree', path: '/repo/packages/octocode-pi-extension', maxDepth: 3 },
      { operation: 'files', path: '/repo/packages/octocode-pi-extension', names: ['AGENTS.md', 'ARCHITECTURE.md', 'README.md'] },
      { operation: 'text', path: '/repo/packages/octocode-pi-extension', searchText: 'subagent|research|session memory|memory.md|update user|progress update|key findings' },
    ] },
  }] };
  assert.deepEqual(validate('MCPTool', args), args);
});

test('the reported flat MCPTool call explains the missing outer envelope', () => {
  const flat = {
    server: 'octocode', tool: 'localSearch', action: 'call',
    reasoning: 'Orient the package and locate its scoped instructions.',
    arguments: { queries: [
      { operation: 'tree', path: '/repo/packages/octocode-pi-extension', maxDepth: 3, pageSize: 200 },
      { operation: 'files', path: '/repo/packages/octocode-pi-extension', pathPattern: '{AGENTS.md,ARCHITECTURE.md,README.md,src/**,tests/**,docs/**}', pageSize: 200 },
      { operation: 'text', path: '/repo/packages/octocode-pi-extension', searchText: 'subagent|research|session memory', pageSize: 100 },
    ] },
  };
  assert.equal(tools.get('MCPTool')!.prepareArguments, undefined, 'do not install a flat-contract compatibility shim');
  assert.throws(() => validate('MCPTool', flat), (error: Error) => {
    const rendered = tools.get('MCPTool')!.renderResult!(
      { content: [{ type: 'text', text: error.message }] }, { expanded: false }, undefined,
      { isError: true, invalidate() {} },
    ).render(200).join('\n');
    assert.match(rendered, /missing outer queries\[\]/i);
    return true;
  });
});

test('collapsed envelope-tool validation errors show the rejected field without private input', () => {
  for (const name of Object.keys(coreExamples)) {
    const tool = tools.get(name)!;
    let message = '';
    try { validate(name, { queries: 'privateInput' }); }
    catch (error) { message = (error as Error).message; }
    assert.match(message, /Validation failed/);
    const rendered = tool.renderResult!(
      { content: [{ type: 'text', text: message }] },
      { expanded: false }, undefined, { isError: true, invalidate() {} },
    ).render(200).join('\n');
    assert.match(rendered, /queries:.*array/, name);
    assert.doesNotMatch(rendered, /privateInput/, name);
  }
});
