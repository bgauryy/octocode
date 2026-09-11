import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { afterEach, describe, expect, it } from 'vitest';
import { defaultDbPath } from '../src/coordination/coordination-shared.js';
import { createAwarenessClient } from '../src/client.js';
import { resolveDbPath } from '../src/db-runtime.js';
import { AWARENESS_DB_FILENAME, parseStorageScope, repoDatabasePath } from '../src/storage-scope.js';
import { AWARENESS_SCHEMA_VERSION } from '../src/db-schema.js';
import { extractGlobalDb } from '../src/cli-adapter/cli-routing.js';

const originalMemoryHome = process.env.OCTOCODE_AGENT_DIR;
const originalOctocodeHome = process.env.OCTOCODE_HOME;
const originalAgentDir = process.env.OCTOCODE_AGENT_DIR;

afterEach(() => {
  if (originalMemoryHome === undefined) delete process.env.OCTOCODE_AGENT_DIR;
  else process.env.OCTOCODE_AGENT_DIR = originalMemoryHome;
  if (originalOctocodeHome === undefined) delete process.env.OCTOCODE_HOME;
  else process.env.OCTOCODE_HOME = originalOctocodeHome;
  if (originalAgentDir === undefined) delete process.env.OCTOCODE_AGENT_DIR;
  else process.env.OCTOCODE_AGENT_DIR = originalAgentDir;
});

describe('Awareness storage scope', () => {
  it('uses Octocode home by default and keeps repository storage explicit', () => {
    const workspace = resolve('/tmp/awareness-workspace');
    const home = mkdtempSync(join(tmpdir(), 'awareness-agent-home-'));
    process.env.OCTOCODE_HOME = home;
    expect(AWARENESS_DB_FILENAME).toBe(`awareness-v${AWARENESS_SCHEMA_VERSION}.sqlite3`);
    expect(defaultDbPath(workspace)).toBe(join(home, 'awareness', 'awareness-v4.sqlite3'));
    expect(resolveDbPath(null, { workspace })).toBe(join(home, 'awareness', 'awareness-v4.sqlite3'));
    expect(repoDatabasePath(workspace, AWARENESS_DB_FILENAME))
      .toBe(join(workspace, '.octocode', 'awareness-v4.sqlite3'));
    expect(defaultDbPath(workspace, 'repo'))
      .toBe(join(workspace, '.octocode', 'awareness-v4.sqlite3'));
    expect(resolveDbPath(null, { scope: 'repo', workspace }))
      .toBe(join(workspace, '.octocode', 'awareness-v4.sqlite3'));
    rmSync(home, { recursive: true, force: true });
  });

  it('uses a distinct global Awareness path and preserves explicit path precedence', () => {
    const home = mkdtempSync(join(tmpdir(), 'awareness-agent-home-'));
    process.env.OCTOCODE_HOME = home;
    try {
      expect(resolveDbPath(null, { scope: 'global', workspace: '/tmp/repo' }))
        .toBe(join(home, 'awareness', 'awareness-v4.sqlite3'));
      expect(resolveDbPath('./explicit.sqlite3', { scope: 'repo', workspace: '/tmp/repo' }))
        .toBe(resolve('./explicit.sqlite3'));
    } finally {
      rmSync(home, { recursive: true, force: true });
    }
  });

  it('validates CLI storage scope values', () => {
    expect(parseStorageScope(undefined)).toBe('global');
    expect(parseStorageScope('repo')).toBe('repo');
    expect(parseStorageScope('global')).toBe('global');
    expect(() => parseStorageScope('workspace')).toThrow('--db-scope must be repo or global');
  });

  it('extracts database scope before root command routing', () => {
    expect(extractGlobalDb(['--db-scope', 'repo', 'maintenance', 'init']))
      .toEqual({ dbPath: null, dbScope: 'repo', filtered: ['maintenance', 'init'] });
    expect(extractGlobalDb(['--db=/tmp/scratch.sqlite3', '--db-scope=global', 'coordination', 'status']))
      .toEqual({ dbPath: '/tmp/scratch.sqlite3', dbScope: 'global', filtered: ['coordination', 'status'] });
  });

  it('routes canonical client state to Octocode home without creating a repository database', async () => {
    const workspace = mkdtempSync(join(tmpdir(), 'awareness-repo-scope-'));
    const home = mkdtempSync(join(tmpdir(), 'awareness-agent-home-'));
    process.env.OCTOCODE_HOME = home;
    try {
      const client = createAwarenessClient({ workspace, agentId: 'reader' });
      await client.orient();
      const dbPath = join(home, 'awareness', 'awareness-v4.sqlite3');
      expect(existsSync(dbPath)).toBe(true);
      expect(client.context.workspace).toMatch(/awareness-repo-scope-/);
      expect(existsSync(join(workspace, '.octocode', 'awareness-v4.sqlite3'))).toBe(false);
      expect(existsSync(join(home, 'agent', 'agent.sqlite3'))).toBe(false);
    } finally {
      rmSync(workspace, { recursive: true, force: true });
      rmSync(home, { recursive: true, force: true });
    }
  });

  it('starts a fresh current generation without touching an older default database', async () => {
    const workspace = mkdtempSync(join(tmpdir(), 'awareness-versioned-workspace-'));
    const home = mkdtempSync(join(tmpdir(), 'awareness-versioned-home-'));
    process.env.OCTOCODE_HOME = home;
    const predecessor = join(home, 'awareness', 'awareness-v3.sqlite3');
    mkdirSync(join(home, 'awareness'), { recursive: true });
    writeFileSync(predecessor, 'predecessor-store');
    try {
      const client = createAwarenessClient({ workspace, agentId: 'reader' });
      await client.orient();
      expect(readFileSync(predecessor, 'utf8')).toBe('predecessor-store');
      expect(existsSync(join(home, 'awareness', 'awareness-v4.sqlite3'))).toBe(true);
    } finally {
      rmSync(workspace, { recursive: true, force: true });
      rmSync(home, { recursive: true, force: true });
    }
  });

  it('lets an explicit database path override repository scope', async () => {
    const workspace = mkdtempSync(join(tmpdir(), 'awareness-explicit-scope-'));
    const dbPath = join(workspace, 'explicit.sqlite3');
    try {
      const client = createAwarenessClient({ workspace, agentId: 'reader', database: dbPath, scope: 'repo' });
      await client.orient();
      expect(existsSync(dbPath)).toBe(true);
      expect(existsSync(join(workspace, '.octocode', 'awareness-v4.sqlite3'))).toBe(false);
    } finally {
      rmSync(workspace, { recursive: true, force: true });
    }
  });
});
