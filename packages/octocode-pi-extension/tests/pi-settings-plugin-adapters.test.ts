import assert from 'node:assert/strict';
import { test } from 'vitest';
import {
  HookCatalog,
  LifecycleBus,
  SettingsRegistry,
  SettingsService,
  eventId,
  revision,
  sessionId,
} from '@octocodeai/agent-core';
import { PiPluginEventAdapter } from '../src/adapters/pi-plugin-adapter.js';
import { PiSettingsAdapter } from '../src/adapters/pi-settings-adapter.js';

test('delegates settings validation and revision ownership to agent-core', async () => {
  const registry = new SettingsRegistry();
  registry.register({
    key: 'runtime.footer-density', schemaVersion: 1, section: 'Appearance', order: 1,
    kind: { type: 'enum', values: ['compact', 'default', 'full'] }, scopes: ['session'],
    defaultValue: 'default', mutability: 'editable', application: 'immediate', visibility: 'public',
    owner: 'pi-extension', documentation: 'docs/SETTINGS.md',
  });
  const adapter = new PiSettingsAdapter(new SettingsService(registry));
  const observed: string[] = [];
  adapter.subscribe((result) => { observed.push(result.redactedImpact.join(',')); });
  const result = await adapter.mutate({
    protocolVersion: 1, requestId: 'request-1', action: 'set', scope: 'session',
    expectedRevision: revision('0'), payload: { key: 'runtime.footer-density', value: 'compact' },
  });
  assert.equal(result.ok, true);
  assert.equal(adapter.snapshot().values[0]?.value, 'compact');
  assert.deepEqual(observed, ['runtime.footer-density:set']);
});

test('plugin event subscriptions unload cleanly and reject non-transactional Pi contributions', async () => {
  const bus = new LifecycleBus<Record<string, unknown>>({
    eventType: 'tool.requested', authority: ['allow-deny'],
    validate: (payload): payload is Record<string, unknown> => Boolean(payload) && typeof payload === 'object',
  });
  const adapter = new PiPluginEventAdapter(new Map([['tool.requested', bus]]));
  adapter.subscribe('guard', { event: 'tool.requested', id: 'deny-write', handler: async () => ({ kind: 'deny', reason: 'plugin policy' }) });
  const envelope = {
    schemaVersion: 1 as const, eventVersion: 1 as const, id: eventId('event-1'), type: 'tool.requested' as const,
    phase: 'permission' as const, sessionId: sessionId('session-1'), timestamp: 1, cwd: '/w', mode: 'headless' as const,
    trust: { workspace: 'trusted' as const, managedOnly: false }, payload: { toolName: 'write' },
  };
  assert.equal((await bus.dispatch(envelope)).decision.kind, 'deny');
  adapter.unload('guard');
  assert.equal((await bus.dispatch(envelope)).decision.kind, 'continue');
  assert.doesNotThrow(() => adapter.assertTransactionalContribution('hook'));
  assert.throws(() => adapter.assertTransactionalContribution('tool'), /cannot transactionally unload tool/);

  assert.ok(HookCatalog, 'agent-core hook catalog remains the canonical owner');
});
