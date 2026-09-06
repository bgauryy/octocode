import { afterEach, describe, expect, it } from 'vitest';
import { chmodSync, mkdirSync, mkdtempSync, rmSync, statSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { closeOctocodeDb, openOctocodeDb } from '../src/db.js';
import { initOctocodeSchema, recordSession } from '../src/schema.js';
import {
  getMcpEnablement,
  getSkillEnablement,
  listMcpOverrides,
  listSkillOverrides,
  setSkillEnabled,
  setMcpServerEnabled,
  setMcpToolEnabled,
} from '../src/mcp-state.js';

const dirs: string[] = [];
function freshDbPath(): string {
  const dir = mkdtempSync(join(tmpdir(), 'octo-shared-'));
  dirs.push(dir);
  return join(dir, 'nested', 'agent.sqlite3');
}

afterEach(() => {
  while (dirs.length) {
    const dir = dirs.pop();
    if (dir) rmSync(dir, { recursive: true, force: true });
  }
});

describe('openOctocodeDb', () => {
  it('creates the file, its parent dir, and the agent schema', () => {
    const path = freshDbPath();
    const db = openOctocodeDb(path);
    const tables = db
      .prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
      .all() as Array<{ name: string }>;
    const names = tables.map((t) => t.name);
    expect(names).toContain('octocode_meta');
    expect(names).toContain('agent_sessions');
    expect(names).toContain('mcp_server_overrides');
    expect(names).toContain('mcp_tool_overrides');
    expect(names).toContain('skill_overrides');
    closeOctocodeDb(path);
  });

  it('hardens the database and its existing parent directory', () => {
    const path = freshDbPath();
    const parent = join(path, '..');
    mkdirSync(parent, { recursive: true });
    chmodSync(parent, 0o755);
    openOctocodeDb(path).exec('CREATE TABLE permission_probe (id INTEGER)');
    expect(statSync(parent).mode & 0o777).toBe(0o700);
    expect(statSync(path).mode & 0o777).toBe(0o600);
    closeOctocodeDb(path);
  });

  it('caches the connection per resolved path', () => {
    const path = freshDbPath();
    expect(openOctocodeDb(path)).toBe(openOctocodeDb(path));
    closeOctocodeDb(path);
  });

  it('is idempotent when re-initializing the schema', () => {
    const path = freshDbPath();
    const db = openOctocodeDb(path);
    expect(() => initOctocodeSchema(db)).not.toThrow();
    closeOctocodeDb(path);
  });
});

describe('MCP enablement state', () => {
  it('resolves workspace tool > workspace server > global tool > global server > config default', () => {
    const dbPath = freshDbPath();
    const db = openOctocodeDb(dbPath);
    expect(getMcpEnablement(db, '/repo', 'docs', 'search', true)).toBe(true);
    setMcpServerEnabled(db, '*', 'docs', false);
    expect(getMcpEnablement(db, '/repo', 'docs', 'search', true)).toBe(false);
    setMcpToolEnabled(db, '*', 'docs', 'search', true);
    expect(getMcpEnablement(db, '/repo', 'docs', 'search', true)).toBe(true);
    setMcpServerEnabled(db, '/repo', 'docs', false);
    expect(getMcpEnablement(db, '/repo', 'docs', 'search', true)).toBe(false);
    setMcpToolEnabled(db, '/repo', 'docs', 'search', true);
    expect(getMcpEnablement(db, '/repo', 'docs', 'search', true)).toBe(true);
    expect(listMcpOverrides(db, '/repo')).toMatchObject({
      servers: [{ scopeKey: '/repo', serverKey: 'docs', enabled: false }],
      tools: [{ scopeKey: '/repo', serverKey: 'docs', toolName: 'search', enabled: true }],
    });
    closeOctocodeDb(dbPath);
  });
});

describe('skill enablement state', () => {
  it('normalizes names and resolves workspace override before global default', () => {
    const dbPath = freshDbPath();
    const db = openOctocodeDb(dbPath);
    expect(getSkillEnablement(db, '/repo', 'Octocode Research')).toBe(true);
    setSkillEnabled(db, '*', '  Octocode   Research ', false);
    expect(getSkillEnablement(db, '/repo', 'octocode research')).toBe(false);
    setSkillEnabled(db, '/repo', 'OCTOCODE RESEARCH', true);
    expect(getSkillEnablement(db, '/repo', 'octocode research')).toBe(true);
    expect(listSkillOverrides(db, '/repo')).toEqual([
      { scopeKey: '/repo', skillKey: 'octocode research', enabled: true },
      { scopeKey: '*', skillKey: 'octocode research', enabled: false },
    ]);
    closeOctocodeDb(dbPath);
  });

  it('keeps duplicate Skill source overrides independent while honoring legacy name defaults', () => {
    const dbPath = freshDbPath();
    const db = openOctocodeDb(dbPath);
    setSkillEnabled(db, '*', 'review', false);
    setSkillEnabled(db, '/repo', 'review', true, 'agents:user:/home/.agents/skills');

    expect(getSkillEnablement(db, '/repo', 'review', true, 'agents:user:/home/.agents/skills')).toBe(true);
    expect(getSkillEnablement(db, '/repo', 'review', true, 'octocode:user:/home/.octocode/agent/skills')).toBe(false);
    closeOctocodeDb(dbPath);
  });
});

describe('recordSession', () => {
  it('upserts, preserving created_at while advancing updated_at', () => {
    const path = freshDbPath();
    const db = openOctocodeDb(path);
    recordSession(db, { sessionId: 's1', workspacePath: '/repo', cwd: '/repo/pkg' });
    const first = db.prepare('SELECT * FROM agent_sessions WHERE session_id = ?').get('s1') as {
      created_at: string;
      updated_at: string;
      workspace_path: string;
    };
    expect(first.workspace_path).toBe('/repo');

    recordSession(db, { sessionId: 's1', workspacePath: '/repo2' });
    const second = db.prepare('SELECT * FROM agent_sessions WHERE session_id = ?').get('s1') as {
      created_at: string;
      workspace_path: string;
    };
    expect(second.created_at).toBe(first.created_at);
    expect(second.workspace_path).toBe('/repo2');
    closeOctocodeDb(path);
  });

  it('defaults optional columns to null', () => {
    const path = freshDbPath();
    const db = openOctocodeDb(path);
    recordSession(db, { sessionId: 's2' });
    const row = db.prepare('SELECT * FROM agent_sessions WHERE session_id = ?').get('s2') as {
      workspace_path: string | null;
      cwd: string | null;
    };
    expect(row.workspace_path).toBeNull();
    expect(row.cwd).toBeNull();
    closeOctocodeDb(path);
  });
});
