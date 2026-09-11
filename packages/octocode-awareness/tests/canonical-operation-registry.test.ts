import { describe, expect, it } from 'vitest';
import { existsSync, readFileSync } from 'node:fs';
import { z } from 'zod';
import {
  getAwarenessOperationDescriptor,
  listAwarenessOperationDescriptors,
} from '../src/schema/operation-catalog.js';

describe('canonical operation registry contract', () => {
  it('owns executable schema, validation, handler, effects, approval, bounds, and continuations', () => {
    for (const descriptor of listAwarenessOperationDescriptors()) {
      expect(() => z.fromJSONSchema(descriptor.inputSchema)).not.toThrow();
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
