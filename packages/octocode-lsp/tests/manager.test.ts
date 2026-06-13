import { chmod, mkdir, mkdtemp, rm, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { afterEach, describe, expect, it } from 'vitest';
import {
  detectLanguageId,
  getLanguageServerForFile,
  loadUserConfig,
} from '../src/config.js';
import {
  acquirePooledClient,
  getLspStatus,
  isLanguageServerAvailable,
  pooledClientCount,
  releaseAllPooledClients,
  releasePooledClientForFile,
} from '../src/manager.js';
import type { LSPClient } from '../src/client.js';

const TEST_ROOT = fileURLToPath(
  new URL('../test-workspaces/', import.meta.url)
);
const CUSTOM_SERVER = fileURLToPath(
  new URL('../benchmark/custom/server.mjs', import.meta.url)
);
const originalConfigPath = process.env.OCTOCODE_LSP_CONFIG;
const originalGoServerPath = process.env.OCTOCODE_GO_SERVER_PATH;
const originalRustServerPath = process.env.OCTOCODE_RUST_SERVER_PATH;
const tempDirs: string[] = [];

afterEach(async () => {
  restoreEnv('OCTOCODE_LSP_CONFIG', originalConfigPath);
  restoreEnv('OCTOCODE_GO_SERVER_PATH', originalGoServerPath);
  restoreEnv('OCTOCODE_RUST_SERVER_PATH', originalRustServerPath);
  await releaseAllPooledClients();
  while (tempDirs.length > 0) {
    await rm(tempDirs.pop()!, { recursive: true, force: true });
  }
});

async function workspace(): Promise<string> {
  await mkdir(TEST_ROOT, { recursive: true });
  const root = await mkdtemp(path.join(TEST_ROOT, 'manager-'));
  tempDirs.push(root);
  return root;
}

async function writeCustomConfig(
  root: string,
  args: string[] = ['server.mjs'],
  fixtureName = 'manager-test'
): Promise<string> {
  const configPath = path.join(root, 'lsp-servers.json');
  await writeFile(
    configPath,
    JSON.stringify({
      languageServers: {
        '.foo': {
          command: process.execPath,
          args,
          languageId: 'foo',
          initializationOptions: { fixtureName },
        },
        '.bad': {
          command: 'sh',
          args: ['-c', 'echo unsafe'],
          languageId: 'bad',
        },
      },
    })
  );
  process.env.OCTOCODE_LSP_CONFIG = configPath;
  return configPath;
}

function resolveClientReady(client: LSPClient): void {
  (client as unknown as { resolveReady(): void }).resolveReady();
}

describe('LSP manager', () => {
  it('reports generic status without a file path', async () => {
    await expect(getLspStatus()).resolves.toMatchObject({
      enabled: true,
      pooledClientCount: 0,
      pooledClients: [],
      hints: [
        'Provide filePath to check language server availability for a specific file.',
      ],
    });
    expect(pooledClientCount()).toBe(0);
  });

  it('loads safe custom local servers and rejects shell commands', async () => {
    const root = await workspace();
    await writeCustomConfig(root);
    const filePath = path.join(root, 'demo.foo');
    await writeFile(filePath, 'define Foo\n');

    await expect(loadUserConfig(root)).resolves.toEqual({
      '.foo': {
        command: process.execPath,
        args: ['server.mjs'],
        languageId: 'foo',
        initializationOptions: { fixtureName: 'manager-test' },
      },
    });
    await expect(getLanguageServerForFile(filePath, root)).resolves.toEqual({
      command: process.execPath,
      args: ['server.mjs'],
      workspaceRoot: root,
      languageId: 'foo',
      initializationOptions: { fixtureName: 'manager-test' },
    });
    await expect(isLanguageServerAvailable(filePath, root)).resolves.toBe(true);
    await expect(releasePooledClientForFile(root, filePath)).resolves.toBe(
      true
    );
  });

  it('acquires pooled custom local servers using the original extension', async () => {
    const root = await workspace();
    await writeCustomConfig(root, [CUSTOM_SERVER], 'pooled-custom');
    const filePath = path.join(root, 'demo.foo');
    await writeFile(filePath, 'define FooSymbol\n');

    const client = await acquirePooledClient(root, filePath);
    if (client === null) {
      throw new Error('Expected pooled custom LSP client');
    }
    resolveClientReady(client);

    await expect(client.documentSymbols(filePath)).resolves.toMatchObject([
      {
        name: 'FooSymbol',
        detail: 'pooled-custom',
      },
    ]);
    await expect(
      getLspStatus({ filePath, workspaceRoot: root })
    ).resolves.toMatchObject({
      pooledClientCount: 1,
      pooledClients: [
        {
          workspaceRoot: root,
          languageId: 'foo',
          extension: '.foo',
        },
      ],
      serverAvailable: true,
      hints: ['Language server appears available for this file.'],
    });
  });

  it('keeps failed pooled server starts out of the shared pool', async () => {
    const root = await workspace();
    const serverPath = path.join(root, 'fail.mjs');
    await writeFile(
      serverPath,
      'process.stderr.write("pooled boom\\n"); process.exit(4);\n'
    );
    await writeCustomConfig(root, [serverPath], 'pooled-failure');
    const filePath = path.join(root, 'demo.foo');
    await writeFile(filePath, 'define FooSymbol\n');

    await expect(acquirePooledClient(root, filePath)).resolves.toBeNull();
    expect(pooledClientCount()).toBe(0);
  });

  it('returns unavailable hints for unknown languages', async () => {
    const root = await workspace();
    await writeFile(path.join(root, 'package.json'), '{}');
    const filePath = path.join(root, 'demo.unknown');
    await writeFile(filePath, 'plain\n');

    expect(detectLanguageId(filePath)).toBe('plaintext');
    await expect(getLanguageServerForFile(filePath, root)).resolves.toBeNull();
    await expect(isLanguageServerAvailable(filePath, root)).resolves.toBe(
      false
    );
    await expect(acquirePooledClient(root, filePath)).resolves.toBeNull();
    await expect(releasePooledClientForFile(root, filePath)).resolves.toBe(
      false
    );
    await expect(
      getLspStatus({ filePath, workspaceRoot: root })
    ).resolves.toMatchObject({
      enabled: true,
      filePath,
      workspaceRoot: root,
      serverAvailable: false,
    });
    await expect(getLspStatus({ filePath })).resolves.toMatchObject({
      enabled: true,
      filePath,
      workspaceRoot: root,
    });
  });

  it('checks absolute and PATH-based default server availability', async () => {
    const root = await workspace();
    const filePath = path.join(root, 'main.go');
    await writeFile(filePath, 'package main\n');

    process.env.OCTOCODE_GO_SERVER_PATH = '/bin/sh';
    await expect(isLanguageServerAvailable(filePath, root)).resolves.toBe(true);

    process.env.OCTOCODE_GO_SERVER_PATH = path.join(root, 'missing-gopls');
    await expect(isLanguageServerAvailable(filePath, root)).resolves.toBe(
      false
    );

    delete process.env.OCTOCODE_GO_SERVER_PATH;
    await expect(isLanguageServerAvailable(filePath, root)).resolves.toEqual(
      expect.any(Boolean)
    );
    await expect(releasePooledClientForFile(root, filePath)).resolves.toBe(
      true
    );
  });

  it('rejects broken rust-analyzer shims during availability checks', async () => {
    const root = await workspace();
    const serverPath = path.join(root, 'rust-analyzer');
    const filePath = path.join(root, 'main.rs');
    await writeFile(
      serverPath,
      '#!/bin/sh\necho "missing component" >&2\nexit 1\n'
    );
    await chmod(serverPath, 0o755);
    await writeFile(filePath, 'fn main() {}\n');

    process.env.OCTOCODE_RUST_SERVER_PATH = serverPath;

    await expect(isLanguageServerAvailable(filePath, root)).resolves.toBe(
      false
    );
  });
});

function restoreEnv(key: string, value: string | undefined): void {
  if (value === undefined) {
    delete process.env[key];
  } else {
    process.env[key] = value;
  }
}
