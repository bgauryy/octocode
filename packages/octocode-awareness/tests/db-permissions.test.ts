import { chmodSync, mkdtempSync, rmSync, statSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { describe, expect, it } from 'vitest';

import { connectDb } from '../src/db-runtime.js';

describe('Awareness database permissions', () => {
  it('repairs the memory directory and database to owner-only access', () => {
    const root = mkdtempSync(join(tmpdir(), 'awareness-permissions-'));
    const memory = join(root, 'memory');
    const dbPath = join(memory, 'awareness.sqlite3');
    try {
      const db = connectDb(dbPath);
      db.close();
      chmodSync(memory, 0o755);
      const reopened = connectDb(dbPath);
      reopened.close();
      expect(statSync(memory).mode & 0o777).toBe(0o700);
      expect(statSync(dbPath).mode & 0o777).toBe(0o600);
    } finally {
      rmSync(root, { recursive: true, force: true });
    }
  });
});
