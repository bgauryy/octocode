/**
 * Skills command (`src/cli/commands/skills.ts`)
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';

const fsMocks = vi.hoisted(() => ({
  existsSync: vi.fn().mockReturnValue(false),
  rmSync: vi.fn(),
  mkdirSync: vi.fn(),
  symlinkSync: vi.fn(),
}));

vi.mock('node:fs', () => ({
  existsSync: (...args: Parameters<typeof import('node:fs').existsSync>) =>
    fsMocks.existsSync(...args),
  readFileSync: vi.fn(),
  writeFileSync: vi.fn(),
  mkdirSync: (...args: Parameters<typeof import('node:fs').mkdirSync>) =>
    fsMocks.mkdirSync(...args),
  unlinkSync: vi.fn(),
  rmSync: (...args: Parameters<typeof import('node:fs').rmSync>) =>
    fsMocks.rmSync(...args),
  statSync: vi.fn(),
  symlinkSync: (...args: Parameters<typeof import('node:fs').symlinkSync>) =>
    fsMocks.symlinkSync(...args),
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

const fsUtilsMocks = vi.hoisted(() => ({
  copyDirectory: vi.fn().mockReturnValue(true),
  dirExists: vi.fn().mockReturnValue(true),
  listSubdirectories: vi
    .fn()
    .mockReturnValue(['octocode-research', 'octocode-plan']),
  removeDirectory: vi.fn().mockReturnValue(true),
}));

vi.mock('../../../src/utils/fs.js', () => ({
  copyDirectory: (...args: unknown[]) => fsUtilsMocks.copyDirectory(...args),
  dirExists: (...args: unknown[]) => fsUtilsMocks.dirExists(...args),
  listSubdirectories: (...args: unknown[]) =>
    fsUtilsMocks.listSubdirectories(...args),
  removeDirectory: (...args: unknown[]) =>
    fsUtilsMocks.removeDirectory(...args),
}));

const skillsUtilsMocks = vi.hoisted(() => {
  const isSafeSkillName = (skillName: string) => {
    const trimmed = skillName.trim();
    return (
      trimmed.length > 0 &&
      trimmed === skillName &&
      trimmed !== '.' &&
      trimmed !== '..' &&
      !trimmed.includes('\0') &&
      !trimmed.includes('/') &&
      !trimmed.includes('\\')
    );
  };

  return {
    SKILL_INSTALL_TARGETS: [
      'claude-code',
      'claude-desktop',
      'cursor',
      'codex',
      'opencode',
    ],
    DEFAULT_SKILL_INSTALL_TARGETS: ['claude-code'],
    CLAUDE_SKILL_INSTALL_TARGETS: ['claude-code', 'claude-desktop'],
    formatSkillInstallTargets: vi
      .fn()
      .mockReturnValue('claude-code, claude-desktop, cursor, codex, opencode'),
    getSkillsSourceDir: vi.fn().mockReturnValue('/fake/skills/src'),
    getSkillsDestDir: vi.fn().mockReturnValue('/fake/skills/dest'),
    normalizeSkillTarget: vi.fn((target: string) => {
      const aliases: Record<string, string> = {
        claude: 'claude-code',
        'claude-code': 'claude-code',
        claudecode: 'claude-code',
        'claude-desktop': 'claude-desktop',
        claudedesktop: 'claude-desktop',
        cursor: 'cursor',
        codex: 'codex',
        opencode: 'opencode',
      };
      return aliases[target.trim().toLowerCase()] ?? null;
    }),
    getSkillsDirForTarget: vi.fn((target: string, defaultDestDir: string) => {
      if (target === 'claude-code') return defaultDestDir;
      if (target === 'claude-desktop') {
        return '/fake/appdata/Claude Desktop/skills';
      }
      return `/home/test/.${target}/skills`;
    }),
    isSafeSkillName: vi.fn(isSafeSkillName),
    resolveSkillDestination: vi.fn((destDir: string, skillName: string) =>
      isSafeSkillName(skillName) ? `${destDir}/${skillName}` : null
    ),
    resolveModeForTarget: vi.fn((strategy: string, target: string) => {
      if (strategy !== 'hybrid') return strategy;
      return target === 'claude-code' || target === 'claude-desktop'
        ? 'copy'
        : 'symlink';
    }),
    installSkillToDestination: vi.fn(
      ({
        sourcePath,
        destinationPath,
        mode,
        force,
      }: {
        sourcePath: string;
        destinationPath: string;
        mode: 'copy' | 'symlink';
        force?: boolean;
      }) => {
        try {
          if (fsMocks.existsSync(destinationPath)) {
            if (!force) return 'skipped';
            fsMocks.rmSync(destinationPath, { recursive: true, force: true });
          }

          const parentDir = destinationPath.replace(/\/[^/]+$/, '');
          if (!fsUtilsMocks.dirExists(parentDir)) {
            fsMocks.mkdirSync(parentDir, { recursive: true, mode: 0o700 });
          }

          if (mode === 'symlink') {
            fsMocks.symlinkSync(sourcePath, destinationPath, 'dir');
            return 'installed';
          }

          return fsUtilsMocks.copyDirectory(sourcePath, destinationPath)
            ? 'installed'
            : 'failed';
        } catch {
          return 'failed';
        }
      }
    ),
  };
});

vi.mock('../../../src/utils/skills.js', () => skillsUtilsMocks);

const promptsMocks = vi.hoisted(() => ({
  loadInquirer: vi.fn().mockResolvedValue(undefined),
  select: vi.fn(),
  checkbox: vi.fn(),
}));

vi.mock('../../../src/utils/prompts.js', () => ({
  loadInquirer: promptsMocks.loadInquirer,
  select: promptsMocks.select,
  checkbox: promptsMocks.checkbox,
}));

vi.mock('../../../src/utils/spinner.js', () => ({
  Spinner: vi.fn(function MockSpinner() {
    const instance = {
      start: vi.fn(),
      stop: vi.fn(),
      succeed: vi.fn(),
      fail: vi.fn(),
    };
    instance.start.mockImplementation(() => instance);
    return instance;
  }),
}));

const platformFlags = vi.hoisted(() => ({
  isWindows: false,
}));

vi.mock('../../../src/utils/platform.js', () => ({
  HOME: '/home/test',
  get isWindows() {
    return platformFlags.isWindows;
  },
  getAppDataPath: vi.fn().mockReturnValue('/fake/appdata'),
}));

describe('skillsCommand', () => {
  let consoleSpy: ReturnType<typeof vi.spyOn>;
  let originalExitCode: typeof process.exitCode;
  let ttyDescriptor: PropertyDescriptor | undefined;

  function setStdoutTTY(value: boolean) {
    Object.defineProperty(process.stdout, 'isTTY', {
      configurable: true,
      enumerable: true,
      writable: true,
      value,
    });
  }

  beforeEach(async () => {
    vi.resetModules();
    vi.clearAllMocks();
    consoleSpy = vi.spyOn(console, 'log').mockImplementation(() => {});
    ttyDescriptor = Object.getOwnPropertyDescriptor(process.stdout, 'isTTY');
    setStdoutTTY(false);
    originalExitCode = process.exitCode;
    process.exitCode = undefined;

    fsMocks.existsSync.mockReturnValue(false);
    fsUtilsMocks.copyDirectory.mockReturnValue(true);
    fsUtilsMocks.dirExists.mockReturnValue(true);
    fsUtilsMocks.listSubdirectories.mockReturnValue([
      'octocode-research',
      'octocode-plan',
    ]);
    fsUtilsMocks.removeDirectory.mockReturnValue(true);
    promptsMocks.loadInquirer.mockResolvedValue(undefined);
    promptsMocks.select.mockReset();
    promptsMocks.checkbox.mockReset();
    platformFlags.isWindows = false;
  });

  afterEach(() => {
    consoleSpy.mockRestore();
    if (ttyDescriptor) {
      Object.defineProperty(process.stdout, 'isTTY', ttyDescriptor);
    } else {
      delete (process.stdout as { isTTY?: boolean }).isTTY;
    }
    process.exitCode = originalExitCode;
  });

  async function loadCommand() {
    const mod = await import('../../../src/cli/commands/skills.js');
    return mod.skillsCommand;
  }

  it('list: shows available skills and destinations', async () => {
    const skillsCommand = await loadCommand();
    await skillsCommand.handler({
      command: 'skills',
      args: ['list'],
      options: {},
    });
    expect(consoleSpy).toHaveBeenCalledWith(
      expect.stringContaining('Available Octocode Skills')
    );
    expect(
      consoleSpy.mock.calls.some((c: unknown[]) =>
        String(c[0]).includes('octocode-research')
      )
    ).toBe(true);
  });

  it('list: shows installed when every destination has the skill folder', async () => {
    fsUtilsMocks.dirExists.mockImplementation((p: string) => {
      if (p === '/fake/skills/src') return true;
      return p.includes('octocode-research');
    });

    const skillsCommand = await loadCommand();
    await skillsCommand.handler({
      command: 'skills',
      args: ['list'],
      options: {},
    });
    expect(consoleSpy).toHaveBeenCalledWith(
      expect.stringContaining('installed')
    );
  });

  it('list: empty skills directory message', async () => {
    fsUtilsMocks.listSubdirectories.mockReturnValue([]);
    const skillsCommand = await loadCommand();
    await skillsCommand.handler({
      command: 'skills',
      args: ['list'],
      options: {},
    });
    expect(consoleSpy).toHaveBeenCalledWith(
      expect.stringContaining('No skills available.')
    );
  });

  it('list: missing src dir errors', async () => {
    fsUtilsMocks.dirExists.mockImplementation((path: string) =>
      path === '/fake/skills/src' ? false : true
    );

    const skillsCommand = await loadCommand();
    await skillsCommand.handler({
      command: 'skills',
      args: ['list'],
      options: {},
    });
    expect(process.exitCode).toBe(1);
    expect(consoleSpy).toHaveBeenCalledWith(
      expect.stringContaining('Skills directory not found')
    );
  });

  it('install specific: success with copy', async () => {
    const skillsCommand = await loadCommand();
    await skillsCommand.handler({
      command: 'skills',
      args: ['install'],
      options: {
        skill: 'octocode-research',
        targets: 'claude-code',
        mode: 'copy',
      },
    });
    expect(fsUtilsMocks.copyDirectory).toHaveBeenCalled();
    expect(process.exitCode).toBeUndefined();
    expect(consoleSpy).toHaveBeenCalledWith(
      expect.stringContaining('Installed to')
    );
  });

  it('install specific: skill not found lists available', async () => {
    const skillsCommand = await loadCommand();
    await skillsCommand.handler({
      command: 'skills',
      args: ['install'],
      options: {
        skill: 'missing-skill',
        targets: 'claude-code',
        mode: 'copy',
      },
    });
    expect(process.exitCode).toBe(1);
    expect(consoleSpy).toHaveBeenCalledWith(
      expect.stringContaining('Skill not found')
    );
  });

  it('install specific: skips existing without --force', async () => {
    fsMocks.existsSync.mockReturnValue(true);

    const skillsCommand = await loadCommand();
    await skillsCommand.handler({
      command: 'skills',
      args: ['install'],
      options: {
        skill: 'octocode-research',
        targets: 'claude-code',
        mode: 'copy',
      },
    });
    expect(consoleSpy).toHaveBeenCalledWith(expect.stringContaining('Skipped'));
    expect(process.exitCode).toBeUndefined();
  });

  it('install specific: --force overwrites existing', async () => {
    fsMocks.existsSync.mockReturnValue(true);

    const skillsCommand = await loadCommand();
    await skillsCommand.handler({
      command: 'skills',
      args: ['install'],
      options: {
        skill: 'octocode-research',
        targets: 'claude-code',
        mode: 'copy',
        force: true,
      },
    });
    expect(fsMocks.rmSync).toHaveBeenCalled();
    expect(fsUtilsMocks.copyDirectory).toHaveBeenCalled();
  });

  it('install specific: symlink mode uses symlinkSync', async () => {
    const skillsCommand = await loadCommand();
    await skillsCommand.handler({
      command: 'skills',
      args: ['install'],
      options: {
        skill: 'octocode-research',
        targets: 'cursor',
        mode: 'symlink',
      },
    });
    expect(fsMocks.symlinkSync).toHaveBeenCalled();
  });

  it('install specific: symlink failure sets exit code', async () => {
    fsMocks.symlinkSync.mockImplementation(() => {
      throw new Error('symlink failed');
    });

    const skillsCommand = await loadCommand();
    await skillsCommand.handler({
      command: 'skills',
      args: ['install'],
      options: {
        skill: 'octocode-research',
        targets: 'cursor',
        mode: 'symlink',
      },
    });

    expect(process.exitCode).toBe(1);
    fsMocks.symlinkSync.mockImplementation(() => undefined);
  });

  it('install specific: mkdir destination when folder missing', async () => {
    fsUtilsMocks.dirExists.mockImplementation((p: string) => {
      if (p === '/fake/skills/src') return true;
      if (p === '/fake/skills/dest') return false;
      return false;
    });

    const skillsCommand = await loadCommand();
    await skillsCommand.handler({
      command: 'skills',
      args: ['install'],
      options: {
        skill: 'octocode-research',
        targets: 'claude-code',
        mode: 'copy',
      },
    });

    expect(fsMocks.mkdirSync).toHaveBeenCalledWith(
      '/fake/skills/dest',
      expect.objectContaining({ recursive: true })
    );
  });

  it('list: Windows paths for claude-desktop target', async () => {
    platformFlags.isWindows = true;

    const skillsCommand = await loadCommand();
    await skillsCommand.handler({
      command: 'skills',
      args: ['list'],
      options: { targets: 'claude-desktop' },
    });

    expect(
      consoleSpy.mock.calls.some((c: unknown[]) =>
        String(c[0]).includes('appdata')
      )
    ).toBe(true);
  });

  it('TTY custom targets uses checkbox selection', async () => {
    setStdoutTTY(true);
    promptsMocks.select
      .mockResolvedValueOnce('custom')
      .mockResolvedValueOnce('hybrid');
    promptsMocks.checkbox.mockResolvedValue(['cursor']);

    const skillsCommand = await loadCommand();
    await skillsCommand.handler({
      command: 'skills',
      args: ['install'],
      options: {},
    });

    expect(promptsMocks.checkbox).toHaveBeenCalled();
    expect(fsMocks.symlinkSync).toHaveBeenCalled();
  });

  it('install specific: copy failure sets exit code', async () => {
    fsUtilsMocks.copyDirectory.mockReturnValue(false);

    const skillsCommand = await loadCommand();
    await skillsCommand.handler({
      command: 'skills',
      args: ['install'],
      options: {
        skill: 'octocode-research',
        targets: 'claude-code',
        mode: 'copy',
      },
    });
    expect(process.exitCode).toBe(1);
  });

  it('install all: succeeds when every copy works', async () => {
    const skillsCommand = await loadCommand();
    await skillsCommand.handler({
      command: 'skills',
      args: ['install'],
      options: {
        targets: 'claude-code',
        mode: 'copy',
      },
    });
    expect(
      consoleSpy.mock.calls.some((c: unknown[]) =>
        String(c[0]).includes('Skills installation finished.')
      )
    ).toBe(true);
    expect(process.exitCode).toBeUndefined();
  });

  it('install all: no skills available exits early', async () => {
    fsUtilsMocks.listSubdirectories.mockReturnValue([]);
    const skillsCommand = await loadCommand();
    await skillsCommand.handler({
      command: 'skills',
      args: ['install'],
      options: {
        targets: 'claude-code',
        mode: 'copy',
      },
    });
    expect(consoleSpy).toHaveBeenCalledWith(
      expect.stringContaining('No skills to install.')
    );
  });

  it('install all: creates destination dir when missing', async () => {
    fsUtilsMocks.dirExists.mockImplementation((p: string) => {
      if (p === '/fake/skills/src') return true;
      if (p === '/fake/skills/dest') return false;
      return false;
    });

    const skillsCommand = await loadCommand();
    await skillsCommand.handler({
      command: 'skills',
      args: ['install'],
      options: {
        targets: 'claude-code',
        mode: 'copy',
      },
    });

    expect(fsMocks.mkdirSync).toHaveBeenCalledWith(
      '/fake/skills/dest',
      expect.objectContaining({ recursive: true })
    );
  });

  it('install all: skipped existing targets prints overwrite hint', async () => {
    fsMocks.existsSync.mockReturnValue(true);

    const skillsCommand = await loadCommand();
    await skillsCommand.handler({
      command: 'skills',
      args: ['install'],
      options: {
        targets: 'claude-code',
        mode: 'copy',
      },
    });

    expect(consoleSpy).toHaveBeenCalledWith(expect.stringContaining('Skipped'));
    expect(consoleSpy).toHaveBeenCalledWith(expect.stringContaining('--force'));
    expect(process.exitCode).toBeUndefined();
  });

  it('install all: partial failures set exit code', async () => {
    let calls = 0;
    fsUtilsMocks.copyDirectory.mockImplementation(() => {
      calls++;
      return calls !== 2;
    });

    const skillsCommand = await loadCommand();
    await skillsCommand.handler({
      command: 'skills',
      args: ['install'],
      options: {
        targets: 'claude-code',
        mode: 'copy',
      },
    });
    expect(process.exitCode).toBe(1);
    expect(consoleSpy).toHaveBeenCalledWith(expect.stringContaining('Failed'));
  });

  it('install: invalid --mode errors', async () => {
    const skillsCommand = await loadCommand();
    await skillsCommand.handler({
      command: 'skills',
      args: ['install'],
      options: {
        mode: 'hybrid',
        targets: 'claude-code',
      },
    });
    expect(process.exitCode).toBe(1);
    expect(consoleSpy).toHaveBeenCalledWith(
      expect.stringContaining('Invalid --mode value')
    );
  });

  it('install: invalid --targets (empty after parse) errors', async () => {
    const skillsCommand = await loadCommand();
    await skillsCommand.handler({
      command: 'skills',
      args: ['install'],
      options: {
        targets: 'unknown-target,also-bad',
        mode: 'copy',
      },
    });
    expect(process.exitCode).toBe(1);
    expect(consoleSpy).toHaveBeenCalledWith(
      expect.stringContaining('No valid targets provided')
    );
  });

  it('remove: missing --skill errors', async () => {
    const skillsCommand = await loadCommand();
    await skillsCommand.handler({
      command: 'skills',
      args: ['remove'],
      options: {},
    });
    expect(process.exitCode).toBe(1);
    expect(consoleSpy).toHaveBeenCalledWith(expect.stringContaining('--skill'));
  });

  it('remove: succeeds when skill dirs exist', async () => {
    fsUtilsMocks.dirExists.mockImplementation((path: string) => {
      if (path === '/fake/skills/src') return true;
      return path.endsWith('octocode-research');
    });

    const skillsCommand = await loadCommand();
    await skillsCommand.handler({
      command: 'skills',
      args: ['remove'],
      options: { skill: 'octocode-research' },
    });
    expect(fsUtilsMocks.removeDirectory).toHaveBeenCalled();
    expect(process.exitCode).toBeUndefined();
  });

  it('remove: honors --targets option', async () => {
    fsUtilsMocks.dirExists.mockImplementation((path: string) => {
      if (path === '/fake/skills/src') return true;
      return path.endsWith('octocode-research');
    });

    const skillsCommand = await loadCommand();
    await skillsCommand.handler({
      command: 'skills',
      args: ['remove'],
      options: {
        skill: 'octocode-research',
        targets: 'cursor,codex',
      },
    });

    expect(fsUtilsMocks.removeDirectory).toHaveBeenCalledTimes(2);
    expect(fsUtilsMocks.removeDirectory).toHaveBeenCalledWith(
      '/home/test/.cursor/skills/octocode-research'
    );
    expect(fsUtilsMocks.removeDirectory).toHaveBeenCalledWith(
      '/home/test/.codex/skills/octocode-research'
    );
    expect(process.exitCode).toBeUndefined();
  });

  it('remove: warns when skill missing on all targets', async () => {
    fsUtilsMocks.dirExists.mockImplementation((path: string) =>
      path === '/fake/skills/src' ? true : false
    );

    const skillsCommand = await loadCommand();
    await skillsCommand.handler({
      command: 'skills',
      args: ['remove'],
      options: { skill: 'ghost-skill' },
    });
    expect(consoleSpy).toHaveBeenCalledWith(
      expect.stringContaining('Not found in')
    );
  });

  it('remove: removeDirectory failure sets exit code', async () => {
    fsUtilsMocks.dirExists.mockReturnValue(true);
    fsUtilsMocks.removeDirectory.mockReturnValue(false);

    const skillsCommand = await loadCommand();
    await skillsCommand.handler({
      command: 'skills',
      args: ['remove'],
      options: { skill: 'octocode-research' },
    });
    expect(process.exitCode).toBe(1);
  });

  it('unknown subcommand errors', async () => {
    const skillsCommand = await loadCommand();
    await skillsCommand.handler({
      command: 'skills',
      args: ['nope'],
      options: {},
    });
    expect(process.exitCode).toBe(1);
    expect(consoleSpy).toHaveBeenCalledWith(
      expect.stringContaining('Unknown subcommand')
    );
  });

  it('Non-TTY skips install prompts when targets+mode omitted', async () => {
    setStdoutTTY(false);

    const skillsCommand = await loadCommand();
    await skillsCommand.handler({
      command: 'skills',
      args: ['install'],
      options: {},
    });

    expect(promptsMocks.select).not.toHaveBeenCalled();
    expect(consoleSpy).toHaveBeenCalledWith(
      expect.stringContaining('Installing Octocode Skills')
    );
  });

  it('TTY with prompts runs hybrid strategy path', async () => {
    setStdoutTTY(true);
    promptsMocks.select
      .mockResolvedValueOnce('all')
      .mockResolvedValueOnce('hybrid');

    const skillsCommand = await loadCommand();
    await skillsCommand.handler({
      command: 'skills',
      args: ['install'],
      options: {},
    });

    expect(promptsMocks.select).toHaveBeenCalled();
    expect(fsMocks.symlinkSync).toHaveBeenCalled();
    expect(fsUtilsMocks.copyDirectory).toHaveBeenCalled();
  });

  it('TTY install cancelled when target preset is cancel', async () => {
    setStdoutTTY(true);
    promptsMocks.select.mockResolvedValueOnce('cancel');

    const skillsCommand = await loadCommand();
    await skillsCommand.handler({
      command: 'skills',
      args: ['install'],
      options: {},
    });

    expect(consoleSpy).toHaveBeenCalledWith(
      expect.stringContaining('cancelled')
    );
  });

  it('TTY install cancelled when strategy is cancel', async () => {
    setStdoutTTY(true);
    promptsMocks.select
      .mockResolvedValueOnce('all')
      .mockResolvedValueOnce('cancel');

    const skillsCommand = await loadCommand();
    await skillsCommand.handler({
      command: 'skills',
      args: ['install'],
      options: {},
    });

    expect(consoleSpy).toHaveBeenCalledWith(
      expect.stringContaining('cancelled')
    );
  });

  it('defaults to list when no subcommand', async () => {
    const skillsCommand = await loadCommand();
    await skillsCommand.handler({
      command: 'skills',
      args: [],
      options: {},
    });
    expect(consoleSpy).toHaveBeenCalledWith(
      expect.stringContaining('Available Octocode Skills')
    );
  });
});
