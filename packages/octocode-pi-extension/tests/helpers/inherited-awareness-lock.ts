import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { openAwarenessStore } from '@octocodeai/octocode-awareness/host';

/** Exercise the registered host gate against a real peer lease in a non-default ledger. */
export async function assertInheritedAwarenessLockGate(runGate: (workspace: string) => Promise<unknown>): Promise<void> {
  const workspace = fs.mkdtempSync(path.join(os.tmpdir(), 'awareness-bound-lock-'));
  const priorDatabase = process.env.OCTOCODE_AWARENESS_DB;
  const dbPath = path.join(workspace, 'inherited.sqlite3');
  process.env.OCTOCODE_AWARENESS_DB = dbPath;
  try {
    fs.writeFileSync(path.join(workspace, 'shared.txt'), 'original');
    const store = openAwarenessStore({ workspace, dbPath });
    try {
      store.acquireLock({ filePath: 'shared.txt', agentId: 'bound-peer', reason: 'exclusive shared evidence', testPlan: 'check lock gate' });
    } finally {
      store.close();
    }
    const result = await runGate(workspace) as { block?: boolean; reason?: string } | undefined;
    assert.equal(result?.block, true);
    assert.match(result!.reason!, /bound-peer/);
    assert.equal(fs.readFileSync(path.join(workspace, 'shared.txt'), 'utf8'), 'original');
  } finally {
    if (priorDatabase === undefined) delete process.env.OCTOCODE_AWARENESS_DB;
    else process.env.OCTOCODE_AWARENESS_DB = priorDatabase;
    fs.rmSync(workspace, { recursive: true, force: true });
  }
}
