import os from 'node:os';
import path from 'node:path';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { extensionHome, extensionStateDbPath } from '../src/extension-paths.js';
import { openOctocodeDb, openPersistentAwareness } from '../src/tools/storage-policy.js';

const mocks = vi.hoisted(() => ({
  openStateDb: vi.fn(() => ({ kind: 'extension-state' })),
  openAwareness: vi.fn(() => ({ kind: 'awareness' })),
}));

vi.mock('@octocodeai/octocode-awareness', () => ({
  openAwareness: mocks.openAwareness,
}));

vi.mock('@octocodeai/octocode-awareness/mcp-state', () => ({
  openOctocodeDb: mocks.openStateDb,
}));

describe('persistent storage policy', () => {
  const previousMode = process.env['OCTOCODE_STORAGE_MODE'];
  const previousHome = process.env['OCTOCODE_HOME'];

  afterEach(() => {
    if (previousMode === undefined) delete process.env['OCTOCODE_STORAGE_MODE'];
    else process.env['OCTOCODE_STORAGE_MODE'] = previousMode;
    if (previousHome === undefined) delete process.env['OCTOCODE_HOME'];
    else process.env['OCTOCODE_HOME'] = previousHome;
    mocks.openStateDb.mockClear();
    mocks.openAwareness.mockClear();
  });

  it('never opens or creates SQLite state in memory mode', () => {
    process.env['OCTOCODE_STORAGE_MODE'] = 'memory';
    expect(() => openOctocodeDb()).toThrow(
      'Persistent storage is disabled (storage.mode=memory)',
    );
    expect(mocks.openStateDb).not.toHaveBeenCalled();
    expect(() => openPersistentAwareness({ workspace: '/workspace' })).toThrow(
      'Persistent storage is disabled (storage.mode=memory)',
    );
    expect(mocks.openAwareness).not.toHaveBeenCalled();
  });

  it('passes an explicit extension-private database path for the default home', () => {
    process.env['OCTOCODE_STORAGE_MODE'] = 'persistent';

    const root = extensionHome();
    const dbPath = extensionStateDbPath();
    expect(path.relative(root, dbPath)).toBe(path.join('state', 'extension.sqlite3'));

    openOctocodeDb();

    expect(mocks.openStateDb).toHaveBeenCalledWith(dbPath);
  });

  it('contains the database under an OCTOCODE_HOME override', () => {
    const home = path.join(os.tmpdir(), `octocode-storage-policy-${process.pid}`);
    process.env['OCTOCODE_HOME'] = home;
    process.env['OCTOCODE_STORAGE_MODE'] = 'persistent';

    const dbPath = extensionStateDbPath();
    expect(dbPath).toBe(path.join(path.resolve(home), 'extension', 'state', 'extension.sqlite3'));
    expect(path.relative(extensionHome(home), dbPath)).not.toMatch(/^\.\.(?:[/\\]|$)/);

    openOctocodeDb();

    expect(mocks.openStateDb).toHaveBeenCalledWith(dbPath);
  });

  it('opens Awareness only when persistence is explicit', () => {
    process.env['OCTOCODE_STORAGE_MODE'] = 'persistent';

    openPersistentAwareness({ workspace: '/workspace' });

    expect(mocks.openAwareness).toHaveBeenCalledWith({ workspace: '/workspace' });
  });
});
