import { DatabaseSync } from 'node:sqlite';
import { mkdtempSync, realpathSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { initDb } from '../src/db-init.js';
import { createAwarenessClient, type AwarenessExecutableCall } from '../src/client.js';

let root: string;
let path: string;
let db: DatabaseSync;
beforeEach(() => {
  root = realpathSync(mkdtempSync(join(tmpdir(), 'query-pages-')));
  path = join(root, 'ledger.sqlite3'); db = new DatabaseSync(path); initDb(db);
});
afterEach(() => { db.close(); rmSync(root, { recursive: true, force: true }); });
function seed(count: number) {
  const statement = db.prepare(`INSERT INTO task_runs
    (run_id, origin, agent_id, rationale, test_plan, status, workspace_path)
    VALUES (?, 'WORK', 'reader', 'fixture', 'fixture check', 'PENDING', ?)`);
  for (let index = 0; index < count; index++) statement.run(`run_fixture_${index}`, root);
}
async function call(request: AwarenessExecutableCall<'work.list'>) {
  const result = await createAwarenessClient({ database: path, workspace: root, agentId: 'reader' }).execute(request);
  expect(result.exitCode, JSON.stringify(result)).toBe(0);
  return result.payload as { rows: Array<{ id: string; item_type: string }>; is_partial: boolean;
    next?: { list: { command: AwarenessExecutableCall<'work.list'> } }; terminal_limit?: { code: string; limit: number } };
}
describe('executable query continuations', () => {
  it('executes continuations until the union covers every bounded workboard row', async () => {
    seed(7);
    const ids = new Set<string>();
    let request: AwarenessExecutableCall<'work.list'> | undefined = {
      operation: 'work.list', params: { kind: 'workboard', limit: 1 },
    };
    for (let guard = 0; request && guard < 10; guard++) {
      const page = await call(request);
      for (const row of page.rows) if (row.item_type === 'run') ids.add(row.id);
      request = page.next?.list.command;
    }
    expect(request).toBeUndefined();
    expect(ids).toEqual(new Set(Array.from({ length: 7 }, (_, index) => `run_fixture_${index}`)));
  });
  it('reports a terminal limit when the workboard cannot return more detail', async () => {
    seed(51);
    const page = await call({ operation: 'work.list', params: { kind: 'workboard', limit: 50 } });
    expect(page.is_partial).toBe(true);
    expect(page.next).toBeUndefined();
    expect(page.terminal_limit).toMatchObject({ code: 'QUERY_VIEW_LIMIT', limit: 50 });
  });
});
