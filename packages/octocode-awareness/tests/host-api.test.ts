import { mkdirSync, mkdtempSync, realpathSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { afterEach, expect, it } from 'vitest';
import { createAwarenessHost } from '../src/host-api.js';

const roots: string[] = [];
afterEach(() => roots.splice(0).forEach((root) => rmSync(root, { recursive: true, force: true })));

it('binds host identity and captures LocalGit history without an argv adapter', async () => {
  const root = mkdtempSync(join(tmpdir(), 'awareness-host-api-'));
  roots.push(root);
  const workspace = join(root, 'repo');
  mkdirSync(workspace);
  const canonicalWorkspace = realpathSync(workspace);
  writeFileSync(join(canonicalWorkspace, 'source.ts'), 'before\n');
  const host = createAwarenessHost({
    database: join(root, 'awareness.sqlite3'),
    workspace: canonicalWorkspace,
    agentId: 'pi-host',
    sessionId: 'session-1',
  });

  const captured = await host.captureHistory({
    phase: 'before',
    operation_id: 'edit-1',
    host: 'pi',
    file: ['source.ts'],
  });

  expect(captured).toMatchObject({
    ok: true,
    operation: {
      operation_id: 'edit-1',
      workspace_path: canonicalWorkspace,
      agent_id: 'pi-host',
      session_id: 'session-1',
    },
  });
});
