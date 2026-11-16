import { test } from 'node:test';
import assert from 'node:assert/strict';
import { DatabaseSync } from 'node:sqlite';
import { join } from 'node:path';
import { binary, execFileSync, tempWorkspace } from './helpers.ts';

// An asynchronous SQLite backup inside the shared worker thread finishes only when the worker's event loop wakes for another reason;
// without a pending timer the loop slept for seconds (observed 8-30 s) although the copy of a few pages takes milliseconds.
test('an export inside the shared CLI worker returns in the time of the copy, not of an idle event loop', t => {
  const workspace = tempWorkspace(t, 'communication-worker-export-', { real: true });
  const database = join(workspace, 'source.sqlite');
  const call = (values: string[]) => JSON.parse(execFileSync(binary, [...values, '--database', database, '--workspace', workspace], { encoding: 'utf8' }));
  call(['join', '{"name":"worker-export","vendor":"generic"}']);
  const writer = new DatabaseSync(database);
  try {
    writer.exec('PRAGMA journal_mode=WAL; PRAGMA wal_autocheckpoint=0');
    for (let n = 0; n < 3; n++) {
      const started = performance.now();
      call(['db', 'export', JSON.stringify({ path: join(workspace, `archive-${n}.sqlite`) })]);
      const elapsed = performance.now() - started;
      assert.ok(elapsed < 3000, `export ${n} took ${Math.round(elapsed)} ms; a tiny database copies in milliseconds`);
    }
  } finally { writer.close(); }
});
