import { mkdtemp, rm, writeFile } from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import { afterEach, describe, expect, it } from 'vitest';
import {
  acquirePooledClient,
  getLspStatus,
  isLanguageServerAvailable,
  pooledClientCount,
  releaseAllPooledClients,
  releasePooledClientForFile,
} from '../src/manager.js';

const tempDirs: string[] = [];

afterEach(async () => {
  await releaseAllPooledClients();
  while (tempDirs.length > 0)
    await rm(tempDirs.pop()!, { recursive: true, force: true });
});

async function workspace(): Promise<string> {
  const root = await mkdtemp(path.join(os.tmpdir(), 'octocode-lsp-manager-'));
  tempDirs.push(root);
  return root;
}

describe('native LSP manager', () => {
  it('reports generic status', async () => {
    await expect(getLspStatus()).resolves.toMatchObject({
      enabled: true,
      pooledClientCount: 0,
    });
    expect(pooledClientCount()).toBe(0);
  });

  it('returns null for unknown language clients', async () => {
    const root = await workspace();
    const file = path.join(root, 'demo.unknown');
    await writeFile(file, 'plain\n');

    await expect(isLanguageServerAvailable(file, root)).resolves.toBe(false);
    await expect(acquirePooledClient(root, file)).resolves.toBeNull();
    await expect(releasePooledClientForFile(root, file)).resolves.toBe(false);
  });

  it('reports file status using native language detection', async () => {
    const root = await workspace();
    const file = path.join(root, 'demo.ts');
    await writeFile(file, 'export const value = 1;\n');

    await expect(
      getLspStatus({ filePath: file, workspaceRoot: root })
    ).resolves.toMatchObject({
      enabled: true,
      filePath: file,
      workspaceRoot: root,
      languageId: 'typescript',
    });
  });
});
