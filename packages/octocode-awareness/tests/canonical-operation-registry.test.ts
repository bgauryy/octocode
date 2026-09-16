import { describe, expect, it } from 'vitest';
import { existsSync, readFileSync } from 'node:fs';
import { z } from 'zod';
import {
  AWARENESS_HOST_PARAMETER_NAMES,
  getAwarenessOperationDescriptor,
  listAwarenessOperationDescriptors,
} from '../src/schema/operation-catalog.js';

describe('canonical operation registry contract', () => {
  it('owns the host-injected parameter names shared by runtime adapters', () => {
    expect(AWARENESS_HOST_PARAMETER_NAMES).toEqual([
      'db', 'database', 'workspace', 'agent_id', 'lead_agent_id', 'session_id', 'compact',
    ]);
  });

  it('owns executable, compact schema plus validation, policy, bounds, and continuations', () => {
    const descriptors = listAwarenessOperationDescriptors();
    const schemaBytes = descriptors.reduce(
      (total, descriptor) => total + Buffer.byteLength(descriptor.inputSchemaText),
      0,
    );
    expect(schemaBytes).toBeLessThanOrEqual(42_000);

    for (const descriptor of descriptors) {
      expect(() => z.fromJSONSchema(descriptor.inputSchema)).not.toThrow();
      expect(descriptor.inputSchemaText).toBe(JSON.stringify(descriptor.inputSchema));
      expect(JSON.parse(descriptor.inputSchemaText)).toEqual(descriptor.inputSchema);
      expect(descriptor.use.length).toBeGreaterThan(30);
      expect(typeof descriptor.validate).toBe('function');
      expect(typeof descriptor.handler).toBe('function');
      expect(typeof descriptor.effect).toBe('function');
      expect(typeof descriptor.approval).toBe('function');
      expect(descriptor.outputBudget).toBeGreaterThan(0);
      expect(typeof descriptor.continuations).toBe('function');
    }
  });

  it('validates the selected route and resolves parameter-sensitive policy', () => {
    const create = getAwarenessOperationDescriptor('work.create')!;
    expect(() => create.validate({ kind: 'invalid' })).toThrow(/kind must be one of/i);
    const send = getAwarenessOperationDescriptor('message.send')!;
    expect(() => send.validate({ kind: 'fyi', subject: 'Update', workspace: '/forged' }))
      .toThrow(/workspace|unknown/i);

    const restore = getAwarenessOperationDescriptor('history.restore')!;
    expect(restore.effect({ action: 'preview', operation_id: 'op1', side: 'before' })).toBe('read');
    expect(restore.approval({ action: 'preview', operation_id: 'op1', side: 'before' })).toBeUndefined();
    expect(restore.effect({ action: 'apply', preview_id: 'p1' })).toBe('workspace-write');
    expect(restore.approval({ action: 'apply', preview_id: 'p1' })).toBe('fs-delete');

    const protect = getAwarenessOperationDescriptor('work.protect')!;
    expect(protect.effect({ action: 'wait', target_file: ['src/a.ts'] })).toBe('read');
    expect(protect.effect({
      action: 'acquire', target_file: ['src/a.ts'], rationale: 'sensitive edit', test_plan: 'test',
    })).toBe('coordination-write');
  });

  it('rejects route inputs that cannot satisfy their domain handler', () => {
    const invalid: Array<[string, Record<string, unknown>]> = [
      ['work.create', { kind: 'task', plan_id: 'plan', title: 'Task', reasoning: 'Reason', acceptance: 'Done' }],
      ['work.create', { kind: 'standalone' }],
      ['work.create', { kind: 'standalone', file: ['src/a.ts'] }],
      ['work.claim', {}],
      ['work.claim', { next: true }],
      ['work.show', { kind: 'presence' }],
      ['work.depend', { task_id: 'task' }],
      ['work.protect', { action: 'release' }],
      ['work.verify', { action: 'mark' }],
      ['work.verify', { action: 'mark', run_id: ['run'] }],
      ['work.verify', { action: 'mark', run_id: ['run'], status: 'SUCCESS' }],
      ['work.verify', { action: 'mark', run_id: ['a', 'b'], message: 'checked', adopt_verification: true }],
      ['work.verify', { action: 'mark', all_pending: true, message: 'checked', adopt_verification: true }],
      ['message.resolve', {}],
      ['message.resolve', { signal_id: [] }],
      ['message.resolve', { signal_id: ['signal'], thread_id: 'thread' }],
    ];
    for (const [operation, params] of invalid) {
      const descriptor = getAwarenessOperationDescriptor(operation)!;
      expect(z.fromJSONSchema(descriptor.inputSchema).safeParse(params).success, `${operation} schema accepted ${JSON.stringify(params)}`).toBe(false);
      expect(() => descriptor.validate(params), `${operation} validator accepted ${JSON.stringify(params)}`).toThrow();
    }

    const valid: Array<[string, Record<string, unknown>]> = [
      ['work.create', { kind: 'task', plan_id: 'plan', title: 'Task', reasoning: 'Reason', acceptance: 'Done', path: ['src/a.ts'] }],
      ['work.create', { kind: 'standalone', file: ['src/a.ts'], rationale: 'Edit', test_plan: 'Test' }],
      ['work.create', { kind: 'standalone', file: ['src/a.ts'], run_id: 'run' }],
      ['work.claim', { task_id: 'task' }],
      ['work.claim', { next: true, plan_id: 'plan' }],
      ['work.show', { kind: 'presence', file: ['src/a.ts'] }],
      ['work.depend', { task_id: 'task', depends_on: ['dependency'] }],
      ['work.protect', { action: 'release', run_id: 'run' }],
      ['work.protect', { action: 'release', target_file: ['src/a.ts'] }],
      ['work.verify', { action: 'mark', run_id: ['run'], status: 'FAILED' }],
      ['work.verify', { action: 'mark', all_pending: true, message: 'Tests passed' }],
      ['work.verify', { action: 'mark', run_id: ['run'], message: 'Tests passed', adopt_verification: true }],
      ['message.resolve', { signal_id: ['signal'] }],
      ['message.resolve', { thread_id: 'thread' }],
    ];
    for (const [operation, params] of valid) {
      const descriptor = getAwarenessOperationDescriptor(operation)!;
      expect(z.fromJSONSchema(descriptor.inputSchema).safeParse(params).success, `${operation} schema rejected ${JSON.stringify(params)}`).toBe(true);
      expect(() => descriptor.validate(params), `${operation} validator rejected ${JSON.stringify(params)}`).not.toThrow();
    }
  });

  it('has no parallel command registry, compatibility mapper, or generic dispatcher', () => {
    const catalog = readFileSync(new URL('../src/schema/operation-catalog.ts', import.meta.url), 'utf8');
    const executor = readFileSync(new URL('../src/operation-executor.ts', import.meta.url), 'utf8');
    const cli = readFileSync(new URL('../src/command-cli.ts', import.meta.url), 'utf8');
    for (const path of [
      '../src/command-api.ts',
      '../src/command-contracts.ts',
      '../src/command-continuations.ts',
      '../src/command-dispatch.ts',
      '../src/commands/routes.ts',
      '../src/schema/command-catalog.ts',
    ]) {
      expect(existsSync(new URL(path, import.meta.url)), path).toBe(false);
    }
    for (const source of [catalog, executor, cli]) {
      for (const dependency of [
        'executeAwarenessCommand', 'COMMAND_ROUTES', 'commandIndex',
        'getAwarenessCommandDescriptor', 'operationCallForLegacyCommand', 'legacyCommands',
      ]) expect(source).not.toContain(dependency);
    }
  });
});
