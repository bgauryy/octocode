import { afterEach, describe, expect, it } from 'vitest';
import { mkdtempSync, realpathSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { createAwarenessClient, type AwarenessExecutableCall } from '../src/client.js';
import { connectDb } from '../src/db-runtime.js';
import { runAwarenessHistoryOperation } from '../src/history.js';
import {
  getAwarenessOperationDescriptor,
} from '../src/schema/operation-catalog.js';

const roots: string[] = [];
afterEach(() => {
  for (const root of roots.splice(0)) rmSync(root, { recursive: true, force: true });
});

function fixture() {
  const workspace = realpathSync(mkdtempSync(join(tmpdir(), 'awareness-client-budget-')));
  roots.push(workspace);
  return { workspace, agentId: 'owner', database: join(workspace, 'awareness.sqlite3') };
}

describe('canonical client held-out budgets', () => {
  it('fails closed without a continuation and finds a nested executable retry', async () => {
    const context = fixture();
    const files = Array.from({ length: 80 }, (_, index) =>
      `${String(index).padStart(3, '0')}-${'long-history-path-'.repeat(8)}.txt`);
    for (const file of files) writeFileSync(join(context.workspace, file), `content ${file}`);
    const db = connectDb(context.database);
    try {
      const captured = await runAwarenessHistoryOperation(db, 'capture', {
        workspace: context.workspace, agent_id: context.agentId, phase: 'before',
        operation_id: 'wide-preview', file: files,
      }) as { ok: boolean };
      expect(captured.ok).toBe(true);
    } finally { db.close(); }

    const client = createAwarenessClient(context);
    const result = await client.execute({
      operation: 'history.restore',
      params: { action: 'preview', operation_id: 'wide-preview', side: 'before' },
    });
    expect(result).toMatchObject({ exitCode: 1, payload: {
      error: expect.stringContaining('exceeded its output budget without an executable continuation'),
    } });

    const previewDb = connectDb(context.database);
    let previewId: string;
    try {
      const preview = await runAwarenessHistoryOperation(previewDb, 'restore-preview', {
        workspace: context.workspace, agent_id: context.agentId,
        operation_id: 'wide-preview', side: 'before',
      });
      previewId = String(preview.preview_id);
    } finally { previewDb.close(); }
    const applied = await client.execute({
      operation: 'history.restore', params: { action: 'apply', preview_id: previewId! },
    });
    expect(applied).toMatchObject({ exitCode: 2, payload: {
      error_code: 'OUTPUT_BUDGET_EXCEEDED',
      next: { retry: { operation: 'history.restore', params: { action: 'preview' } } },
    } });
  });

  it('shrinks an oversized polymorphic list with an executable retry', async () => {
    const client = createAwarenessClient(fixture());
    for (let index = 0; index < 32; index += 1) {
      const created = await client.execute({ operation: 'work.create', params: {
        kind: 'plan', name: `Plan ${index} ${'n'.repeat(90)}`,
        objective: `Objective ${index} ${'o'.repeat(420)}`,
      } });
      expect(created.exitCode, JSON.stringify(created.payload)).toBe(0);
    }
    const oversized = await client.execute({ operation: 'work.list', params: { kind: 'plan', limit: 32, full: true } });
    expect(oversized).toMatchObject({ exitCode: 2, payload: {
      error_code: 'OUTPUT_BUDGET_EXCEEDED',
      next: { retry: { operation: 'work.list', params: { kind: 'plan' } } },
    } });
    const retry = (oversized.payload as Record<string, any>).next.retry as AwarenessExecutableCall<'work.list'>;
    expect(retry.params?.limit).toBeGreaterThanOrEqual(1);
    expect(retry.params?.limit).toBeLessThan(32);
  });
});

describe('canonical operation catalog held-out behavior', () => {
  it('canonicalizes nested continuations without leaking host bindings', () => {
    const descriptor = getAwarenessOperationDescriptor('message.list')!;
    const result = descriptor.continuations({
      untouched: { command: 'signal list', params: { workspace: '/outside' } },
      next: { command: 'signal list', params: { workspace: '/outside', agent_id: 'forged', limit: 2, offset: 2 } },
      actions: [{ command: 'lock wait', args: { database: '/tmp/nope', target_file: ['a.ts'] } }],
      continuations: [{ command: 'unknown command', params: { workspace: '/still-data' } }],
      undo_preview: { command: 'history restore-apply', args: { preview_id: 'preview-1', session_id: 'forged' } },
    }) as Record<string, any>;
    expect(result.untouched).toEqual({ command: 'signal list', params: { workspace: '/outside' } });
    expect(result.next).toEqual({ operation: 'message.list', params: { limit: 2, offset: 2 } });
    expect(result.actions[0]).toEqual({ operation: 'work.protect', params: { target_file: ['a.ts'], action: 'wait' } });
    expect(result.continuations[0]).toEqual({ command: 'unknown command', params: { workspace: '/still-data' } });
    expect(result.undo_preview).toEqual({ operation: 'history.restore', params: { preview_id: 'preview-1', action: 'apply' } });
  });

});
