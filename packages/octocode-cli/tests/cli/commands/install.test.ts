import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';

vi.mock('node:fs', () => {
  const mod = {
    existsSync: vi.fn().mockReturnValue(false),
    readFileSync: vi.fn(),
    writeFileSync: vi.fn(),
    mkdirSync: vi.fn(),
    unlinkSync: vi.fn(),
    rmSync: vi.fn(),
    statSync: vi.fn(),
    symlinkSync: vi.fn(),
    copyFileSync: vi.fn(),
    accessSync: vi.fn(),
    constants: { W_OK: 2 },
    promises: {
      readFile: vi.fn(),
      writeFile: vi.fn(),
      mkdir: vi.fn(),
      unlink: vi.fn(),
      stat: vi.fn(),
    },
  };

  return { ...mod, default: mod };
});

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
    const fs = await import('node:fs');
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
      fs,
    };
  }

  function lastJson() {
    const calls = consoleSpy.mock.calls;
    for (let i = calls.length - 1; i >= 0; i--) {
      const arg = calls[i]?.[0];
      if (typeof arg === 'string') {
        try {
          return JSON.parse(arg);
        } catch {
          void 0;
        }
      }
    }
    return null;
  }

  const basePreview = {
    client: 'cursor' as const,
    method: 'npx' as const,
    configPath: '/mock/mcp.json',
    serverConfig: {},
    action: 'create' as const,
  };

  it('errors when no IDE is provided in non-TTY environment', async () => {
    Object.defineProperty(process.stdout, 'isTTY', {
      configurable: true,
      value: false,
    });
    const { installCommand } = await loadDeps();
    await installCommand.handler!({
      command: 'install',
      args: [],
      options: {},
    });
    expect(process.exitCode).toBe(1);
    expect(consoleSpy).toHaveBeenCalledWith(
      expect.stringContaining('Missing required option')
    );
    Object.defineProperty(process.stdout, 'isTTY', {
      configurable: true,
      value: undefined,
    });
  });

  it('calls runInteractiveMode when no IDE is provided in TTY environment', async () => {
    Object.defineProperty(process.stdout, 'isTTY', {
      configurable: true,
      value: true,
    });
    const { installCommand, runInteractiveMode } = await loadDeps();
    await installCommand.handler!({
      command: 'install',
      args: [],
      options: {},
    });
    expect(runInteractiveMode).toHaveBeenCalledTimes(1);
    expect(process.exitCode).toBeUndefined();
    Object.defineProperty(process.stdout, 'isTTY', {
      configurable: true,
      value: undefined,
    });
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

  it('rejects direct method as invalid', async () => {
    const { installCommand } = await loadDeps();

    await installCommand.handler!({
      command: 'install',
      args: [],
      options: { ide: 'cursor', method: 'direct' },
    });

    expect(process.exitCode).toBe(1);
  });

  it('uses short -m method alias with npx', async () => {
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
      method: 'npx',
      action: 'create',
    });
    vi.mocked(installOctocodeForClient).mockReturnValue({
      success: true,
      configPath: '/path',
    });

    await installCommand.handler!({
      command: 'install',
      args: [],
      options: { ide: 'cursor', m: 'npx' },
    });

    expect(getInstallPreviewForClient).toHaveBeenCalledWith('cursor', 'npx');
    expect(installOctocodeForClient).toHaveBeenCalledWith({
      client: 'cursor',
      method: 'npx',
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

  it('outputs JSON error when no IDE provided and --json set', async () => {
    Object.defineProperty(process.stdout, 'isTTY', {
      configurable: true,
      value: true,
    });
    const { installCommand } = await loadDeps();
    await installCommand.handler!({
      command: 'install',
      args: [],
      options: { json: true },
    });
    const json = lastJson();
    expect(json).toMatchObject({
      success: false,
      ide: null,
      configPath: null,
    });
    expect(json.error).toContain('Missing required option');
    expect(process.exitCode).toBe(1);
    Object.defineProperty(process.stdout, 'isTTY', {
      configurable: true,
      value: undefined,
    });
  });

  it('outputs JSON error when Node missing and --json set', async () => {
    const { installCommand, checkNodeInPath } = await loadDeps();
    vi.mocked(checkNodeInPath).mockReturnValueOnce({
      installed: false,
      version: null,
    });
    await installCommand.handler!({
      command: 'install',
      args: [],
      options: { ide: 'cursor', method: 'npx', json: true },
    });
    expect(lastJson()).toMatchObject({
      success: false,
      ide: 'cursor',
      configPath: null,
      error: 'Node.js is not found in PATH',
    });
    expect(process.exitCode).toBe(1);
  });

  it('outputs JSON error when npm missing and --json set', async () => {
    const { installCommand, checkNpmInPath } = await loadDeps();
    vi.mocked(checkNpmInPath).mockReturnValueOnce({
      installed: false,
      version: null,
    });
    await installCommand.handler!({
      command: 'install',
      args: [],
      options: { ide: 'cursor', method: 'npx', json: true },
    });
    expect(lastJson()).toMatchObject({
      success: false,
      error: 'npm is not found in PATH',
    });
    expect(process.exitCode).toBe(1);
  });

  it('outputs JSON error on invalid IDE with --json', async () => {
    const { installCommand } = await loadDeps();
    await installCommand.handler!({
      command: 'install',
      args: [],
      options: { ide: 'not-real', method: 'npx', json: true },
    });
    const json = lastJson();
    expect(json.success).toBe(false);
    expect(json.error).toContain('Invalid IDE');
    expect(process.exitCode).toBe(1);
  });

  it('outputs JSON error on invalid method with --json', async () => {
    const { installCommand } = await loadDeps();
    await installCommand.handler!({
      command: 'install',
      args: [],
      options: { ide: 'cursor', method: 'bogus', json: true },
    });
    const json = lastJson();
    expect(json.success).toBe(false);
    expect(json.error).toContain('Invalid method');
    expect(process.exitCode).toBe(1);
  });

  it('outputs JSON error when already configured without --force and --json', async () => {
    const { installCommand, getInstallPreviewForClient } = await loadDeps();
    vi.mocked(getInstallPreviewForClient).mockReturnValue({
      ...basePreview,
      action: 'override',
    });
    await installCommand.handler!({
      command: 'install',
      args: [],
      options: { ide: 'cursor', method: 'npx', json: true },
    });
    const json = lastJson();
    expect(json).toMatchObject({
      success: false,
      ide: 'cursor',
      configPath: '/mock/mcp.json',
      error: 'Already configured. Use --force to overwrite.',
    });
    expect(process.exitCode).toBe(1);
  });

  it('outputs JSON success when install succeeds with --json', async () => {
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
      backupPath: '/mock/mcp.json.bak',
    });
    await installCommand.handler!({
      command: 'install',
      args: [],
      options: { ide: 'cursor', method: 'npx', json: true },
    });
    expect(vi.mocked(Spinner)).not.toHaveBeenCalled();
    expect(lastJson()).toMatchObject({
      success: true,
      ide: 'cursor',
      configPath: '/mock/mcp.json',
      backupPath: '/mock/mcp.json.bak',
      error: null,
    });
    expect(process.exitCode).toBeUndefined();
  });

  it('outputs JSON failure with null configPath when install fails with --json', async () => {
    const {
      installCommand,
      installOctocodeForClient,
      getInstallPreviewForClient,
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
      options: { ide: 'cursor', method: 'npx', json: true },
    });
    expect(lastJson()).toMatchObject({
      success: false,
      configPath: null,
      backupPath: null,
      error: 'disk full',
    });
    expect(process.exitCode).toBe(1);
  });

  it('rollback fails (text) when backup not found', async () => {
    const { installCommand, fs } = await loadDeps();
    vi.mocked(fs.existsSync).mockReturnValue(false);
    await installCommand.handler!({
      command: 'install',
      args: [],
      options: { ide: 'cursor', rollback: true },
    });
    expect(consoleSpy).toHaveBeenCalledWith(
      expect.stringContaining('Backup not found')
    );
    expect(process.exitCode).toBe(1);
  });

  it('rollback fails (json) when backup not found with explicit --backup-path', async () => {
    const { installCommand, fs } = await loadDeps();
    vi.mocked(fs.existsSync).mockReturnValue(false);
    await installCommand.handler!({
      command: 'install',
      args: [],
      options: {
        ide: 'cursor',
        rollback: true,
        json: true,
        'backup-path': '/custom/backup.bak',
      },
    });
    const json = lastJson();
    expect(json.success).toBe(false);
    expect(json.backupPath).toBe('/custom/backup.bak');
    expect(json.error).toContain('Backup not found');
    expect(process.exitCode).toBe(1);
  });

  it('rollback succeeds (text) copying backup over config', async () => {
    const { installCommand, fs } = await loadDeps();
    vi.mocked(fs.existsSync).mockReturnValue(true);
    await installCommand.handler!({
      command: 'install',
      args: [],
      options: { ide: 'cursor', rollback: true },
    });
    expect(fs.mkdirSync).toHaveBeenCalled();
    expect(fs.copyFileSync).toHaveBeenCalled();
    expect(consoleSpy).toHaveBeenCalledWith(
      expect.stringContaining('Rolled back')
    );
    expect(process.exitCode).toBeUndefined();
  });

  it('rollback succeeds (json) and reports paths', async () => {
    const { installCommand, fs } = await loadDeps();
    vi.mocked(fs.existsSync).mockReturnValue(true);
    await installCommand.handler!({
      command: 'install',
      args: [],
      options: { ide: 'cursor', rollback: true, json: true },
    });
    const json = lastJson();
    expect(json).toMatchObject({ success: true, ide: 'cursor' });
    expect(json.backupPath).toContain('.bak');
    expect(process.exitCode).toBeUndefined();
  });

  it('rollback handles copy failure (text)', async () => {
    const { installCommand, fs } = await loadDeps();
    vi.mocked(fs.existsSync).mockReturnValue(true);
    vi.mocked(fs.copyFileSync).mockImplementationOnce(() => {
      throw new Error('permission denied');
    });
    await installCommand.handler!({
      command: 'install',
      args: [],
      options: { ide: 'cursor', rollback: true },
    });
    expect(consoleSpy).toHaveBeenCalledWith(
      expect.stringContaining('Rollback failed')
    );
    expect(process.exitCode).toBe(1);
  });

  it('rollback handles copy failure (json) with non-Error throw', async () => {
    const { installCommand, fs } = await loadDeps();
    vi.mocked(fs.existsSync).mockReturnValue(true);
    vi.mocked(fs.copyFileSync).mockImplementationOnce(() => {
      throw 'string failure';
    });
    await installCommand.handler!({
      command: 'install',
      args: [],
      options: { ide: 'cursor', rollback: true, json: true },
    });
    const json = lastJson();
    expect(json.success).toBe(false);
    expect(json.error).toBe('string failure');
    expect(process.exitCode).toBe(1);
  });

  it('check (text) reports ready when parent writable and action create', async () => {
    const { installCommand, getInstallPreviewForClient, fs } = await loadDeps();
    vi.mocked(getInstallPreviewForClient).mockReturnValue({
      ...basePreview,
      action: 'create',
    });
    vi.mocked(fs.existsSync).mockImplementation(
      (p: unknown) => String(p) === '/mock'
    );
    vi.mocked(fs.accessSync).mockImplementation(() => undefined);
    await installCommand.handler!({
      command: 'install',
      args: [],
      options: { ide: 'cursor', method: 'npx', check: true },
    });
    expect(consoleSpy).toHaveBeenCalledWith(
      expect.stringContaining('Ready to install')
    );
    expect(process.exitCode).toBeUndefined();
  });

  it('check (text) warns overwrite when action override and no force', async () => {
    const { installCommand, getInstallPreviewForClient, fs } = await loadDeps();
    vi.mocked(getInstallPreviewForClient).mockReturnValue({
      ...basePreview,
      action: 'override',
    });
    vi.mocked(fs.existsSync).mockReturnValue(true);
    vi.mocked(fs.accessSync).mockImplementation(() => undefined);
    await installCommand.handler!({
      command: 'install',
      args: [],
      options: { ide: 'cursor', method: 'npx', check: true },
    });
    expect(consoleSpy).toHaveBeenCalledWith(
      expect.stringContaining('Add --force to overwrite')
    );
    expect(process.exitCode).toBeUndefined();
  });

  it('check (text) reports not writable when parent exists but access throws', async () => {
    const { installCommand, getInstallPreviewForClient, fs } = await loadDeps();
    vi.mocked(getInstallPreviewForClient).mockReturnValue({
      ...basePreview,
      action: 'create',
    });
    vi.mocked(fs.existsSync).mockImplementation(
      (p: unknown) => String(p) === '/mock'
    );
    vi.mocked(fs.accessSync).mockImplementation(() => {
      throw new Error('EACCES');
    });
    await installCommand.handler!({
      command: 'install',
      args: [],
      options: { ide: 'cursor', method: 'npx', check: true },
    });
    expect(consoleSpy).toHaveBeenCalledWith(
      expect.stringContaining('Cannot write to')
    );
    expect(process.exitCode).toBe(1);
  });

  it('check (text) checks grandparent when parent dir missing and writable', async () => {
    const { installCommand, getInstallPreviewForClient, fs } = await loadDeps();
    vi.mocked(getInstallPreviewForClient).mockReturnValue({
      ...basePreview,
      action: 'create',
    });
    vi.mocked(fs.existsSync).mockReturnValue(false);
    vi.mocked(fs.accessSync).mockImplementation(() => undefined);
    await installCommand.handler!({
      command: 'install',
      args: [],
      options: { ide: 'cursor', method: 'npx', check: true },
    });
    expect(consoleSpy).toHaveBeenCalledWith(
      expect.stringContaining('Ready to install')
    );
    expect(process.exitCode).toBeUndefined();
  });

  it('check (text) reports not writable when grandparent access throws', async () => {
    const { installCommand, getInstallPreviewForClient, fs } = await loadDeps();
    vi.mocked(getInstallPreviewForClient).mockReturnValue({
      ...basePreview,
      action: 'create',
    });
    vi.mocked(fs.existsSync).mockReturnValue(false);
    vi.mocked(fs.accessSync).mockImplementation(() => {
      throw new Error('EACCES');
    });
    await installCommand.handler!({
      command: 'install',
      args: [],
      options: { ide: 'cursor', method: 'npx', check: true },
    });
    expect(consoleSpy).toHaveBeenCalledWith(
      expect.stringContaining('Cannot write to')
    );
    expect(process.exitCode).toBe(1);
  });

  it('check (text) overwrite with force shows ready', async () => {
    const { installCommand, getInstallPreviewForClient, fs } = await loadDeps();
    vi.mocked(getInstallPreviewForClient).mockReturnValue({
      ...basePreview,
      action: 'override',
    });
    vi.mocked(fs.existsSync).mockReturnValue(true);
    vi.mocked(fs.accessSync).mockImplementation(() => undefined);
    await installCommand.handler!({
      command: 'install',
      args: [],
      options: { ide: 'cursor', method: 'npx', check: true, force: true },
    });
    expect(consoleSpy).toHaveBeenCalledWith(
      expect.stringContaining('Ready to install')
    );
    expect(process.exitCode).toBeUndefined();
  });

  it('check (json) returns full pre-flight payload', async () => {
    const { installCommand, getInstallPreviewForClient, fs } = await loadDeps();
    vi.mocked(getInstallPreviewForClient).mockReturnValue({
      ...basePreview,
      action: 'override',
    });
    vi.mocked(fs.existsSync).mockReturnValue(true);
    vi.mocked(fs.accessSync).mockImplementation(() => undefined);
    await installCommand.handler!({
      command: 'install',
      args: [],
      options: { ide: 'cursor', method: 'npx', check: true, json: true },
    });
    const json = lastJson();
    expect(json).toMatchObject({
      ide: 'cursor',
      configPath: '/mock/mcp.json',
      configExists: true,
      parentDirExists: true,
      parentDirWritable: true,
      action: 'override',
      method: 'npx',
      wouldOverwrite: true,
    });
    expect(json.ready).toBe(false);
    expect(process.exitCode).toBeUndefined();
  });

  it('check (json) reports octocodeInstalled true when config has octocode-mcp', async () => {
    const { installCommand, getInstallPreviewForClient, fs } = await loadDeps();
    vi.mocked(getInstallPreviewForClient).mockReturnValue({
      ...basePreview,
      action: 'create',
    });
    vi.mocked(fs.existsSync).mockReturnValue(true);
    vi.mocked(fs.statSync).mockReturnValue({
      isFile: () => true,
      isDirectory: () => true,
    } as unknown as ReturnType<typeof fs.statSync>);
    vi.mocked(fs.readFileSync).mockReturnValue(
      JSON.stringify({ mcpServers: { 'octocode-mcp': {} } })
    );
    vi.mocked(fs.accessSync).mockImplementation(() => undefined);
    await installCommand.handler!({
      command: 'install',
      args: [],
      options: { ide: 'cursor', method: 'npx', check: true, json: true },
    });
    expect(lastJson()).toMatchObject({
      octocodeInstalled: true,
      ready: true,
    });
  });
});
