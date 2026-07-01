/**
 * Audit #11: a materialized GitHub listing must not leak clone byproducts
 * (.git internals, the .octocode-clone-meta.json marker) into result rows or
 * pagination totals. executeMaterialize must inject those exclusions into the
 * re-rooted local query.
 */
import { describe, expect, it, vi, beforeEach } from 'vitest';

const { runDirect } = vi.hoisted(() => ({ runDirect: vi.fn() }));
const { executeLocal } = vi.hoisted(() => ({ executeLocal: vi.fn() }));
vi.mock('../../src/oql/adapters/runner.js', async importOriginal => ({
  ...(await importOriginal<object>()),
  runDirect,
}));
vi.mock('../../src/oql/adapters/local.js', () => ({ executeLocal }));

import { executeMaterialize } from '../../src/oql/adapters/materialize.js';
import type { OqlQuery } from '../../src/oql/types.js';

function cloneOk(localPath: string) {
  return {
    content: [],
    structuredContent: {
      base: '/tmp',
      results: [{ status: 'success', data: { localPath } }],
    },
  };
}

beforeEach(() => {
  runDirect.mockReset();
  executeLocal.mockReset();
  executeLocal.mockResolvedValue({
    results: [],
    diagnostics: [],
    provenance: [],
  });
});

describe('audit #11: materialized listings exclude clone byproducts', () => {
  it('injects .git + clone-meta exclusions into the re-rooted local query', async () => {
    runDirect.mockResolvedValue(cloneOk('/tmp/clone'));
    await executeMaterialize({
      schema: 'oql',
      target: 'files',
      from: { kind: 'github', repo: 'facebook/react' },
    } as OqlQuery);

    expect(executeLocal).toHaveBeenCalledTimes(1);
    const localQuery = executeLocal.mock.calls[0]![0] as OqlQuery;
    expect(localQuery.from).toEqual({
      kind: 'materialized',
      localPath: '/tmp/clone',
      source: { kind: 'github', repo: 'facebook/react' },
    });
    expect(localQuery.scope?.excludeDir).toContain('.git');
    expect(localQuery.scope?.exclude).toContain('.octocode-clone-meta.json');
  });

  it('preserves user-supplied excludeDir/exclude alongside the defaults', async () => {
    runDirect.mockResolvedValue(cloneOk('/tmp/clone'));
    await executeMaterialize({
      schema: 'oql',
      target: 'files',
      from: { kind: 'github', repo: 'a/b' },
      scope: { excludeDir: ['node_modules'], exclude: ['*.log'] },
    } as OqlQuery);

    const q = executeLocal.mock.calls[0]![0] as OqlQuery;
    expect(q.scope?.excludeDir).toEqual(
      expect.arrayContaining(['node_modules', '.git'])
    );
    expect(q.scope?.exclude).toEqual(
      expect.arrayContaining(['*.log', '.octocode-clone-meta.json'])
    );
  });
});
