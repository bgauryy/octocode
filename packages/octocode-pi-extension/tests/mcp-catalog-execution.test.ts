import { describe, expect, it, vi } from 'vitest';
import { McpCatalogExecution } from '../src/tools/mcp/catalog-execution.js';

const identity = {
  workspace: '/workspace',
  server: 'octocode',
  signature: 'v1',
};

describe('McpCatalogExecution', () => {
  it('shares one discovery and commits it once', async () => {
    const coordinator = new McpCatalogExecution<string>();
    const discover = vi.fn(async () => 'catalog');
    const commit = vi.fn();

    await expect(Promise.all([
      coordinator.resolve(identity, discover, commit),
      coordinator.resolve(identity, discover, commit),
    ])).resolves.toEqual(['catalog', 'catalog']);

    expect(discover).toHaveBeenCalledTimes(1);
    expect(commit).toHaveBeenCalledTimes(1);
    expect(coordinator.isFresh(identity)).toBe(true);
  });

  it('fences a late discovery after invalidation', async () => {
    const coordinator = new McpCatalogExecution<string>();
    let release!: (value: string) => void;
    const discover = vi.fn(() => new Promise<string>((resolve) => { release = resolve; }));
    const commit = vi.fn();
    const pending = coordinator.resolve(identity, discover, commit);

    await vi.waitFor(() => expect(discover).toHaveBeenCalledOnce());
    coordinator.invalidateServer(identity.server);
    release('stale');

    await expect(pending).rejects.toThrow(/catalog changed.*retry/i);
    expect(commit).not.toHaveBeenCalled();
    expect(coordinator.isFresh(identity)).toBe(false);
    await expect(coordinator.resolve(identity, async () => 'fresh', commit)).resolves.toBe('fresh');
  });
});
