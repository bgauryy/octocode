import { describe, it, expect } from 'vitest';
import { DatabaseSync } from 'node:sqlite';
import { mkdirSync, mkdtempSync, rmSync, symlinkSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { initDb } from '../src/db.js';
import { listAgents, registerAgent, resolveAgentName, resolveAgentNames, touchAgent } from '../src/agents.js';

function freshDb(): DatabaseSync {
  const db = new DatabaseSync(':memory:');
  db.exec('PRAGMA foreign_keys = ON');
  initDb(db);
  return db;
}

describe('agent identity registry', () => {
  it('registers agents in the shared awareness database and lists them by scope', () => {
    const db = freshDb();

    registerAgent(db, {
      agentId: 'codex-a',
      agentName: 'Codex A',
      workspacePath: '/repo',
      artifact: 'packages/octocode-awareness',
      context: 'codex',
    });
    registerAgent(db, {
      agentId: 'claude-b',
      agentName: 'Claude B',
      workspacePath: '/other',
      artifact: 'packages/octocode-awareness',
      context: 'claude-code',
    });
    registerAgent(db, {
      agentId: 'global-c',
      agentName: 'Global C',
      context: 'pi',
    });

    const scoped = listAgents(db, {
      workspacePath: '/repo',
      artifact: 'packages/octocode-awareness',
    });

    expect(scoped.agents.map((a) => a.agent_id)).toEqual(expect.arrayContaining(['global-c', 'codex-a']));
    expect(scoped.agents).toHaveLength(2);
    expect(scoped.agents.find((a) => a.agent_id === 'codex-a')?.context).toBe('codex');
  });

  it('does not overwrite a known display name with an empty registration', () => {
    const db = freshDb();

    registerAgent(db, {
      agentId: 'agent-a',
      agentName: 'Helpful Agent',
      workspacePath: '/repo',
    });
    registerAgent(db, {
      agentId: 'agent-a',
      agentName: '',
      workspacePath: '/repo',
    });

    expect(resolveAgentName(db, 'agent-a')).toBe('Helpful Agent');
  });

  it('touch updates last seen scope without creating a new identity', () => {
    const db = freshDb();

    registerAgent(db, {
      agentId: 'agent-a',
      agentName: 'Agent A',
      workspacePath: '/repo-a',
      artifact: 'pkg-a',
    });
    touchAgent(db, 'agent-a', '/repo-b', 'pkg-b');

    const listed = listAgents(db, { workspacePath: '/repo-b', artifact: 'pkg-b' });
    expect(listed.agents).toHaveLength(1);
    expect(listed.agents[0]?.agent_id).toBe('agent-a');
    expect(listed.agents[0]?.workspace_path).toBe('/repo-b');
    expect(listed.agents[0]?.artifact).toBe('pkg-b');
  });

  it('resolves multiple agent names for communication displays', () => {
    const db = freshDb();

    registerAgent(db, { agentId: 'agent-a', agentName: 'Agent A' });
    registerAgent(db, { agentId: 'agent-b', agentName: 'Agent B' });

    const names = resolveAgentNames(db, ['agent-a', 'agent-b', 'unknown']);
    expect(names.get('agent-a')).toBe('Agent A');
    expect(names.get('agent-b')).toBe('Agent B');
    expect(names.has('unknown')).toBe(false);
  });

  describe('workspace-scope symlink stability (regression)', () => {
    // registerAgent/touchAgent/listAgents used to store/query workspace_path
    // verbatim with no normalization, unlike memory/lock/signal which resolve
    // through fillScope. A symlinked workspace path (e.g. macOS /tmp ->
    // /private/tmp) could register under one form and list under another.
    function tempDirWithLink(): { real: string; link: string; base: string } {
      const base = mkdtempSync(join(tmpdir(), 'oc-agents-scope-'));
      const real = join(base, 'real');
      const link = join(base, 'link');
      mkdirSync(real, { recursive: true });
      symlinkSync(real, link);
      return { real, link, base };
    }

    it('an agent registered via a symlinked workspace path is listed via the real path', () => {
      const db = freshDb();
      const { real, link, base } = tempDirWithLink();
      try {
        registerAgent(db, { agentId: 'agent-link', agentName: 'Linked', workspacePath: link });
        const listed = listAgents(db, { workspacePath: real });
        expect(listed.agents.map((a) => a.agent_id)).toContain('agent-link');
      } finally {
        rmSync(base, { recursive: true, force: true });
      }
    });

    it('touchAgent normalizes the same way registerAgent does — no divergent duplicate row', () => {
      const db = freshDb();
      const { real, link, base } = tempDirWithLink();
      try {
        registerAgent(db, { agentId: 'agent-touch', agentName: 'Touch', workspacePath: real });
        touchAgent(db, 'agent-touch', link);
        const listedByRealPath = listAgents(db, { workspacePath: real });
        expect(listedByRealPath.agents.map((a) => a.agent_id)).toContain('agent-touch');
        // Still exactly one identity row — registering via `real` and touching
        // via `link` must key the same scope, not create two divergent rows.
        expect(listAgents(db).agents.filter((a) => a.agent_id === 'agent-touch')).toHaveLength(1);
      } finally {
        rmSync(base, { recursive: true, force: true });
      }
    });
  });
});
