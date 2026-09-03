import assert from 'node:assert/strict';
import { test } from 'vitest';
import {
  CommandRegistry,
  ToolRegistry,
  createEffectSet,
  type CommandDefinition as CoreCommandDefinition,
  type ToolDefinition as CoreToolDefinition,
} from '@octocodeai/agent-core';
import {
  PiCommandRegistryAdapter,
  PiToolRegistryAdapter,
  createPiCanonicalRegistryComposition,
  getPiRegistryRegistrationReceipts,
} from '../src/adapters/pi-registry-adapters.js';
import type { CommandDefinition, PiInstance, ToolDefinition } from '../src/types.js';

function hostHarness() {
  const tools = new Map<string, ToolDefinition>();
  const commands = new Map<string, CommandDefinition>();
  const pi = {
    registerTool: (definition: ToolDefinition) => tools.set(definition.name, definition),
    registerCommand: (name: string, definition: CommandDefinition) => commands.set(name, definition),
    sendUserMessage: () => undefined,
    on: () => undefined,
  } as unknown as PiInstance;
  return { pi, tools, commands };
}

test('registers canonical tools through a policy-owning execution callback', async () => {
  const { pi, tools } = hostHarness();
  const registry = new ToolRegistry();
  const executions: string[] = [];
  const adapter = new PiToolRegistryAdapter(pi, registry, async (name, input) => {
    executions.push(`${name}:${input.context.cwd}:${input.callId}`);
    return { ok: true, content: 'done', detailsVersion: 1 };
  });
  const definition: CoreToolDefinition = {
    name: 'example', label: 'Example', description: 'Example tool', schemaVersion: 1,
    inputSchema: { type: 'object' }, outputSchema: { type: 'string' }, outputVersion: 1,
    policy: { effects: createEffectSet('write'), trust: 'workspace', approval: 'on-request', plan: 'allowed' },
    execute: async () => { throw new Error('adapter must dispatch through policy callback'); },
  };
  adapter.register(definition, 'plugin:example');

  const result = await tools.get('example')?.execute('call-1', {}, undefined, undefined, {
    cwd: '/workspace', mode: 'tui', sessionManager: { getSessionId: () => 'session-1' },
  });
  assert.deepEqual(executions, ['example:/workspace:call-1']);
  assert.deepEqual(result?.content, [{ type: 'text', text: 'done' }]);
  assert.equal(registry.get('example')?.owner, 'plugin:example');
});

test('registers canonical commands with narrowed command capabilities', async () => {
  const { pi, commands } = hostHarness();
  const registry = new CommandRegistry();
  let observedCapabilities: readonly string[] = [];
  const adapter = new PiCommandRegistryAdapter(pi, registry, async (_name, _args, context) => {
    observedCapabilities = [...context.capabilities];
    return { status: 'ok', message: 'done' };
  });
  const definition: CoreCommandDefinition = {
    name: 'example', description: 'Example command', permission: 'trusted-workspace', headless: 'unsupported',
    execute: async () => ({ status: 'ok' }),
  };
  adapter.register(definition, 'builtin');
  await commands.get('example')?.handler('one two', { cwd: '/workspace', mode: 'tui' });

  assert.deepEqual(observedCapabilities, ['session.read', 'session.mutate', 'model.select', 'settings.read', 'ui.interact']);
  assert.equal(registry.get('example')?.owner, 'builtin');
});

test('preserves Pi-only tool and command projections while recording host receipts', async () => {
  const { pi, tools, commands } = hostHarness();
  const composition = createPiCanonicalRegistryComposition(pi);
  const prepareArguments = (args: unknown) => ({ wrapped: args });
  const renderer = { render: () => ['rendered'], invalidate: () => undefined };
  const renderCall = () => renderer;
  const renderResult = () => renderer;
  const execute = async () => ({
    content: [{ type: 'text' as const, text: 'projected' }],
    details: { presentation: 'kept' },
  });

  composition.pi.registerTool?.({
    name: 'projected',
    label: 'Projected',
    description: 'Projected tool',
    promptSnippet: 'projection snippet',
    promptGuidelines: ['projection guideline'],
    parameters: { type: 'object' },
    prepareArguments,
    execute,
    renderCall,
    renderResult,
  });
  composition.pi.registerCommand?.('projected-command', {
    description: 'Projected command',
    getArgumentCompletions: () => [{ value: 'one', label: 'one', description: 'kept' }],
    handler: async () => undefined,
  });

  const projectedTool = tools.get('projected');
  assert.equal(projectedTool?.prepareArguments, prepareArguments);
  assert.equal(projectedTool?.renderCall, renderCall);
  assert.equal(projectedTool?.renderResult, renderResult);
  assert.equal(projectedTool?.promptSnippet, 'projection snippet');
  assert.deepEqual(projectedTool?.promptGuidelines, ['projection guideline']);
  assert.deepEqual((await projectedTool?.execute('call', {}))?.details, { presentation: 'kept' });
  assert.deepEqual(await commands.get('projected-command')?.getArgumentCompletions?.(''), [
    { value: 'one', label: 'one', description: 'kept' },
  ]);

  assert.equal(composition.toolRegistry.get('projected')?.owner, 'builtin:octocode-pi');
  assert.equal(composition.commandRegistry.get('projected-command')?.owner, 'builtin:octocode-pi');
  assert.deepEqual(getPiRegistryRegistrationReceipts(pi).map(({ kind, name }) => `${kind}:${name}`), [
    'command:projected-command',
    'tool:projected',
  ]);
});

test('does not publish a canonical registration or receipt when the Pi host rejects it', () => {
  const registry = new ToolRegistry();
  const pi = {
    registerTool: () => { throw new Error('host rejected registration'); },
  } as unknown as PiInstance;
  const adapter = new PiToolRegistryAdapter(pi, registry, async () => ({
    ok: true,
    content: 'unused',
    detailsVersion: 1,
  }));
  const definition: CoreToolDefinition = {
    name: 'rejected', label: 'Rejected', description: 'Rejected tool', schemaVersion: 1,
    inputSchema: { type: 'object' }, outputSchema: {}, outputVersion: 1,
    policy: { effects: createEffectSet('write'), trust: 'workspace', approval: 'on-request', plan: 'allowed' },
    execute: async () => ({ ok: true, content: 'unused', detailsVersion: 1 }),
  };

  assert.throws(() => adapter.register(definition, 'builtin'), /host rejected registration/);
  assert.equal(registry.get('rejected'), undefined);
  assert.deepEqual(adapter.listReceipts(), []);
});

test('validates the canonical tool contract before calling the Pi host', () => {
  const registry = new ToolRegistry();
  let hostCalls = 0;
  const pi = {
    registerTool: () => { hostCalls += 1; },
  } as unknown as PiInstance;
  const adapter = new PiToolRegistryAdapter(pi, registry, async () => ({
    ok: true,
    content: 'unused',
    detailsVersion: 1,
  }));
  const invalidDefinition = {
    name: '', label: 'Invalid', description: 'Invalid tool', schemaVersion: 1,
    inputSchema: { type: 'object' }, outputSchema: {}, outputVersion: 1,
    policy: { effects: createEffectSet('write'), trust: 'workspace', approval: 'on-request', plan: 'allowed' },
    execute: async () => ({ ok: true, content: 'unused', detailsVersion: 1 }),
  } as CoreToolDefinition;

  assert.throws(() => adapter.register(invalidDefinition, 'builtin'), /tool name/);
  assert.equal(hostCalls, 0);
  assert.deepEqual(adapter.listReceipts(), []);
});
