/**
 * T2.2 — lspRestart tool: clears the shared LSP client pool.
 *
 * Pure unit test on the handler — we don't spin up MCP for this.
 */
import { describe, expect, it, vi, beforeEach } from 'vitest';

const mocks = vi.hoisted(() => ({
  release: vi.fn(async () => {}),
  count: vi.fn(() => 0),
}));

vi.mock('../../src/lsp/manager.js', () => ({
  releaseAllPooledClients: mocks.release,
  pooledClientCount: mocks.count,
}));

const { executeLspRestart } =
  await import('../../src/tools/lsp_restart/execution.js');

describe('T2.2 — executeLspRestart', () => {
  beforeEach(() => {
    mocks.release.mockClear();
    mocks.count.mockClear();
  });

  it('reports the pre-restart pool size, then clears the pool', async () => {
    mocks.count.mockReturnValueOnce(3); // before
    mocks.count.mockReturnValueOnce(0); // after
    const result = await executeLspRestart();
    expect(mocks.release).toHaveBeenCalledTimes(1);
    expect(result.status).toBe('hasResults');
    expect(result.clientsStopped).toBe(3);
    expect(result.poolSizeAfter).toBe(0);
  });

  it("returns 'empty' status when no clients were pooled", async () => {
    mocks.count.mockReturnValueOnce(0);
    mocks.count.mockReturnValueOnce(0);
    const result = await executeLspRestart();
    expect(result.status).toBe('empty');
    expect(result.clientsStopped).toBe(0);
  });

  it('surfaces errors from releaseAllPooledClients as an error result', async () => {
    mocks.count.mockReturnValueOnce(1);
    mocks.release.mockRejectedValueOnce(new Error('stop failed'));
    const result = await executeLspRestart();
    expect(result.status).toBe('error');
    expect(result.error).toMatch(/stop failed/);
  });

  it('stringifies non-Error rejections instead of crashing', async () => {
    mocks.count.mockReturnValueOnce(2);
    mocks.release.mockRejectedValueOnce('plain-string-rejection');
    const result = await executeLspRestart();
    expect(result.status).toBe('error');
    expect(result.error).toBe('plain-string-rejection');
  });
});
