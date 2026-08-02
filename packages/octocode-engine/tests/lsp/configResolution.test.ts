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

describe('pre-native fallback (language absent from the native spec table)', () => {
  it('includes workspaceRoot in the bundled shellscript config', async () => {
    // Shellscript has no entry in the native spec table, so
    // getLanguageServerForFile returns undefined and resolution falls to
    // BUNDLED_BY_LANGUAGE. Regression test: this branch used to construct the
    // config object without `workspaceRoot` — a required field on
    // LanguageServerConfig (and on the Rust-side JsLanguageServerConfig,
    // which rejects a missing field at napi deserialization) — so every
    // lspGetSemantics call on a .sh file failed with
    // "Missing field `workspaceRoot`" even when the caller passed one in.
    vi.resetModules();
    vi.doMock('../../src/lsp/native.js', () => ({
      nativeBinding: {
        getLanguageServerForFile: vi.fn(() => undefined),
        detectLanguageId: vi.fn(() => 'shellscript'),
        isCommandAvailable: vi.fn(() => false),
      },
    }));
    try {
      const { resolveServerForFile } = await import('../../src/lsp/config.js');
      const resolution = await resolveServerForFile('/repo/a.sh', '/repo');
      expect(resolution?.source).toBe('bundled');
      expect(resolution?.config.workspaceRoot).toBe('/repo');
    } finally {
      vi.doUnmock('../../src/lsp/native.js');
      vi.resetModules();
    }
  });
});
