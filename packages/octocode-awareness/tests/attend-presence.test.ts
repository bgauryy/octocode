import { DatabaseSync } from 'node:sqlite';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { initDb } from '../src/db-init.js';
import { registerAgent } from '../src/agents.js';
import { attendWorkspace } from '../src/attend-presence.js';
import { cmdAttend } from '../src/commands/repo.js';
import { parseArgs } from '../src/command-parser.js';
import { validateFlagValues } from '../src/cli-adapter/cli-routing.js';

const databases: DatabaseSync[] = [];
afterEach(() => { vi.restoreAllMocks(); for (const db of databases.splice(0)) db.close(); });
function fixture() {
  const db = new DatabaseSync(':memory:'); initDb(db); databases.push(db);
  for (let i = 0; i < 7; i++) registerAgent(db, { agentId: `peer-${i}`, workspacePath: '/repo', agentName: `Peer ${i}` });
  registerAgent(db, { agentId: 'elsewhere', workspacePath: '/other' });
  return db;
}

describe('default attend presence', () => {
  it('reads only workspace peers, excludes departed sessions, and does not search memories or workboards', () => {
    const db = fixture();
    db.prepare("UPDATE awareness_agents SET status = 'LEFT' WHERE agent_id = 'peer-6'").run();
    const prepare = vi.spyOn(db, 'prepare');
    const result = attendWorkspace(db, { workspacePath: '/repo', agentId: 'peer-0' });
    expect(result).toMatchObject({ mode: 'presence', count: 6, partial: false, self_id: 'peer-0' });
    expect(prepare.mock.calls).toHaveLength(1);
    expect(prepare.mock.calls[0]?.[0]).toContain('FROM awareness_agents');
    expect(result).not.toHaveProperty('organ_state');
    expect(result).not.toHaveProperty('evidence');
  });

  it('executes every next peer page without omissions', () => {
    const db = fixture();
    const peers = new Set<string>();
    let offset = 0;
    for (;;) {
      const result = attendWorkspace(db, { workspacePath: '/repo', limit: 2, offset });
      if (!('peers' in result)) throw new Error('expected presence');
      for (const peer of result.peers) peers.add(String(peer['agent_id']));
      if (!result.partial) break;
      const continuation = result.next!.list.command;
      expect(continuation.name).toBe('attend');
      expect(continuation.args).toEqual(expect.arrayContaining(['--workspace', '/repo', '--limit', '2']));
      const args = parseArgs(continuation.args);
      validateFlagValues(args);
      const out = vi.spyOn(process.stdout, 'write').mockImplementation(() => true);
      expect(cmdAttend(db, args, ':memory:', { compact: true })).toBe(0);
      const nextPage = JSON.parse(String(out.mock.calls.at(-1)![0]));
      out.mockRestore();
      expect(nextPage.mode).toBe('presence');
      expect(nextPage.offset).toBe(Number(args['offset']));
      offset = nextPage.offset;
    }
    expect([...peers].sort()).toEqual(Array.from({ length: 7 }, (_, i) => `peer-${i}`));
  });

  it('keeps detailed inspection explicit and rejects invalid continuations', () => {
    const db = fixture();
    expect(attendWorkspace(db, { workspacePath: '/repo', details: true })).toHaveProperty('workboard');
    expect(attendWorkspace(db, { workspacePath: '/repo', file: 'a.ts' })).toHaveProperty('workboard');
    expect(() => attendWorkspace(db, { workspacePath: '/repo', offset: -1 })).toThrow('offset');
    expect(() => attendWorkspace(db, { workspacePath: '/repo', limit: NaN })).toThrow('limit');
    expect(() => attendWorkspace(db, { workspacePath: '/repo', details: true, offset: 2 })).toThrow('offset');
  });
});
