import { describe, expect, it, vi } from 'vitest';

/**
 * Exercises the resolution ladder in config.ts when the native default command
 * is NOT available on the machine — the production case. Verifies the bundled
 * JS-server layer kicks in (command-keyed for YAML, language-keyed for Python).
 */
async function withUnavailableNative(
  spec: { command: string; args: string[]; languageId: string },
  run: (mod: typeof import('../../src/lsp/config.js')) => Promise<void>
): Promise<void> {
  vi.resetModules();
  vi.doMock('../../src/lsp/native.js', () => ({
    nativeBinding: {
      getLanguageServerForFile: vi.fn((_f: string, workspaceRoot: string) => ({
        ...spec,
        workspaceRoot,
      })),
      isCommandAvailable: vi.fn(() => false),
    },
  }));
  try {
    await run(await import('../../src/lsp/config.js'));
  } finally {
    vi.doUnmock('../../src/lsp/native.js');
    vi.resetModules();
  }
}

describe('config resolution ladder (server unavailable on PATH)', () => {
  it('falls back to the bundled YAML server (command-keyed)', async () => {
    await withUnavailableNative(
      { command: 'yaml-language-server', args: ['--stdio'], languageId: 'yaml' },
      async ({ resolveServerForFile }) => {
        const resolution = await resolveServerForFile('/repo/a.yaml', '/repo');
        expect(resolution?.source).toBe('bundled');
        expect(resolution?.config.command).toBe(process.execPath);
        expect(resolution?.config.args?.[0]).toMatch(
          /yaml-language-server[/\\]bin[/\\]yaml-language-server$/
        );
        expect(resolution?.config.args?.slice(1)).toEqual(['--stdio']);
      }
    );
  });

  it('falls back to bundled pyright for Python even though the native default is pylsp (language-keyed)', async () => {
    await withUnavailableNative(
      { command: 'pylsp', args: [], languageId: 'python' },
      async ({ resolveServerForFile }) => {
        const resolution = await resolveServerForFile('/repo/a.py', '/repo');
        expect(resolution?.source).toBe('bundled');
        expect(resolution?.config.command).toBe(process.execPath);
        expect(resolution?.config.args?.[0]).toMatch(
          /pyright[/\\]langserver\.index\.js$/
        );
        expect(resolution?.config.args?.slice(1)).toEqual(['--stdio']);
      }
    );
  });

  it('reports unavailable when no server can be resolved', async () => {
    await withUnavailableNative(
      { command: 'no-such-server-zzz', args: [], languageId: 'madeuplang' },
      async ({ resolveServerForFile }) => {
        const resolution = await resolveServerForFile('/repo/a.zzz', '/repo');
        expect(resolution?.source).toBe('unavailable');
        expect(resolution?.config.command).toBe('no-such-server-zzz');
      }
    );
  });
});
