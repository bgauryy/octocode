import { describe, expect, it } from 'vitest';

import {
  getLspStatus,
  pooledClientCount,
  releaseAllPooledClients,
  releasePooledClientForFile,
} from '../../src/lsp/manager.js';

describe('LSP manager status/restart primitives', () => {
  it('returns general status without requiring a file path', async () => {
    const status = await getLspStatus();

    expect(status).toMatchObject({
      enabled: true,
      pooledClientCount: expect.any(Number),
      pooledClients: expect.any(Array),
      hints: [
        'Provide filePath to check language server availability for a specific file.',
      ],
    });
  });

  it('returns unsupported language status for files without an LSP mapping', async () => {
    const status = await getLspStatus({
      filePath: '/workspace/README.md',
      workspaceRoot: '/workspace',
    });

    expect(status).toMatchObject({
      enabled: true,
      filePath: '/workspace/README.md',
      workspaceRoot: '/workspace',
      serverAvailable: false,
    });
    expect(status.languageId).toBeUndefined();
    expect(status.hints[0]).toContain('No language server is available');
  });

  it('releasePooledClientForFile returns false for unsupported extensions', async () => {
    await expect(
      releasePooledClientForFile('/workspace', '/workspace/README.md')
    ).resolves.toBe(false);
  });

  it('releaseAllPooledClients clears the process-local pool', async () => {
    await releaseAllPooledClients();

    expect(pooledClientCount()).toBe(0);
  });
});
