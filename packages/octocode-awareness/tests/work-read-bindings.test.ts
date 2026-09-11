import { mkdtempSync, realpathSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { afterEach, describe, expect, it } from 'vitest';
import { createAwarenessClient } from '../src/client.js';

const roots: string[] = [];
afterEach(() => { for (const root of roots.splice(0)) rmSync(root, { recursive: true, force: true }); });
describe('work read selectors and acting identity', () => {
  it('executes peer inspection with the original host context', async () => {
    const workspace = realpathSync(mkdtempSync(join(tmpdir(), 'work-selector-')));
    roots.push(workspace);
    const database = join(workspace, 'ledger.sqlite3');
    const owner = createAwarenessClient({ database, workspace, agentId: 'owner' });
    const peer = createAwarenessClient({ database, workspace, agentId: 'peer' });
    const start = await peer.execute({ operation: 'work.create', params: {
      kind: 'standalone', file: ['peer.ts'], rationale: 'Peer intent', test_plan: 'Peer check',
    } });
    expect(start.exitCode).toBe(0);
    const inspection = await owner.execute({ operation: 'work.show', params: { kind: 'presence', file: ['peer.ts'] } });
    expect(inspection.exitCode, JSON.stringify(inspection.payload)).toBe(0);
    expect(inspection.payload).toMatchObject({ files: [expect.objectContaining({ agent_id: 'peer' })] });
    const denied = await owner.execute({ operation: 'work.create', params: {
      kind: 'standalone', agent_id: 'peer', file: ['other.ts'], rationale: 'Peer spoof', test_plan: 'must fail',
    } });
    expect(denied.exitCode).toBe(1);
    expect(JSON.stringify(denied.payload)).toContain('invalid parameters');
  });
});
