import { mkdtemp, rm, writeFile } from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import { afterEach, describe, expect, it } from 'vitest';
import {
  detectLanguageId,
  getLanguageServerForFile,
  loadUserConfig,
  resolveLanguageServer,
  resolveTypeScriptLspProvider,
} from '../src/config.js';
import { buildInitializationOptions } from '../src/initParams.js';

const TS_SERVER_CONFIG = {
  command: 'typescript-language-server',
  args: ['--stdio'],
  envVar: 'OCTOCODE_TS_SERVER_PATH',
};

const ORIGINAL_PROVIDER = process.env.OCTOCODE_TS_LSP_PROVIDER;
const ORIGINAL_SERVER_PATH = process.env.OCTOCODE_TS_SERVER_PATH;
const ORIGINAL_CUSTOM_SERVER_PATH = process.env.OCTOCODE_CUSTOM_SERVER_PATH;
const ORIGINAL_LSP_CONFIG = process.env.OCTOCODE_LSP_CONFIG;
const tempDirs: string[] = [];

afterEach(async () => {
  restoreEnv('OCTOCODE_TS_LSP_PROVIDER', ORIGINAL_PROVIDER);
  restoreEnv('OCTOCODE_TS_SERVER_PATH', ORIGINAL_SERVER_PATH);
  restoreEnv('OCTOCODE_CUSTOM_SERVER_PATH', ORIGINAL_CUSTOM_SERVER_PATH);
  restoreEnv('OCTOCODE_LSP_CONFIG', ORIGINAL_LSP_CONFIG);
  while (tempDirs.length > 0) {
    await rm(tempDirs.pop()!, { recursive: true, force: true });
  }
});

async function tempConfig(content: unknown): Promise<string> {
  const dir = await mkdtemp(path.join(os.tmpdir(), 'octocode-lsp-config-'));
  tempDirs.push(dir);
  const filePath = path.join(dir, 'lsp-servers.json');
  await writeFile(filePath, JSON.stringify(content));
  return filePath;
}

describe('generic LSP server resolution', () => {
  it('uses non-TypeScript env overrides and falls back from bad packages', () => {
    process.env.OCTOCODE_CUSTOM_SERVER_PATH = process.execPath;

    expect(
      resolveLanguageServer({
        command: 'custom-lsp',
        args: ['--stdio'],
        envVar: 'OCTOCODE_CUSTOM_SERVER_PATH',
      })
    ).toEqual({ command: process.execPath, args: ['--stdio'] });

    delete process.env.OCTOCODE_CUSTOM_SERVER_PATH;

    expect(
      resolveLanguageServer({
        command: 'custom-lsp',
        args: ['--stdio'],
        envVar: 'OCTOCODE_CUSTOM_SERVER_PATH',
      })
    ).toEqual({ command: 'custom-lsp', args: ['--stdio'] });
    expect(
      resolveLanguageServer({
        command: 'custom-lsp',
        args: [],
        envVar: 'OCTOCODE_CUSTOM_SERVER_PATH',
        packageName: 'typescript',
        binName: 'missing-bin',
      })
    ).toEqual({ command: 'custom-lsp', args: [] });
    expect(
      resolveLanguageServer({
        command: 'custom-lsp',
        args: [],
        envVar: 'OCTOCODE_CUSTOM_SERVER_PATH',
        packageName: 'definitely-missing-lsp-package',
        binName: 'missing',
      })
    ).toEqual({ command: 'custom-lsp', args: [] });
    expect(
      resolveLanguageServer({
        command: 'custom-lsp',
        args: ['--stdio'],
        envVar: 'OCTOCODE_CUSTOM_SERVER_PATH',
        packageName: 'typescript',
        binName: 'tsc',
      }).command
    ).toBe(process.execPath);
  });

  it('detects registered language ids and resolves default servers', async () => {
    delete process.env.OCTOCODE_LSP_CONFIG;

    expect(detectLanguageId('demo.mts')).toBe('typescript');
    expect(detectLanguageId('demo.cjs')).toBe('javascript');
    expect(detectLanguageId('demo.unknown')).toBe('plaintext');
    await expect(
      getLanguageServerForFile('demo.ts', process.cwd())
    ).resolves.toMatchObject({
      languageId: 'typescript',
      workspaceRoot: process.cwd(),
    });
  });

  it('loads optional config paths and user servers without args', async () => {
    delete process.env.OCTOCODE_LSP_CONFIG;
    await expect(loadUserConfig()).resolves.toEqual({});

    process.env.OCTOCODE_LSP_CONFIG = await tempConfig({
      languageServers: { foo: { command: 'bad', languageId: 'foo' } },
    });
    await expect(loadUserConfig()).resolves.toEqual({});

    process.env.OCTOCODE_LSP_CONFIG = await tempConfig({});
    await expect(loadUserConfig()).resolves.toEqual({});

    process.env.OCTOCODE_LSP_CONFIG = await tempConfig({
      languageServers: {
        '.foo': {
          command: process.execPath,
          languageId: 'foo',
        },
      },
    });
    await expect(
      getLanguageServerForFile('demo.foo', process.cwd())
    ).resolves.toMatchObject({
      command: process.execPath,
      args: [],
      languageId: 'foo',
    });
  });
});

describe('TypeScript LSP provider selection', () => {
  it('uses tsgo as the speed-first default', () => {
    delete process.env.OCTOCODE_TS_LSP_PROVIDER;
    delete process.env.OCTOCODE_TS_SERVER_PATH;

    expect(resolveTypeScriptLspProvider()).toBe('tsgo');
  });

  it('falls back to the speed-first default for unknown provider values', () => {
    process.env.OCTOCODE_TS_LSP_PROVIDER = 'fast-but-imaginary';
    delete process.env.OCTOCODE_TS_SERVER_PATH;

    expect(resolveTypeScriptLspProvider()).toBe('tsgo');
  });

  it('resolves tsgo to its stdio LSP subcommand', () => {
    process.env.OCTOCODE_TS_LSP_PROVIDER = 'tsgo';
    delete process.env.OCTOCODE_TS_SERVER_PATH;

    expect(resolveLanguageServer(TS_SERVER_CONFIG).args.slice(-2)).toEqual([
      '--lsp',
      '--stdio',
    ]);
  });

  it('resolves vtsls to stdio mode', () => {
    process.env.OCTOCODE_TS_LSP_PROVIDER = 'vtsls';
    delete process.env.OCTOCODE_TS_SERVER_PATH;

    expect(resolveLanguageServer(TS_SERVER_CONFIG).args.slice(-1)).toEqual([
      '--stdio',
    ]);
  });

  it('uses default tsgo args with OCTOCODE_TS_SERVER_PATH', () => {
    delete process.env.OCTOCODE_TS_LSP_PROVIDER;
    process.env.OCTOCODE_TS_SERVER_PATH = '/custom/tsgo';

    expect(resolveLanguageServer(TS_SERVER_CONFIG)).toEqual({
      command: '/custom/tsgo',
      args: ['--lsp', '--stdio'],
    });
  });

  it('uses selected TypeScript provider args with OCTOCODE_TS_SERVER_PATH', () => {
    process.env.OCTOCODE_TS_LSP_PROVIDER = 'typescript-language-server';
    process.env.OCTOCODE_TS_SERVER_PATH = '/custom/typescript-language-server';

    expect(resolveLanguageServer(TS_SERVER_CONFIG)).toEqual({
      command: '/custom/typescript-language-server',
      args: ['--stdio'],
    });
  });

  it('preserves explicit tsgo args with OCTOCODE_TS_SERVER_PATH', () => {
    process.env.OCTOCODE_TS_LSP_PROVIDER = 'tsgo';
    process.env.OCTOCODE_TS_SERVER_PATH = '/custom/tsgo';

    expect(resolveLanguageServer(TS_SERVER_CONFIG)).toEqual({
      command: '/custom/tsgo',
      args: ['--lsp', '--stdio'],
    });
  });
});

describe('LSP initialization options', () => {
  it('forwards custom initialization options for user-provided servers', () => {
    expect(
      buildInitializationOptions({
        languageId: 'foo',
        initializationOptions: {
          analyzer: { mode: 'strict' },
          customFeature: true,
        },
      })
    ).toEqual({
      analyzer: { mode: 'strict' },
      customFeature: true,
    });
  });

  it('merges TypeScript defaults with custom initialization options', () => {
    const options = buildInitializationOptions({
      languageId: 'typescript',
      initializationOptions: {
        preferences: {
          includePackageJsonAutoImports: 'on',
        },
        customFeature: true,
      },
    });

    expect(options).toMatchObject({
      tsserver: expect.any(Object),
      preferences: {
        includePackageJsonAutoImports: 'on',
      },
      customFeature: true,
    });
  });
});

function restoreEnv(key: string, value: string | undefined): void {
  if (value === undefined) {
    delete process.env[key];
  } else {
    process.env[key] = value;
  }
}
