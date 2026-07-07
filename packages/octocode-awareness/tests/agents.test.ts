import { describe, it, expect } from 'vitest';
import { DatabaseSync } from 'node:sqlite';
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
});
