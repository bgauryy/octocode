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

test('all registered core tools accept representative queries through the real Pi validator', () => {
  const examples: Record<string, Record<string, unknown>> = {
    file: { type: 'write', path: '/tmp/example.txt', content: 'example' },
    bash: { command: 'true' },
    readMedia: { type: 'image', path: '/tmp/example.png' },
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
    memory: { action: 'recall', query: 'tools' },
    lock: { action: 'release', file: '/tmp/example.txt' },
    message: { action: 'read' },
    MCPTool: { action: 'status' },
  };
  assert.deepEqual([...tools.keys()].sort(), [...OCTOCODE_SUPPORT_TOOL_NAMES, 'bash'].sort());
  assert.deepEqual(Object.keys(examples).sort(), [...tools.keys()].sort());
  for (const [name, query] of Object.entries(examples)) {
    const args = { queries: [{ reasoning: 'Check the host input boundary.', ...query }] };
    assert.deepEqual(validate(name, args), args, name);
    assert.throws(() => validate(name, { queries: [] }), /Validation failed/, name);
    assert.throws(() => validate(name, { ...args, unsupported: true }), /Validation failed/, name);
  }
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
  assert.deepEqual(tools.get('MCPTool')!.prepareArguments?.(flat), flat, 'do not silently accept a retired flat contract');
  assert.throws(() => validate('MCPTool', flat), (error: Error) => {
    const rendered = tools.get('MCPTool')!.renderResult!(
      { content: [{ type: 'text', text: error.message }] }, { expanded: false }, undefined,
      { isError: true, invalidate() {} },
    ).render(200).join('\n');
    assert.match(rendered, /missing outer queries\[\]/i);
    return true;
  });
});

test('collapsed host validation errors show the rejected field across all core tools', () => {
  for (const [name, tool] of tools) {
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
