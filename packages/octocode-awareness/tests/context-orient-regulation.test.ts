import { afterEach, expect, it } from 'vitest';
import { mkdtempSync, realpathSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { createAwarenessClient } from '../src/client.js';
import { executeAwarenessCli } from '../src/command-cli.js';

const roots: string[] = [];
afterEach(() => { for (const root of roots.splice(0)) rmSync(root, { recursive: true, force: true }); });
function fixture() {
  const workspace = realpathSync(mkdtempSync(join(tmpdir(), 'awareness-regulation-integration-')));
  roots.push(workspace);
  return { workspace, database: join(workspace, 'aw.sqlite3'), agentId: 'agent', sessionId: 'session' };
}

it('reports unavailable sensors without inventing degraded recovery', async () => {
  const result = await createAwarenessClient(fixture()).orient();
  expect(result).not.toHaveProperty('recovery');
  expect(result).toMatchObject({ operational: { unavailable: expect.arrayContaining(['context']) }, regulation: { advisory: true } });
  expect(Buffer.byteLength(JSON.stringify(result))).toBeLessThanOrEqual(1_500);
});

it('preserves owned verification advice in the canonical CLI and API', async () => {
  const context = fixture();
  const client = createAwarenessClient(context);
  const started = await client.execute({ operation: 'work.create', params: {
    kind: 'standalone', file: ['owned.ts'], rationale: 'change', test_plan: 'test owned behavior',
  } });
  expect(started.exitCode, JSON.stringify(started.payload)).toBe(0);
  const ended = await client.execute({ operation: 'work.update', params: {
    transition: 'end', run_id: (started.payload as { run_id: string }).run_id,
  } });
  expect(ended.exitCode, JSON.stringify(ended.payload)).toBe(0);
  const orientation = await client.orient();
  expect(orientation).toMatchObject({ regulation: { actions: expect.arrayContaining(['verify_owned_work']) } });
  const cli = await executeAwarenessCli(['context', 'orient', '--db', context.database,
    '--workspace', context.workspace, '--agent-id', context.agentId, '--session-id', context.sessionId]);
  expect(cli.exitCode).toBe(0);
  expect(cli.payload).toEqual(orientation);
});

it('shows scoped peer overlap after compact projection without treating self-only work as a peer', async () => {
  const context = fixture(); const client = createAwarenessClient(context);
  const self = await client.execute({ operation: 'work.create', params: {
    kind: 'standalone', file: ['self.ts'], rationale: 'own change', test_plan: 'test',
  } });
  const peer = await createAwarenessClient({ ...context, agentId: 'peer', sessionId: 'peer-session' }).execute({ operation: 'work.create', params: {
    kind: 'standalone', file: ['peer.ts'], rationale: 'peer change', test_plan: 'test',
  } });
  expect(self.exitCode, JSON.stringify(self.payload)).toBe(0);
  expect(peer.exitCode, JSON.stringify(peer.payload)).toBe(0);
  expect(await client.orient({ file: ['peer.ts'] })).toMatchObject({
    work: { overlaps: [{ path: 'peer.ts' }] }, regulation: { actions: expect.arrayContaining(['inspect_overlap']) },
  });
  expect(await client.orient({ file: ['self.ts'] })).toMatchObject({ work: { overlaps: [] } });
});
