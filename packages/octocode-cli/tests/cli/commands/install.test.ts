

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';

vi.mock('node:fs', () => ({
  existsSync: vi.fn().mockReturnValue(false),
  readFileSync: vi.fn(),
  writeFileSync: vi.fn(),
  mkdirSync: vi.fn(),
  unlinkSync: vi.fn(),
  rmSync: vi.fn(),
  statSync: vi.fn(),
  symlinkSync: vi.fn(),
  promises: {
    readFile: vi.fn(),
    writeFile: vi.fn(),
    mkdir: vi.fn(),
    unlink: vi.fn(),
    stat: vi.fn(),
  },
}));

vi.mock('node:crypto', () => ({
  randomBytes: vi.fn().mockReturnValue(Buffer.alloc(32)),
  createCipheriv: vi.fn().mockReturnValue({
    update: vi.fn().mockReturnValue('encrypted'),
    final: vi.fn().mockReturnValue(''),
    getAuthTag: vi.fn().mockReturnValue(Buffer.alloc(16)),
  }),
  createDecipheriv: vi.fn().mockReturnValue({
    update: vi.fn().mockReturnValue('{}'),
    final: vi.fn().mockReturnValue(''),
    setAuthTag: vi.fn(),
  }),
}));

vi.mock('../../../src/features/install.js', () => ({
  installOctocodeForClient: vi.fn(),
  getInstallPreviewForClient: vi.fn(),
}));

vi.mock('../../../src/features/node-check.js', () => ({
  checkNodeInPath: vi.fn().mockReturnValue({ installed: true }),
  checkNpmInPath: vi.fn().mockReturnValue({ installed: true }),
}));

vi.mock('../../../src/interactive.js', () => ({
  runInteractiveMode: vi.fn(),
}));

vi.mock('../../../src/utils/spinner.js', () => ({
  Spinner: vi.fn(function SpinnerMock(this: unknown) {
    return {
      start: vi.fn().mockReturnThis(),
      stop: vi.fn(),
      succeed: vi.fn(),
      fail: vi.fn(),
    };
  }),
}));

vi.mock('../../../src/ui/constants.js', () => ({
  IDE_INFO: { cursor: { name: 'Cursor' } },
  CLIENT_INFO: {
    cursor: { name: 'Cursor' },
    codex: { name: 'Codex' },
  },
  INSTALL_METHOD_INFO: {
    npx: { name: 'npx' },
    direct: { name: 'Direct' },
  },
}));

describe('cli/commands/install', () => {
  let consoleSpy: ReturnType<typeof vi.spyOn>;
  let originalExitCode: typeof process.exitCode;

  beforeEach(() => {
    vi.resetModules();
    vi.clearAllMocks();
    consoleSpy = vi.spyOn(console, 'log').mockImplementation(() => {});
    originalExitCode = process.exitCode;
    process.exitCode = undefined;
  });

  afterEach(() => {
    consoleSpy.mockRestore();
    process.exitCode = originalExitCode;
  });

  async function loadDeps() {
    const { installOctocodeForClient, getInstallPreviewForClient } =
      await import('../../../src/features/install.js');
    const { runInteractiveMode } = await import('../../../src/interactive.js');
    const { checkNodeInPath, checkNpmInPath } =
      await import('../../../src/features/node-check.js');
    const { Spinner } = await import('../../../src/utils/spinner.js');
    const { installCommand } =
      await import('../../../src/cli/commands/install.js');
    return {
      runInteractiveMode,
      checkNodeInPath,
      checkNpmInPath,
      Spinner,
      installCommand,
      installOctocodeForClient,
      getInstallPreviewForClient,
    };
  }

  const basePreview = {
    client: 'cursor' as const,
    method: 'npx' as const,
    configPath: '/mock/mcp.json',
    serverConfig: {},
    action: 'create' as const,
  };

  it('calls runInteractiveMode when no IDE is provided', async () => {
    const { installCommand, runInteractiveMode } = await loadDeps();
    await installCommand.handler!({
      command: 'install',
      args: [],
      options: {},
    });
    expect(runInteractiveMode).toHaveBeenCalledTimes(1);
    expect(process.exitCode).toBeUndefined();
  });

  it('errors when Node is not in PATH for npx method', async () => {
    const { installCommand, checkNodeInPath } = await loadDeps();
    vi.mocked(checkNodeInPath).mockReturnValueOnce({
      installed: false,
      version: null,
    });

    await installCommand.handler!({
      command: 'install',
      args: [],
      options: { ide: 'cursor', method: 'npx' },
    });

    expect(consoleSpy).toHaveBeenCalledWith(
      expect.stringContaining('not found in PATH')
    );
    expect(process.exitCode).toBe(1);
  });

  it('errors when npm is not in PATH for npx method', async () => {
    const { installCommand, checkNpmInPath } = await loadDeps();
    vi.mocked(checkNpmInPath).mockReturnValueOnce({
      installed: false,
      version: null,
    });

    await installCommand.handler!({
      command: 'install',
      args: [],
      options: { ide: 'cursor', method: 'npx' },
    });

    expect(consoleSpy).toHaveBeenCalledWith(expect.stringContaining('npm is'));
    expect(process.exitCode).toBe(1);
  });

  it('errors on invalid IDE', async () => {
    const { installCommand } = await loadDeps();

    await installCommand.handler!({
      command: 'install',
      args: [],
      options: { ide: 'not-a-real-ide', method: 'npx' },
    });

    expect(consoleSpy).toHaveBeenCalledWith(
      expect.stringContaining('Invalid IDE')
    );
    expect(process.exitCode).toBe(1);
  });

  it('errors on invalid method', async () => {
    const { installCommand } = await loadDeps();

    await installCommand.handler!({
      command: 'install',
      args: [],
      options: { ide: 'cursor', method: 'bogus' },
    });

    expect(consoleSpy).toHaveBeenCalledWith(
      expect.stringContaining('Invalid method')
    );
    expect(process.exitCode).toBe(1);
  });

  it('errors when already configured without --force', async () => {
    const { installCommand, getInstallPreviewForClient } = await loadDeps();
    vi.mocked(getInstallPreviewForClient).mockReturnValue({
      ...basePreview,
      action: 'override',
    });

    await installCommand.handler!({
      command: 'install',
      args: [],
      options: { ide: 'cursor', method: 'npx' },
    });

    expect(consoleSpy).toHaveBeenCalledWith(
      expect.stringContaining('already configured')
    );
    expect(process.exitCode).toBe(1);
  });

  it('runs successful install with spinner success path', async () => {
    const {
      installCommand,
      installOctocodeForClient,
      getInstallPreviewForClient,
      Spinner,
    } = await loadDeps();

    vi.mocked(getInstallPreviewForClient).mockReturnValue({
      ...basePreview,
      action: 'create',
    });
    vi.mocked(installOctocodeForClient).mockReturnValue({
      success: true,
      configPath: '/mock/mcp.json',
    });

    await installCommand.handler!({
      command: 'install',
      args: [],
      options: { ide: 'cursor', method: 'npx' },
    });

    const spinnerInst = vi.mocked(Spinner).mock.results[0]?.value as {
      succeed: ReturnType<typeof vi.fn>;
    };
    expect(spinnerInst?.succeed).toHaveBeenCalledWith('Installation complete!');
    expect(consoleSpy).toHaveBeenCalledWith(
      expect.stringContaining('Config saved')
    );
    expect(installOctocodeForClient).toHaveBeenCalledWith({
      client: 'cursor',
      method: 'npx',
      force: false,
    });
    expect(process.exitCode).toBeUndefined();
  });

  it('handles install failure', async () => {
    const {
      installCommand,
      installOctocodeForClient,
      getInstallPreviewForClient,
      Spinner,
    } = await loadDeps();

    vi.mocked(getInstallPreviewForClient).mockReturnValue({
      ...basePreview,
      action: 'create',
    });
    vi.mocked(installOctocodeForClient).mockReturnValue({
      success: false,
      configPath: '/mock/mcp.json',
      error: 'disk full',
    });

    await installCommand.handler!({
      command: 'install',
      args: [],
      options: { ide: 'cursor', method: 'npx' },
    });

    const spinnerInst = vi.mocked(Spinner).mock.results[0]?.value as {
      fail: ReturnType<typeof vi.fn>;
    };
    expect(spinnerInst?.fail).toHaveBeenCalledWith('Installation failed');
    expect(consoleSpy).toHaveBeenCalledWith(
      expect.stringContaining('disk full')
    );
    expect(process.exitCode).toBe(1);
  });

  it('handles install failure without an error message', async () => {
    const {
      installCommand,
      installOctocodeForClient,
      getInstallPreviewForClient,
      Spinner,
    } = await loadDeps();

    vi.mocked(getInstallPreviewForClient).mockReturnValue({
      ...basePreview,
      action: 'create',
    });
    vi.mocked(installOctocodeForClient).mockReturnValue({
      success: false,
      configPath: '/mock/mcp.json',
    });

    await installCommand.handler!({
      command: 'install',
      args: [],
      options: { ide: 'cursor', method: 'npx' },
    });

    const spinnerInst = vi.mocked(Spinner).mock.results[0]?.value as {
      fail: ReturnType<typeof vi.fn>;
    };
    expect(spinnerInst?.fail).toHaveBeenCalledWith('Installation failed');
    expect(process.exitCode).toBe(1);
  });

  it('prints backup path when install succeeds with backup', async () => {
    const {
      installCommand,
      installOctocodeForClient,
      getInstallPreviewForClient,
    } = await loadDeps();

    vi.mocked(getInstallPreviewForClient).mockReturnValue({
      ...basePreview,
      action: 'override',
    });
    vi.mocked(installOctocodeForClient).mockReturnValue({
      success: true,
      configPath: '/mock/mcp.json',
      backupPath: '/mock/mcp.json.bak',
    });

    await installCommand.handler!({
      command: 'install',
      args: [],
      options: {
        ide: 'cursor',
        method: 'npx',
        force: true,
      },
    });

    expect(consoleSpy).toHaveBeenCalledWith(
      expect.stringContaining('/mock/mcp.json.bak')
    );
    expect(process.exitCode).toBeUndefined();
  });

  it('does not check node/npm when method is direct', async () => {
    const {
      installCommand,
      checkNodeInPath,
      checkNpmInPath,
      installOctocodeForClient,
      getInstallPreviewForClient,
    } = await loadDeps();

    vi.mocked(checkNodeInPath).mockReturnValue({
      installed: false,
      version: null,
    });
    vi.mocked(checkNpmInPath).mockReturnValue({
      installed: false,
      version: null,
    });
    vi.mocked(getInstallPreviewForClient).mockReturnValue({
      ...basePreview,
      method: 'direct',
      action: 'create',
    });
    vi.mocked(installOctocodeForClient).mockReturnValue({
      success: true,
      configPath: '/path',
    });

    await installCommand.handler!({
      command: 'install',
      args: [],
      options: { ide: 'cursor', method: 'direct' },
    });

    expect(process.exitCode).toBeUndefined();
    expect(installOctocodeForClient).toHaveBeenCalled();
  });

  it('uses short -m method alias', async () => {
    const {
      installCommand,
      installOctocodeForClient,
      getInstallPreviewForClient,
    } = await loadDeps();

    vi.mocked(getInstallPreviewForClient).mockReturnValue({
      ...basePreview,
      method: 'direct',
      action: 'create',
    });
    vi.mocked(installOctocodeForClient).mockReturnValue({
      success: true,
      configPath: '/path',
    });

    await installCommand.handler!({
      command: 'install',
      args: [],
      options: { ide: 'cursor', m: 'direct' },
    });

    expect(getInstallPreviewForClient).toHaveBeenCalledWith('cursor', 'direct');
    expect(installOctocodeForClient).toHaveBeenCalledWith({
      client: 'cursor',
      method: 'direct',
      force: false,
    });
  });

  it('installs advertised non-legacy clients through client install API', async () => {
    const {
      installCommand,
      installOctocodeForClient,
      getInstallPreviewForClient,
      checkNodeInPath,
      checkNpmInPath,
    } = await loadDeps();

    vi.mocked(checkNodeInPath).mockReturnValue({
      installed: true,
      version: 'v22.0.0',
    });
    vi.mocked(checkNpmInPath).mockReturnValue({
      installed: true,
      version: '10.0.0',
    });
    vi.mocked(getInstallPreviewForClient).mockReturnValue({
      ...basePreview,
      client: 'codex',
      action: 'create',
    });
    vi.mocked(installOctocodeForClient).mockReturnValue({
      success: true,
      configPath: '/path',
    });

    await installCommand.handler!({
      command: 'install',
      args: [],
      options: { ide: 'codex', method: 'npx' },
    });

    expect(getInstallPreviewForClient).toHaveBeenCalledWith('codex', 'npx');
    expect(installOctocodeForClient).toHaveBeenCalledWith({
      client: 'codex',
      method: 'npx',
      force: false,
    });
  });
});
