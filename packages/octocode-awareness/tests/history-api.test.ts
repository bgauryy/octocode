import { mkdirSync, mkdtempSync, realpathSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { connectDb } from '../src/db-runtime.js';
import { runAwarenessHistoryOperation } from '../src/history-api.js';

describe('history-api lazy facade', () => {
  let root: string;
  let workspace: string;
  let db: ReturnType<typeof connectDb>;

  beforeEach(() => {
    root = mkdtempSync(join(tmpdir(), 'awareness-history-api-'));
    workspace = join(root, 'repo');
    mkdirSync(workspace);
    workspace = realpathSync(workspace);
    db = connectDb(join(root, 'awareness.sqlite3'));
    writeFileSync(join(workspace, 'a.ts'), 'before');
  });

  afterEach(() => {
    db.close();
    rmSync(root, { recursive: true, force: true });
  });

  it('runAwarenessHistoryOperation loads the history backend on demand', async () => {
    const result = await runAwarenessHistoryOperation(db, 'status', { workspace });
    expect(result).toBeTypeOf('object');
  });
});
