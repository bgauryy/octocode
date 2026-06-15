import { mkdtemp, rm, writeFile } from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import { afterEach, describe, expect, it } from 'vitest';
import { LSPClient } from '../src/client.js';
import type { LanguageServerConfig } from '../src/types.js';

const tempDirs: string[] = [];

afterEach(async () => {
  while (tempDirs.length > 0)
    await rm(tempDirs.pop()!, { recursive: true, force: true });
});

async function fixture(): Promise<{
  root: string;
  file: string;
  config: LanguageServerConfig;
}> {
  const root = await mkdtemp(
    path.join(os.tmpdir(), 'octocode-lsp-native-client-')
  );
  tempDirs.push(root);
  const file = path.join(root, 'demo.unknown');
  await writeFile(file, 'plain\n');
  return {
    root,
    file,
    config: {
      command: process.execPath,
      args: ['-e', 'process.stdin.resume()'],
      workspaceRoot: root,
      languageId: 'plaintext',
    },
  };
}

describe('LSPClient native wrapper', () => {
  it('constructs and exposes lifecycle methods without TypeScript JSON-RPC internals', async () => {
    const { config } = await fixture();
    const client = new LSPClient(config);

    expect(client.hasCapability('definitionProvider')).toBe(false);
    await expect(client.stop()).resolves.toBeUndefined();
    expect(client.getRecentStderr()).toEqual([]);
  });

  it('routes document operations through the native client', async () => {
    const { config, file } = await fixture();
    const client = new LSPClient(config);

    await expect(client.openDocument(file, 'plain\n')).rejects.toThrow(
      'LSP client not initialized'
    );
    await expect(client.closeDocument(file)).resolves.toBeUndefined();
  });
});
