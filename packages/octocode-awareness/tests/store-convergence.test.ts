import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { afterEach, describe, expect, it } from 'vitest';
import { insertMemory, openAwarenessStore } from '../src/index.js';
import { DatabaseSync } from 'node:sqlite';

const roots: string[] = [];
afterEach(() => roots.splice(0).forEach((root) => rmSync(root, { recursive: true, force: true })));

describe('single-store convergence', () => {
  it('recalls host and root memories through the same canonical store', () => {
    const workspace = mkdtempSync(join(tmpdir(), 'store-convergence-'));
    roots.push(workspace);
    const store = openAwarenessStore({ workspace, dbPath: join(workspace, 'awareness.sqlite3') });
    const db = new DatabaseSync(store.dbPath);
    try {
      store.storeVerifiedMemory({ label: 'OTHER', text: 'Use one store', sourceDigest: 'sha256:source' });
      store.storeMemory({ label: 'OTHER', text: 'host observation' });
      insertMemory(db, { taskContext: 'ledger', observation: 'full runtime row', importance: 5, workspacePath: workspace });
      expect(db.prepare('SELECT COUNT(*) AS count FROM awareness_memories').get()).toEqual({ count: 3 });
      expect(store.recallMemory({ query: 'full runtime row' }).map(row => row.text)).toContain('full runtime row');
      expect(store.recallVerifiedMemory({ sourceDigest: 'sha256:source' })).toHaveLength(1);
      expect(db.prepare("SELECT 1 FROM sqlite_schema WHERE name = 'memories'").get()).toBeUndefined();
    } finally { db.close(); store.close(); }
  });

});
