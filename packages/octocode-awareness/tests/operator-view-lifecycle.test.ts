import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { afterEach, describe, expect, it } from 'vitest';
import { registerAgent } from '../src/agents.js';
import { connectDb } from '../src/db-runtime.js';
import { initDb } from '../src/db-init.js';
import { createOperatorAwarenessView } from '../src/operator-view.js';
import { renderOperatorAwarenessView, type AwarenessViewSnapshot } from '../src/operator-view-html.js';

const roots: string[] = [];
afterEach(() => roots.splice(0).forEach(root => rmSync(root, { recursive: true, force: true })));

describe('operator view lifecycle inspection', () => {
  it('renders scoped agents and canonical entity lifecycle metadata with escaped fields', () => {
    const snapshot: AwarenessViewSnapshot = {
      generated_at: '2026-09-13T00:00:00Z',
      workspace: '/repo/<unsafe>',
      database: { path: '/db', sqlite_version: '3', journal_mode: 'wal', application_id: 1, user_version: 5 },
      local_git: { initialized: false },
      agents: [{ agent_id: '<script>peer</script>', agent_name: 'A & B', provenance: 'observed', workspace_path: '/repo', last_seen_at: 'now' }],
      entities: [{
        name: 'awareness_agents', family: 'identity', kind: 'table', owner: 'work',
        lifecycle: {
          access: 'read-write', retention: 'domain-lifecycle', deletion: 'store-only',
          cleanup_operation: 'maintenance store-retire',
        },
        columns: [{ cid: 0, name: 'agent_id', type: 'TEXT', not_null: 1, default_value: null, primary_key: 1 }],
        rows: [{ agent_id: '<script>peer</script>' }],
      }],
    };

    const html = renderOperatorAwarenessView(snapshot);
    expect(html).toContain('<h2>Agents · workspace scope</h2>');
    expect(html).toContain('Store-wide SQLite rows');
    expect(html).toContain('Workspace-scoped agents');
    expect(html).toContain('observed');
    expect(html).toContain('&lt;script&gt;peer&lt;/script&gt;');
    expect(html).not.toContain('<script>peer</script>');
    expect(html).toContain('owner: work');
    expect(html).toContain('read-write');
    expect(html).toContain('domain-lifecycle');
    expect(html).toContain('store-only');
    expect(html).toContain('maintenance store-retire');
    expect(html).not.toMatch(/https?:\/\//);
  });

  it('exhausts the paged projection for a complete operator snapshot', async () => {
    const workspace = mkdtempSync(join(tmpdir(), 'awareness-view-agents-'));
    roots.push(workspace);
    const database = join(workspace, 'awareness.sqlite3');
    const db = connectDb(database);
    initDb(db);
    for (let index = 0; index < 501; index += 1) {
      registerAgent(db, { agentId: `agent-${String(index).padStart(3, '0')}`, workspacePath: workspace });
    }
    db.close();

    const result = await createOperatorAwarenessView({
      database,
      workspace,
      out: join(workspace, 'view.html'),
      open: false,
    });
    expect(result.agent_count).toBe(501);
  });
});
