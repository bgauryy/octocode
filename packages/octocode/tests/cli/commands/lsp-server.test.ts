import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { EXIT } from '../../../src/cli/exit-codes.js';

const mocks = vi.hoisted(() => ({
  rmSync: vi.fn(),
  getLspStatus: vi.fn(),
  isCommandOnPath: vi.fn().mockReturnValue(false),
  listManifestServers: vi.fn(),
  manifestServer: vi.fn(),
  manifestInstallHint: vi.fn(),
  managedCacheRoot: vi.fn().mockReturnValue('/cache/lsp'),
  provisionMode: vi.fn().mockReturnValue('off'),
  resolveCachedServer: vi.fn(),
  provisionServer: vi.fn(),
  uninstallServer: vi.fn(),
  discoverServer: vi.fn(),
  discoverServerBatch: vi.fn(),
}));

vi.mock('node:fs', async importOriginal => ({
  ...(await importOriginal<typeof import('node:fs')>()),
  rmSync: mocks.rmSync,
}));

vi.mock('@octocodeai/octocode-engine/lsp/manager', () => ({
  getLspStatus: mocks.getLspStatus,
  TOOLCHAIN_SERVERS: [
    { server: 'rust-analyzer', languageId: 'rust', hint: 'Install Rust.' },
  ],
}));

vi.mock('@octocodeai/octocode-engine/lsp/config', () => ({
  BUNDLED_SERVER_NAMES: ['typescript-language-server'],
  isCommandOnPath: mocks.isCommandOnPath,
}));

vi.mock('@octocodeai/octocode-engine/lsp/serverManifest', () => ({
  listManifestServers: mocks.listManifestServers,
  manifestServer: mocks.manifestServer,
  manifestInstallHint: mocks.manifestInstallHint,
  managedCacheRoot: mocks.managedCacheRoot,
  provisionMode: mocks.provisionMode,
  resolveCachedServer: mocks.resolveCachedServer,
}));

vi.mock('@octocodeai/octocode-engine/lsp/serverProvisioner', () => ({
  provisionServer: mocks.provisionServer,
  uninstallServer: mocks.uninstallServer,
}));

vi.mock('@octocodeai/octocode-engine/lsp/serverDiscovery', () => ({
  discoverServer: mocks.discoverServer,
  discoverServerBatch: mocks.discoverServerBatch,
}));

vi.mock('../../../src/utils/colors.js', () => ({
  c: (_tag: string, text: string) => text,
  bold: (text: string) => text,
  dim: (text: string) => text,
}));

describe('lsp-server command', () => {
  let logSpy: ReturnType<typeof vi.spyOn>;
  let originalExitCode: typeof process.exitCode;

  beforeEach(() => {
    vi.clearAllMocks();
    process.exitCode = undefined;
    originalExitCode = process.exitCode;
    logSpy = vi.spyOn(console, 'log').mockImplementation(() => {});
    mocks.listManifestServers.mockReturnValue([
      { name: 'clangd', languageId: 'cpp', releaseTag: 'v1' },
    ]);
    mocks.discoverServerBatch.mockReturnValue({});
    mocks.manifestServer.mockImplementation((name: string) =>
      name === 'clangd' ? { name: 'clangd' } : undefined
    );
    mocks.getLspStatus.mockResolvedValue({
      pooledClientCount: 0,
      serverAvailable: false,
      languageId: 'plaintext',
      hints: [],
    });
  });

  afterEach(() => {
    logSpy.mockRestore();
    process.exitCode = originalExitCode;
  });

  async function run(
    args: string[],
    options: Record<string, string | boolean> = {}
  ) {
    const { lspServerCommand } =
      await import('../../../src/cli/commands/lsp-server.js');
    await lspServerCommand.handler({
      command: 'lsp-server',
      args,
      options,
    });
  }

  function json(): Record<string, unknown> {
    return JSON.parse(String(logSpy.mock.calls.at(-1)?.[0]));
  }

  it('lists manifest and toolchain servers as JSON', async () => {
    mocks.resolveCachedServer.mockReturnValue('/cache/clangd');
    await run(['list'], { json: true });
    expect(json()).toMatchObject({
      autoDownload: [{ name: 'clangd', status: 'installed (managed cache)' }],
      toolchain: [{ name: 'rust-analyzer', status: 'not installed' }],
    });
    expect(process.exitCode).toBeUndefined();
  });

  it('reports file resolution status as JSON', async () => {
    mocks.getLspStatus.mockResolvedValue({
      pooledClientCount: 1,
      serverAvailable: true,
      languageId: 'typescript',
      serverSource: 'bundled',
      hints: [],
    });
    await run(['status', 'src/index.ts'], { json: true });
    expect(mocks.getLspStatus).toHaveBeenCalledWith({
      filePath: 'src/index.ts',
    });
    expect(json()).toMatchObject({
      serverAvailable: true,
      languageId: 'typescript',
    });
  });

  it('rejects install without a target', async () => {
    await run(['install']);
    expect(process.exitCode).toBe(EXIT.USAGE);
    expect(mocks.provisionServer).not.toHaveBeenCalled();
  });

  it('instructs for toolchain servers instead of pretending to install', async () => {
    await run(['install', 'rust-analyzer'], { json: true });
    expect(json()).toMatchObject({
      install: [{ name: 'rust-analyzer', action: 'instruct' }],
    });
    expect(process.exitCode).toBe(EXIT.NOT_FOUND);
  });

  it('reports unknown servers', async () => {
    mocks.manifestInstallHint.mockReturnValue('No manifest entry.');
    await run(['install', 'mystery'], { json: true });
    expect(json()).toMatchObject({
      install: [{ name: 'mystery', action: 'unknown' }],
    });
    expect(process.exitCode).toBe(EXIT.NOT_FOUND);
  });

  it('skips an already available server unless forced', async () => {
    mocks.resolveCachedServer.mockReturnValue('/cache/clangd');
    await run(['install', 'clangd'], { json: true });
    expect(json()).toMatchObject({
      install: [{ name: 'clangd', action: 'skipped', path: '/cache/clangd' }],
    });
    expect(mocks.provisionServer).not.toHaveBeenCalled();
  });

  it('provisions forced installs and surfaces failures', async () => {
    mocks.provisionServer
      .mockResolvedValueOnce({
        ok: true,
        source: 'downloaded',
        path: '/cache/clangd',
      })
      .mockResolvedValueOnce({ ok: false, error: 'download failed' });
    await run(['install', 'clangd'], { force: true, json: true });
    expect(mocks.provisionServer).toHaveBeenCalledWith('clangd', {
      mode: 'auto',
    });
    expect(process.exitCode).toBeUndefined();

    await run(['install', 'clangd'], { force: true, json: true });
    expect(json()).toMatchObject({ install: [{ action: 'error' }] });
    expect(process.exitCode).toBe(EXIT.NOT_FOUND);
  });

  it('uninstalls named managed servers', async () => {
    mocks.uninstallServer.mockReturnValue(true);
    await run(['uninstall', 'clangd'], { json: true });
    expect(mocks.uninstallServer).toHaveBeenCalledWith('clangd');
    expect(json()).toEqual({ uninstall: [{ name: 'clangd', removed: true }] });
  });

  it('keeps clean as a dry-run until --yes', async () => {
    await run(['clean'], { json: true });
    expect(json()).toEqual({ clean: 'dry-run', root: '/cache/lsp' });
    expect(mocks.rmSync).not.toHaveBeenCalled();

    await run(['clean'], { yes: true, json: true });
    expect(mocks.rmSync).toHaveBeenCalledWith('/cache/lsp', {
      recursive: true,
      force: true,
    });
    expect(json()).toEqual({ clean: 'done', root: '/cache/lsp' });
  });

  it('rejects unknown subcommands', async () => {
    await run(['wat']);
    expect(process.exitCode).toBe(EXIT.USAGE);
  });
});
