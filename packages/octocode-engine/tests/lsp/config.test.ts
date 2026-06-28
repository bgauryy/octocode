import { describe, expect, it, vi } from 'vitest';

type ConfigModule = typeof import('../../src/lsp/config.js');

describe('native config wrappers', () => {
  it('detects language ids and default native server configs', async () => {
    const { detectLanguageId, getLanguageServerForFile } = await import(
      '../../src/lsp/config.js'
    );

    expect(detectLanguageId('demo.ts')).toBe('typescript');
    expect(detectLanguageId('demo.unknown')).toBe('plaintext');
    await expect(
      getLanguageServerForFile('demo.ts', process.cwd())
    ).resolves.toMatchObject({ languageId: 'typescript' });
  });

  it('uses the package-local TypeScript server when a remote workspace cannot resolve the bare command', async () => {
    await withMockedNative(async ({ getLanguageServerForFile }, nativeBinding) => {
      const config = await getLanguageServerForFile(
        '/tmp/octocode-remote/repo/demo.ts',
        '/tmp/octocode-remote/repo'
      );

      expect(config).toMatchObject({
        command: process.execPath,
        languageId: 'typescript',
        workspaceRoot: '/tmp/octocode-remote/repo',
      });
      expect(config?.args?.[0]).toMatch(
        /typescript-language-server[/\\]lib[/\\]cli\.mjs$/
      );
      expect(config?.args?.slice(1)).toEqual(['--stdio']);
      expect(nativeBinding.isCommandAvailable).toHaveBeenCalledWith(
        'typescript-language-server'
      );
    });
  });

  it('preserves PATH-resolved TypeScript server commands', async () => {
    await withMockedNative(
      async ({ getLanguageServerForFile }) => {
        await expect(
          getLanguageServerForFile('/workspace/demo.ts', '/workspace')
        ).resolves.toMatchObject({
          command: 'typescript-language-server',
          args: ['--stdio'],
        });
      },
      { commandAvailable: true }
    );
  });
});

async function withMockedNative(
  run: (
    configModule: Pick<ConfigModule, 'getLanguageServerForFile'>,
    nativeBinding: ReturnType<typeof nativeBindingMock>
  ) => Promise<void>,
  options: { commandAvailable?: boolean } = {}
): Promise<void> {
  vi.resetModules();
  const nativeBinding = nativeBindingMock(options.commandAvailable ?? false);
  vi.doMock('../../src/lsp/native.js', () => ({ nativeBinding }));
  try {
    const configModule = await import('../../src/lsp/config.js');
    await run(configModule, nativeBinding);
  } finally {
    vi.doUnmock('../../src/lsp/native.js');
    vi.resetModules();
  }
}

function nativeBindingMock(commandAvailable: boolean) {
  return {
    detectLanguageId: vi.fn((filePath: string) =>
      filePath.endsWith('.ts') ? 'typescript' : undefined
    ),
    getLanguageServerForFile: vi.fn(
      (filePath: string, workspaceRoot: string) => {
        if (!filePath.endsWith('.ts')) return undefined;
        return {
          command: 'typescript-language-server',
          args: ['--stdio'],
          workspaceRoot,
          languageId: 'typescript',
        };
      }
    ),
    isCommandAvailable: vi.fn(() => commandAvailable),
  };
}
