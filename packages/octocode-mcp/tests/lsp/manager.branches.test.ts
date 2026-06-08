/**
 * Targeted tests for manager.ts uncovered branches.
 * Covers: releasePooledClientForFile (lines 130-131) and getLspStatus (line 170).
 *
 * NOTE: No vi.mock('client.ts') here — that mock would leak into other test
 * files sharing the same Vitest worker and corrupt their module registry.
 */
import { describe, it, expect, vi } from 'vitest';

vi.mock('../../src/session.js', () => ({
  logSessionError: vi.fn(() => Promise.resolve()),
}));

describe('manager — releasePooledClientForFile (lines 130-131)', () => {
  it('returns true for a recognised file extension (.ts)', async () => {
    const { releasePooledClientForFile } = await import(
      '../../src/lsp/manager.js'
    );
    // .ts has a registered language server — languageIdForFile returns non-null.
    // sharedPool.clear() is called even when no client exists (no-op), then returns true.
    const released = await releasePooledClientForFile(
      '/workspace',
      '/workspace/foo.ts'
    );
    expect(released).toBe(true);
  });

  it('returns false for an unrecognised file extension', async () => {
    const { releasePooledClientForFile } = await import(
      '../../src/lsp/manager.js'
    );
    const released = await releasePooledClientForFile(
      '/workspace',
      '/workspace/foo.unknownxyz'
    );
    expect(released).toBe(false);
  });
});

describe('manager — getLspStatus (line 170 ?? branch)', () => {
  it('languageId is undefined for an unrecognised extension (null ?? undefined)', async () => {
    const { getLspStatus } = await import('../../src/lsp/manager.js');
    // .unknownxyz not in registry → languageIdForFile returns null
    // → null ?? undefined → languageId field is undefined
    const result = await getLspStatus({
      filePath: '/workspace/foo.unknownxyz',
      workspaceRoot: '/workspace',
    });
    expect(result.languageId).toBeUndefined();
  });

  it('returns base result with hint when no filePath provided', async () => {
    const { getLspStatus } = await import('../../src/lsp/manager.js');
    const result = await getLspStatus({});
    expect(result.enabled).toBe(true);
    expect(result.hints.length).toBeGreaterThan(0);
  });
});
