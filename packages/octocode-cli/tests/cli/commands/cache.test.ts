import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { existsSync, rmSync } from 'node:fs';
import {
  clearSkillsCache,
  getSkillsCacheDir,
} from '../../../src/utils/skills-fetch.js';
import { getDirectorySizeBytes, formatBytes } from 'octocode-shared';

const { mockOctocodePaths } = vi.hoisted(() => ({
  mockOctocodePaths: {
    home: '/fake/octocode',
    repos: '/fake/repos',
    logs: '/fake/logs',
  },
}));

vi.mock('node:fs', () => ({
  existsSync: vi.fn().mockReturnValue(true),
  rmSync: vi.fn(),
  readFileSync: vi.fn(),
  writeFileSync: vi.fn(),
  mkdirSync: vi.fn(),
  unlinkSync: vi.fn(),
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

vi.mock('../../../src/utils/skills-fetch.js', () => ({
  clearSkillsCache: vi.fn(),
  getSkillsCacheDir: vi.fn().mockReturnValue('/fake/cache/skills'),
}));

vi.mock('octocode-shared', () => ({
  paths: mockOctocodePaths,
  getDirectorySizeBytes: vi.fn().mockReturnValue(1024),
  formatBytes: vi.fn().mockImplementation((b: number) => `${b} B`),
}));

const { confirmMock } = vi.hoisted(() => ({
  confirmMock: vi.fn().mockResolvedValue(true),
}));

vi.mock('../../../src/utils/prompts.js', () => ({
  confirm: confirmMock,
}));

describe('cacheCommand', () => {
  let consoleSpy: ReturnType<typeof vi.spyOn>;
  let originalExitCode: typeof process.exitCode;
  let originalOctocodeHome: string | undefined;
  let originalHome: string | undefined;

  beforeEach(() => {
    vi.resetModules();
    vi.clearAllMocks();
    mockOctocodePaths.home = '/fake/octocode';
    mockOctocodePaths.repos = '/fake/repos';
    mockOctocodePaths.logs = '/fake/logs';
    originalOctocodeHome = process.env.OCTOCODE_HOME;
    originalHome = process.env.HOME;
    delete process.env.OCTOCODE_HOME;
    vi.mocked(existsSync).mockReturnValue(true);
    vi.mocked(getDirectorySizeBytes).mockReturnValue(1024);
    vi.mocked(formatBytes).mockImplementation((b: number) => `${b} B`);
    vi.mocked(getSkillsCacheDir).mockReturnValue('/fake/cache/skills');
    consoleSpy = vi.spyOn(console, 'log').mockImplementation(() => {});
    originalExitCode = process.exitCode;
    process.exitCode = undefined;
  });

  afterEach(() => {
    consoleSpy.mockRestore();
    process.exitCode = originalExitCode;
    if (originalOctocodeHome === undefined) {
      delete process.env.OCTOCODE_HOME;
    } else {
      process.env.OCTOCODE_HOME = originalOctocodeHome;
    }
    if (originalHome === undefined) {
      delete process.env.HOME;
    } else {
      process.env.HOME = originalHome;
    }
  });

  it('status (default subcommand) shows repos/skills/logs sizes and total', async () => {
    const { cacheCommand } = await import('../../../src/cli/commands/cache.js');
    await cacheCommand.handler({
      command: 'cache',
      args: [],
      options: {},
    });

    expect(getDirectorySizeBytes).toHaveBeenCalledWith('/fake/repos');
    expect(getDirectorySizeBytes).toHaveBeenCalledWith('/fake/cache/skills');
    expect(getDirectorySizeBytes).toHaveBeenCalledWith('/fake/logs');
    expect(
      consoleSpy.mock.calls.some((call: unknown[]) =>
        String(call[0]).includes('Total')
      )
    ).toBe(true);
    expect(
      consoleSpy.mock.calls.some((call: unknown[]) =>
        String(call[0]).includes('repos')
      )
    ).toBe(true);
    expect(
      consoleSpy.mock.calls.some((call: unknown[]) =>
        String(call[0]).includes('skills')
      )
    ).toBe(true);
    expect(
      consoleSpy.mock.calls.some((call: unknown[]) =>
        String(call[0]).includes('logs')
      )
    ).toBe(true);
    expect(process.exitCode).toBeUndefined();
  });

  it('status explicit subcommand matches default output shape', async () => {
    const { cacheCommand } = await import('../../../src/cli/commands/cache.js');
    await cacheCommand.handler({
      command: 'cache',
      args: ['status'],
      options: {},
    });
    expect(getDirectorySizeBytes).toHaveBeenCalledTimes(3);
  });

  it('clean without target flags sets exitCode 1', async () => {
    const { cacheCommand } = await import('../../../src/cli/commands/cache.js');
    await cacheCommand.handler({
      command: 'cache',
      args: ['clean'],
      options: {},
    });
    expect(process.exitCode).toBe(1);
    expect(
      consoleSpy.mock.calls.some((call: unknown[]) =>
        String(call[0]).includes('Missing clean target')
      )
    ).toBe(true);
  });

  it('clean --repos removes repos dir and reports freed bytes', async () => {
    const { cacheCommand } = await import('../../../src/cli/commands/cache.js');
    await cacheCommand.handler({
      command: 'cache',
      args: ['clean'],
      options: { repos: true },
    });

    expect(existsSync).toHaveBeenCalledWith('/fake/repos');
    expect(rmSync).toHaveBeenCalledWith('/fake/repos', {
      recursive: true,
      force: true,
    });
    expect(
      consoleSpy.mock.calls.some((call: unknown[]) =>
        String(call[0]).includes('Cache cleanup complete')
      )
    ).toBe(true);
    expect(process.exitCode).toBeUndefined();
  });

  it('clean --skills calls clearSkillsCache', async () => {
    const { cacheCommand } = await import('../../../src/cli/commands/cache.js');
    await cacheCommand.handler({
      command: 'cache',
      args: ['clean'],
      options: { skills: true },
    });

    expect(clearSkillsCache).toHaveBeenCalled();
    expect(
      consoleSpy.mock.calls.some((call: unknown[]) =>
        String(call[0]).includes('Cache cleanup complete')
      )
    ).toBe(true);
  });

  it('clean --logs removes logs dir', async () => {
    const { cacheCommand } = await import('../../../src/cli/commands/cache.js');
    await cacheCommand.handler({
      command: 'cache',
      args: ['clean'],
      options: { logs: true },
    });

    expect(rmSync).toHaveBeenCalledWith('/fake/logs', {
      recursive: true,
      force: true,
    });
  });

  it('clean --all cleans repos, skills, and logs', async () => {
    const { cacheCommand } = await import('../../../src/cli/commands/cache.js');
    await cacheCommand.handler({
      command: 'cache',
      args: ['clean'],
      options: { all: true },
    });

    expect(rmSync).toHaveBeenCalledWith('/fake/repos', {
      recursive: true,
      force: true,
    });
    expect(rmSync).toHaveBeenCalledWith('/fake/logs', {
      recursive: true,
      force: true,
    });
    expect(clearSkillsCache).toHaveBeenCalled();
  });

  it('clean -a alias triggers all targets', async () => {
    const { cacheCommand } = await import('../../../src/cli/commands/cache.js');
    await cacheCommand.handler({
      command: 'cache',
      args: ['clean'],
      options: { a: true },
    });

    expect(clearSkillsCache).toHaveBeenCalled();
    expect(rmSync).toHaveBeenCalled();
  });

  it.each([
    ['tools', { tools: true }],
    ['local', { local: true }],
    ['lsp', { lsp: true }],
    ['api', { api: true }],
  ])('clean --%s shows in-memory cache message', async (_name, options) => {
    const { cacheCommand } = await import('../../../src/cli/commands/cache.js');
    await cacheCommand.handler({
      command: 'cache',
      args: ['clean'],
      options,
    });

    expect(
      consoleSpy.mock.calls.some((call: unknown[]) =>
        String(call[0]).includes('No disk caches to clean')
      )
    ).toBe(true);
    expect(
      consoleSpy.mock.calls.some((call: unknown[]) =>
        String(call[0]).includes('in-memory')
      )
    ).toBe(true);
  });

  it('clean when repos/logs missing prints nothing to clean when only disk targets', async () => {
    vi.mocked(existsSync).mockReturnValue(false);

    const { cacheCommand } = await import('../../../src/cli/commands/cache.js');
    await cacheCommand.handler({
      command: 'cache',
      args: ['clean'],
      options: { repos: true, logs: true },
    });

    expect(rmSync).not.toHaveBeenCalled();
    expect(
      consoleSpy.mock.calls.some((call: unknown[]) =>
        String(call[0]).includes('Nothing to clean')
      )
    ).toBe(true);
  });

  it('unknown subcommand sets exitCode 1', async () => {
    const { cacheCommand } = await import('../../../src/cli/commands/cache.js');
    await cacheCommand.handler({
      command: 'cache',
      args: ['nope'],
      options: {},
    });

    expect(process.exitCode).toBe(1);
    expect(
      consoleSpy.mock.calls.some((call: unknown[]) =>
        String(call[0]).includes('Unknown cache subcommand')
      )
    ).toBe(true);
  });

  it('status uses OCTOCODE_HOME when paths.home is falsy', async () => {
    mockOctocodePaths.home = '';
    process.env.OCTOCODE_HOME = '/env/octocode';

    const { cacheCommand } = await import('../../../src/cli/commands/cache.js');
    await cacheCommand.handler({
      command: 'cache',
      args: ['status'],
      options: {},
    });

    expect(
      consoleSpy.mock.calls.some((call: unknown[]) =>
        String(call[0]).includes('/env/octocode')
      )
    ).toBe(true);
  });

  it('status uses HOME/.octocode when paths.home and OCTOCODE_HOME are unset', async () => {
    mockOctocodePaths.home = '';
    process.env.HOME = '/users/tester';

    const { cacheCommand } = await import('../../../src/cli/commands/cache.js');
    await cacheCommand.handler({
      command: 'cache',
      args: ['status'],
      options: {},
    });

    expect(
      consoleSpy.mock.calls.some((call: unknown[]) =>
        String(call[0]).includes('/users/tester/.octocode')
      )
    ).toBe(true);
  });

  it('status falls back to relative .octocode when HOME is unset', async () => {
    mockOctocodePaths.home = '';
    delete process.env.HOME;

    const { cacheCommand } = await import('../../../src/cli/commands/cache.js');
    await cacheCommand.handler({
      command: 'cache',
      args: ['status'],
      options: {},
    });

    expect(
      consoleSpy.mock.calls.some((call: unknown[]) =>
        String(call.join(' ')).includes('.octocode')
      )
    ).toBe(true);
  });

  it('resolves repos and logs dirs under octocodeHome when paths.repos/logs falsy', async () => {
    mockOctocodePaths.repos = '';
    mockOctocodePaths.logs = '';

    const { cacheCommand } = await import('../../../src/cli/commands/cache.js');
    await cacheCommand.handler({
      command: 'cache',
      args: ['status'],
      options: {},
    });

    expect(getDirectorySizeBytes).toHaveBeenCalledWith('/fake/octocode/repos');
    expect(getDirectorySizeBytes).toHaveBeenCalledWith('/fake/octocode/logs');
  });

  it('clean --skills counts cleanup when size decreases after clear', async () => {
    let skillsPass = 0;
    vi.mocked(getDirectorySizeBytes).mockImplementation((dir: string) => {
      if (dir === '/fake/cache/skills') {
        skillsPass += 1;
        // plan call returns 500, before-delete call returns 500, after-delete call returns 0
        return skillsPass >= 3 ? 0 : 500;
      }
      return 1024;
    });

    const { cacheCommand } = await import('../../../src/cli/commands/cache.js');
    await cacheCommand.handler({
      command: 'cache',
      args: ['clean'],
      options: { skills: true },
    });

    expect(clearSkillsCache).toHaveBeenCalled();
    expect(
      consoleSpy.mock.calls.some((call: unknown[]) =>
        String(call[0]).includes('Cache cleanup complete')
      )
    ).toBe(true);
  });

  it('status --json outputs structured cache report', async () => {
    const { cacheCommand } = await import('../../../src/cli/commands/cache.js');
    await cacheCommand.handler({
      command: 'cache',
      args: ['status'],
      options: { json: true },
    });

    const out = consoleSpy.mock.calls
      .map((call: unknown[]) => String(call[0]))
      .join('\n');
    const parsed = JSON.parse(out.trim());
    expect(parsed.repos.path).toBe('/fake/repos');
    expect(parsed.skills.path).toBe('/fake/cache/skills');
    expect(parsed.logs.path).toBe('/fake/logs');
    expect(parsed.totalBytes).toBe(3072);
    expect(parsed.totalFormatted).toBe('3072 B');
  });

  it('clean without target --json outputs error object and exits 1', async () => {
    const { cacheCommand } = await import('../../../src/cli/commands/cache.js');
    await cacheCommand.handler({
      command: 'cache',
      args: ['clean'],
      options: { json: true },
    });

    const out = consoleSpy.mock.calls
      .map((call: unknown[]) => String(call[0]))
      .join('\n');
    const parsed = JSON.parse(out.trim());
    expect(parsed.success).toBe(false);
    expect(parsed.error).toContain('Missing clean target');
    expect(process.exitCode).toBe(1);
  });

  it('clean --dry-run --json reports plan without deleting', async () => {
    const { cacheCommand } = await import('../../../src/cli/commands/cache.js');
    await cacheCommand.handler({
      command: 'cache',
      args: ['clean'],
      options: { all: true, 'dry-run': true, json: true },
    });

    expect(rmSync).not.toHaveBeenCalled();
    expect(clearSkillsCache).not.toHaveBeenCalled();
    const out = consoleSpy.mock.calls
      .map((call: unknown[]) => String(call[0]))
      .join('\n');
    const parsed = JSON.parse(out.trim());
    expect(parsed.dryRun).toBe(true);
    expect(parsed.plan.map((p: { target: string }) => p.target)).toEqual([
      'repos',
      'skills',
      'logs',
    ]);
    expect(parsed.totalBytes).toBe(3072);
  });

  it('clean --dry-run (non-json) prints plan and tools advisory', async () => {
    const { cacheCommand } = await import('../../../src/cli/commands/cache.js');
    await cacheCommand.handler({
      command: 'cache',
      args: ['clean'],
      options: { all: true, tools: true, n: true },
    });

    expect(rmSync).not.toHaveBeenCalled();
    expect(
      consoleSpy.mock.calls.some((call: unknown[]) =>
        String(call[0]).includes('DRY RUN')
      )
    ).toBe(true);
    expect(
      consoleSpy.mock.calls.some((call: unknown[]) =>
        String(call[0]).includes('in-memory only')
      )
    ).toBe(true);
    expect(
      consoleSpy.mock.calls.some((call: unknown[]) =>
        String(call[0]).includes('Remove --dry-run to apply.')
      )
    ).toBe(true);
  });

  it('clean --all on TTY prompts and aborts when declined', async () => {
    const originalIsTTY = process.stdout.isTTY;
    Object.defineProperty(process.stdout, 'isTTY', {
      value: true,
      configurable: true,
    });
    confirmMock.mockResolvedValueOnce(false);

    try {
      const { cacheCommand } =
        await import('../../../src/cli/commands/cache.js');
      await cacheCommand.handler({
        command: 'cache',
        args: ['clean'],
        options: { all: true },
      });

      expect(confirmMock).toHaveBeenCalled();
      expect(rmSync).not.toHaveBeenCalled();
      expect(
        consoleSpy.mock.calls.some((call: unknown[]) =>
          String(call[0]).includes('Cancelled.')
        )
      ).toBe(true);
    } finally {
      Object.defineProperty(process.stdout, 'isTTY', {
        value: originalIsTTY,
        configurable: true,
      });
    }
  });

  it('clean --all on TTY prompts and proceeds when confirmed', async () => {
    const originalIsTTY = process.stdout.isTTY;
    Object.defineProperty(process.stdout, 'isTTY', {
      value: true,
      configurable: true,
    });
    confirmMock.mockResolvedValueOnce(true);

    try {
      const { cacheCommand } =
        await import('../../../src/cli/commands/cache.js');
      await cacheCommand.handler({
        command: 'cache',
        args: ['clean'],
        options: { all: true },
      });

      expect(confirmMock).toHaveBeenCalled();
      expect(rmSync).toHaveBeenCalledWith('/fake/repos', {
        recursive: true,
        force: true,
      });
      expect(clearSkillsCache).toHaveBeenCalled();
    } finally {
      Object.defineProperty(process.stdout, 'isTTY', {
        value: originalIsTTY,
        configurable: true,
      });
    }
  });

  it('clean --json outputs success summary with targets', async () => {
    const { cacheCommand } = await import('../../../src/cli/commands/cache.js');
    await cacheCommand.handler({
      command: 'cache',
      args: ['clean'],
      options: { all: true, json: true },
    });

    expect(rmSync).toHaveBeenCalled();
    expect(clearSkillsCache).toHaveBeenCalled();
    const out = consoleSpy.mock.calls
      .map((call: unknown[]) => String(call[0]))
      .join('\n');
    const parsed = JSON.parse(out.trim());
    expect(parsed.success).toBe(true);
    expect(parsed.cleaned).toBe(true);
    expect(parsed.targets).toEqual(['repos', 'skills', 'logs']);
  });

  it('clean --tools --json suppresses in-memory message and reports nothing cleaned', async () => {
    vi.mocked(existsSync).mockReturnValue(false);
    const { cacheCommand } = await import('../../../src/cli/commands/cache.js');
    await cacheCommand.handler({
      command: 'cache',
      args: ['clean'],
      options: { tools: true, json: true },
    });

    expect(
      consoleSpy.mock.calls.some((call: unknown[]) =>
        String(call[0]).includes('No disk caches to clean')
      )
    ).toBe(false);
    const out = consoleSpy.mock.calls
      .map((call: unknown[]) => String(call[0]))
      .join('\n');
    const parsed = JSON.parse(out.trim());
    expect(parsed.success).toBe(true);
    expect(parsed.cleaned).toBe(false);
  });
});
