import { existsSync, mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { afterEach, describe, expect, it } from 'vitest';
import { defaultDbPath } from '../src/coordination/coordination-shared.js';
import { runCli } from '../src/coordination/cli.js';
import { resolveDbPath } from '../src/db-runtime.js';
import { parseStorageScope, repoDatabasePath } from '../src/storage-scope.js';
import { extractGlobalDb } from '../bin/cli-routing.js';

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
    expect(defaultDbPath(workspace)).toBe(join(home, 'awareness', 'awareness.sqlite3'));
    expect(resolveDbPath(null, { workspace })).toBe(join(home, 'awareness', 'awareness.sqlite3'));
    expect(repoDatabasePath(workspace, 'awareness.sqlite3'))
      .toBe(join(workspace, '.octocode', 'awareness.sqlite3'));
    expect(defaultDbPath(workspace, 'repo'))
      .toBe(join(workspace, '.octocode', 'awareness.sqlite3'));
    expect(resolveDbPath(null, { scope: 'repo', workspace }))
      .toBe(join(workspace, '.octocode', 'awareness.sqlite3'));
    rmSync(home, { recursive: true, force: true });
  });

  it('uses a distinct global Awareness path and preserves explicit path precedence', () => {
    const home = mkdtempSync(join(tmpdir(), 'awareness-agent-home-'));
    process.env.OCTOCODE_HOME = home;
    try {
      expect(resolveDbPath(null, { scope: 'global', workspace: '/tmp/repo' }))
        .toBe(join(home, 'awareness', 'awareness.sqlite3'));
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

  it('routes coordination CLI state to Octocode home without creating a repository database', () => {
    const workspace = mkdtempSync(join(tmpdir(), 'awareness-repo-scope-'));
    const home = mkdtempSync(join(tmpdir(), 'awareness-agent-home-'));
    process.env.OCTOCODE_HOME = home;
    let stdout = '';
    try {
      expect(runCli(
        ['status', '--workspace', workspace],
        { write: (chunk) => { stdout += chunk; } },
      )).toBe(0);
      const dbPath = join(home, 'awareness', 'awareness.sqlite3');
      expect(existsSync(dbPath)).toBe(true);
      expect(JSON.parse(stdout)).toMatchObject({ dbPath, workspace: expect.stringMatching(/awareness-repo-scope-/) });
      expect(existsSync(join(workspace, '.octocode', 'awareness.sqlite3'))).toBe(false);
      expect(existsSync(join(home, 'agent', 'agent.sqlite3'))).toBe(false);
    } finally {
      rmSync(workspace, { recursive: true, force: true });
      rmSync(home, { recursive: true, force: true });
    }
  });

  it('lets an explicit database path override repository scope', () => {
    const workspace = mkdtempSync(join(tmpdir(), 'awareness-explicit-scope-'));
    const dbPath = join(workspace, 'explicit.sqlite3');
    let stdout = '';
    try {
      expect(runCli(
        ['status', '--workspace', workspace, '--db-scope', 'repo', '--db', dbPath],
        { write: (chunk) => { stdout += chunk; } },
      )).toBe(0);
      expect(JSON.parse(stdout)).toMatchObject({ dbPath });
      expect(existsSync(join(workspace, '.octocode', 'awareness.sqlite3'))).toBe(false);
    } finally {
      rmSync(workspace, { recursive: true, force: true });
    }
  });

  it('keeps hook installation on the single root CLI surface', () => {
    expect(() => runCli(['hooks', 'install', '--host', 'codex']))
      .toThrow('hooks installation is owned by the root hooks install command');
  });
});
