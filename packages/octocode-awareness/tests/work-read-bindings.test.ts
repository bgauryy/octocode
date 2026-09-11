import { mkdtempSync, realpathSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { afterEach, describe, expect, it } from 'vitest';
import { executeAwarenessCommand, type AwarenessCommandCall } from '../src/command-api.js';

const roots: string[] = [];
afterEach(() => { for (const root of roots.splice(0)) rmSync(root, { recursive: true, force: true }); });
describe('work read selectors and acting identity', () => {
  it.each([false, true])('executes peer inspection with the original host context (compact=%s)', async compact => {
    const workspace = realpathSync(mkdtempSync(join(tmpdir(), 'work-selector-')));
    roots.push(workspace);
    const context = { database: join(workspace, 'ledger.sqlite3'), workspace, agentId: 'owner', compact };
    const start = await executeAwarenessCommand({ command: 'work start', params: { file: ['peer.ts'], rationale: 'Peer intent', test_plan: 'Peer check' } }, { ...context, agentId: 'peer' });
    expect(start.exitCode).toBe(0);
    const attend = await executeAwarenessCommand({ command: 'attend', params: { details: true, file: ['peer.ts'] } }, context);
    expect(attend.exitCode).toBe(0);
    const next = (attend.payload as { next: { command: AwarenessCommandCall } }).next.command;
    expect(next.command).toBe('work show');
    const inspection = await executeAwarenessCommand(next, context);
    expect(inspection.exitCode, JSON.stringify(inspection.payload)).toBe(0);
    expect(inspection.payload).toMatchObject({ files: [expect.objectContaining({ agent_id: 'peer' })] });
    expect((await executeAwarenessCommand({ command: 'work list', params: { agent_id: 'peer' } }, context)).payload)
      .toMatchObject({ files: [expect.objectContaining({ agent_id: 'peer' })] });
    const denied = await executeAwarenessCommand({ command: 'work start', params: { agent_id: 'peer', file: ['other.ts'] } }, context);
    expect(denied.exitCode).toBe(1);
    expect(JSON.stringify(denied.payload)).toContain('conflicts with the host binding');
  });
});
