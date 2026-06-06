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

const fsReadMocks = vi.hoisted(() => ({
  fileExists: vi.fn().mockReturnValue(false),
  readFileContent: vi.fn().mockReturnValue(null),
}));

vi.mock('../../../src/utils/fs.js', () => ({
  copyDirectory: (...args: unknown[]) => fsUtilsMocks.copyDirectory(...args),
  dirExists: (...args: unknown[]) => fsUtilsMocks.dirExists(...args),
  listSubdirectories: (...args: unknown[]) =>
    fsUtilsMocks.listSubdirectories(...args),
  removeDirectory: (...args: unknown[]) =>
    fsUtilsMocks.removeDirectory(...args),
  fileExists: (...args: unknown[]) => fsReadMocks.fileExists(...args),
  readFileContent: (...args: unknown[]) => fsReadMocks.readFileContent(...args),
}));

vi.mock('../../../src/utils/parsers/frontmatter.js', () => ({
  parseSkillFrontmatter: vi.fn().mockReturnValue(null),
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
    getSkillsDirForTarget: vi.fn((target: string, defaultDestDir?: string) => {
      const base = defaultDestDir ?? '/fake/skills/dest';
      if (target === 'claude-code') return base;
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
    getSkillMetadata: vi.fn().mockReturnValue(null),
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

const skillsFetchMocks = vi.hoisted(() => ({
  fetchSkillsShSearch: vi.fn(),
  readSkillFromGitHub: vi.fn(),
}));

vi.mock('../../../src/utils/skills-fetch.js', () => ({
  fetchSkillsShSearch: skillsFetchMocks.fetchSkillsShSearch,
  readSkillFromGitHub: skillsFetchMocks.readSkillFromGitHub,
  fetchMarketplaceSkills: vi.fn().mockResolvedValue([]),
  searchSkills: vi.fn().mockReturnValue([]),
  installMarketplaceSkill: vi.fn(),
  clearSkillsCache: vi.fn(),
  clearSourceCache: vi.fn(),
  getCacheInfo: vi.fn(),
  getSkillsCacheDir: vi.fn().mockReturnValue('/fake/cache'),
}));

const octocodePublicMocks = vi.hoisted(() => ({
  executeDirectTool: vi.fn(),
  prepareDirectToolInputFromJsonText: vi.fn(),
}));

vi.mock('octocode-mcp/public', async importOriginal => {
  const original = await importOriginal<typeof import('octocode-mcp/public')>();
  return {
    ...original,
    executeDirectTool: octocodePublicMocks.executeDirectTool,
    prepareDirectToolInputFromJsonText:
      octocodePublicMocks.prepareDirectToolInputFromJsonText,
  };
});

vi.mock('../../../src/configs/skills-marketplace.js', () => ({
  SKILLS_MARKETPLACES: [
    {
      id: 'octocode-skills',
      name: 'Octocode',
      type: 'github',
      owner: 'bgauryy',
      repo: 'octocode-mcp',
      branch: 'main',
      skillsPath: 'skills',
      skillPattern: 'skill-folders',
      description: 'Research, planning, code review & documentation',
      url: 'https://github.com/bgauryy/octocode-mcp/tree/main/skills',
    },
    {
      id: 'anthropic-skills',
      name: 'Anthropic Official',
      type: 'github',
      owner: 'anthropics',
      repo: 'skills',
      branch: 'main',
      skillsPath: 'skills',
      skillPattern: 'skill-folders',
      description: 'Official Anthropic skills — artifacts, design & docs',
      url: 'https://github.com/anthropics/skills/tree/main/skills',
    },
  ],
  getMarketplaceById: vi.fn(),
  getMarketplaceCount: vi.fn().mockReturnValue(2),
  isLocalSource: vi.fn().mockReturnValue(false),
  getLocalMarketplaces: vi.fn().mockReturnValue([]),
  getGitHubMarketplaces: vi.fn().mockReturnValue([]),
  fetchMarketplaceStars: vi.fn().mockResolvedValue(null),
  fetchAllMarketplaceStars: vi.fn().mockResolvedValue(new Map()),
  clearStarsCache: vi.fn(),
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

    octocodePublicMocks.prepareDirectToolInputFromJsonText.mockReturnValue({});
    octocodePublicMocks.executeDirectTool.mockResolvedValue({
      isError: false,
      content: [],
      structuredContent: { results: [] },
    });

    fsReadMocks.fileExists.mockReturnValue(false);
    fsReadMocks.readFileContent.mockReturnValue(null);
    skillsFetchMocks.readSkillFromGitHub.mockResolvedValue('');
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

  it('list: --json outputs structured targets array', async () => {
    fsUtilsMocks.listSubdirectories.mockReturnValue([
      'my-skill',
      'other-skill',
    ]);
    fsUtilsMocks.dirExists.mockReturnValue(true);

    const skillsCommand = await loadCommand();
    await skillsCommand.handler({
      command: 'skills',
      args: ['list'],
      options: { json: true },
    });

    const jsonLine = consoleSpy.mock.calls.flat().find((line: unknown) => {
      if (typeof line !== 'string') return false;
      try {
        JSON.parse(line);
        return true;
      } catch {
        return false;
      }
    });
    expect(jsonLine).toBeDefined();
    const parsed = JSON.parse(jsonLine as string);
    expect(Array.isArray(parsed.targets)).toBe(true);
  });

  it('list: shows skills on OS grouped by target', async () => {
    const skillsCommand = await loadCommand();
    await skillsCommand.handler({
      command: 'skills',
      args: ['list'],
      options: {},
    });
    expect(consoleSpy).toHaveBeenCalledWith(
      expect.stringContaining('Skills on OS')
    );
    expect(
      consoleSpy.mock.calls.some((c: unknown[]) =>
        String(c[0]).includes('octocode-research')
      )
    ).toBe(true);
  });

  it('list: shows (no skills installed) when target dir is empty', async () => {
    fsUtilsMocks.listSubdirectories.mockReturnValue([]);
    const skillsCommand = await loadCommand();
    await skillsCommand.handler({
      command: 'skills',
      args: ['list'],
      options: {},
    });
    expect(consoleSpy).toHaveBeenCalledWith(
      expect.stringContaining('no skills installed')
    );
  });

  it('list: shows (directory not found) when target dir does not exist', async () => {
    fsUtilsMocks.dirExists.mockImplementation((p: string) =>
      p === '/fake/skills/src' ? true : false
    );
    const skillsCommand = await loadCommand();
    await skillsCommand.handler({
      command: 'skills',
      args: ['list'],
      options: {},
    });
    expect(consoleSpy).toHaveBeenCalledWith(
      expect.stringContaining('directory not found')
    );
  });

  it('list: works without bundled source dir', async () => {
    fsUtilsMocks.dirExists.mockImplementation((path: string) =>
      path === '/fake/skills/src' ? false : true
    );

    const skillsCommand = await loadCommand();
    await skillsCommand.handler({
      command: 'skills',
      args: ['list'],
      options: {},
    });
    expect(process.exitCode).toBeUndefined();
    expect(
      consoleSpy.mock.calls.some((call: unknown[]) =>
        String(call[0]).includes('Skills directory not found')
      )
    ).toBe(false);
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

  it('list: Windows paths shown for claude-desktop target', async () => {
    platformFlags.isWindows = true;

    const skillsCommand = await loadCommand();
    await skillsCommand.handler({
      command: 'skills',
      args: ['list'],
      options: {},
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
      expect.stringContaining('Skills on OS')
    );
  });

  it('search: errors when no query provided', async () => {
    const skillsCommand = await loadCommand();
    await skillsCommand.handler({
      command: 'skills',
      args: ['search'],
      options: {},
    });
    expect(process.exitCode).toBe(1);
    expect(consoleSpy).toHaveBeenCalledWith(
      expect.stringContaining('Missing search query')
    );
  });

  it('search: outputs protocol URL and query in human mode', async () => {
    const skillsCommand = await loadCommand();
    await skillsCommand.handler({
      command: 'skills',
      args: ['search', 'code review'],
      options: {},
    });

    expect(process.exitCode).toBeUndefined();
    const output = consoleSpy.mock.calls.flat().join('\n');
    expect(output).toContain('code review');
    expect(output).toContain('bgauryy/octocode');
    expect(output).toContain('SKILL.md');
    expect(output).toContain('references');
  });

  it('search: --json returns protocol instruction with query and URLs', async () => {
    const skillsCommand = await loadCommand();
    await skillsCommand.handler({
      command: 'skills',
      args: ['search', 'security audit'],
      options: { json: true },
    });

    const jsonLine = consoleSpy.mock.calls.flat().find((line: unknown) => {
      if (typeof line !== 'string') return false;
      try {
        JSON.parse(line);
        return true;
      } catch {
        return false;
      }
    });
    expect(jsonLine).toBeDefined();
    const parsed = JSON.parse(jsonLine as string);
    expect(parsed.query).toBe('security audit');
    expect(parsed.instruction).toContain('Read the skill protocol');
    expect(parsed.skillProtocol.url).toContain('SKILL.md');
    expect(parsed.skillProtocol.raw).toContain('raw.githubusercontent.com');
    expect(parsed.skillProtocol.references).toContain('references');
  });

  it('search: errors when no query provided', async () => {
    const skillsCommand = await loadCommand();
    await skillsCommand.handler({
      command: 'skills',
      args: ['search'],
      options: {},
    });

    expect(process.exitCode).toBe(1);
    const output = consoleSpy.mock.calls.flat().join('\n');
    expect(output).toContain('Missing search query');
  });

  it('search --direct: shows grouped results from skills.sh', async () => {
    skillsFetchMocks.fetchSkillsShSearch.mockResolvedValueOnce({
      results: [
        {
          id: 'a/b/skill-one',
          skillId: 'skill-one',
          name: 'skill-one',
          installs: 500,
          source: 'owner-a/repo-a',
        },
        {
          id: 'a/b/skill-two',
          skillId: 'skill-two',
          name: 'skill-two',
          installs: 300,
          source: 'owner-a/repo-a',
        },
        {
          id: 'c/d/skill-x',
          skillId: 'skill-x',
          name: 'skill-x',
          installs: 800,
          source: 'owner-b/repo-b',
        },
      ],
      count: 3,
    });

    const skillsCommand = await loadCommand();
    await skillsCommand.handler({
      command: 'skills',
      args: ['search', 'testing'],
      options: { direct: true },
    });

    expect(process.exitCode).toBeUndefined();
    const output = consoleSpy.mock.calls.flat().join('\n');
    expect(output).toContain('owner-b/repo-b');
    expect(output).toContain('owner-a/repo-a');
    expect(output).toContain('skill-one');
    expect(output).toContain('octocode skills read');
  });

  it('search --direct --json: returns structured results array', async () => {
    skillsFetchMocks.fetchSkillsShSearch.mockResolvedValueOnce({
      results: [
        {
          id: 'a/b/my-skill',
          skillId: 'my-skill',
          name: 'my-skill',
          installs: 42,
          source: 'owner/repo',
        },
      ],
      count: 1,
    });

    const skillsCommand = await loadCommand();
    await skillsCommand.handler({
      command: 'skills',
      args: ['search', 'auth'],
      options: { direct: true, json: true },
    });

    const jsonLine = consoleSpy.mock.calls.flat().find((l: unknown) => {
      if (typeof l !== 'string') return false;
      try {
        JSON.parse(l);
        return true;
      } catch {
        return false;
      }
    });
    expect(jsonLine).toBeDefined();
    const parsed = JSON.parse(jsonLine as string);
    expect(parsed.query).toBe('auth');
    expect(parsed.source).toBe('skills.sh');
    expect(parsed.results[0].name).toBe('my-skill');
    expect(parsed.results[0].readCmd).toContain('skills read');
  });

  it('search --direct: shows warning when skills.sh fails', async () => {
    skillsFetchMocks.fetchSkillsShSearch.mockRejectedValueOnce(
      new Error('Timeout')
    );

    const skillsCommand = await loadCommand();
    await skillsCommand.handler({
      command: 'skills',
      args: ['search', 'deploy'],
      options: { direct: true },
    });

    expect(process.exitCode).toBeUndefined();
    const output = consoleSpy.mock.calls.flat().join('\n');
    expect(output).toContain('unavailable');
  });

  it('search --direct: shows no-results message when empty', async () => {
    skillsFetchMocks.fetchSkillsShSearch.mockResolvedValueOnce({
      results: [],
      count: 0,
    });

    const skillsCommand = await loadCommand();
    await skillsCommand.handler({
      command: 'skills',
      args: ['search', 'xyznothing'],
      options: { direct: true },
    });

    expect(process.exitCode).toBeUndefined();
    const output = consoleSpy.mock.calls.flat().join('\n');
    expect(output).toContain('No results');
  });

  it('read: errors when no path provided', async () => {
    const skillsCommand = await loadCommand();
    await skillsCommand.handler({
      command: 'skills',
      args: ['read'],
      options: {},
    });
    expect(process.exitCode).toBe(1);
    expect(consoleSpy).toHaveBeenCalledWith(
      expect.stringContaining('Missing path')
    );
  });

  it('read: reads local SKILL.md', async () => {
    const content = `---
name: my-skill
description: A test skill
---
# My Skill
This is the skill content.`;

    fsReadMocks.fileExists.mockReturnValue(true);
    fsReadMocks.readFileContent.mockReturnValue(content);

    const skillsCommand = await loadCommand();
    await skillsCommand.handler({
      command: 'skills',
      args: ['read', '/fake/skills/my-skill/SKILL.md'],
      options: {},
    });

    expect(process.exitCode).toBeUndefined();
    const output = consoleSpy.mock.calls.flat().join('\n');
    expect(output).toContain('My Skill');
    expect(output).toContain('This is the skill content.');
  });

  it('read: truncates long content and shows "use --full" hint in non-json mode', async () => {
    const longContent = '# Skill\n' + 'x'.repeat(4000);

    fsReadMocks.fileExists.mockReturnValue(true);
    fsReadMocks.readFileContent.mockReturnValue(longContent);

    const skillsCommand = await loadCommand();
    await skillsCommand.handler({
      command: 'skills',
      args: ['read', '/fake/skills/my-skill'],
      options: {},
    });

    const output = consoleSpy.mock.calls.flat().join('\n');
    expect(output).toContain('--full');
  });

  it('read: local --json output', async () => {
    const content = `---
name: my-skill
description: A test skill
---
# My Skill`;

    fsReadMocks.fileExists.mockReturnValue(true);
    fsReadMocks.readFileContent.mockReturnValue(content);

    const skillsCommand = await loadCommand();
    await skillsCommand.handler({
      command: 'skills',
      args: ['read', '/fake/skills/my-skill'],
      options: { json: true },
    });

    const jsonLine = consoleSpy.mock.calls.flat().find((line: unknown) => {
      if (typeof line !== 'string') return false;
      try {
        JSON.parse(line);
        return true;
      } catch {
        return false;
      }
    });
    expect(jsonLine).toBeDefined();
    const parsed = JSON.parse(jsonLine as string);
    expect(parsed.success).toBe(true);
    expect(parsed.content).toContain('# My Skill');
  });

  it('read: errors when local SKILL.md not found', async () => {
    fsReadMocks.fileExists.mockReturnValue(false);

    const skillsCommand = await loadCommand();
    await skillsCommand.handler({
      command: 'skills',
      args: ['read', '/fake/skills/missing-skill'],
      options: {},
    });

    expect(process.exitCode).toBe(1);
  });

  it('read: reads GitHub path (owner/repo/path)', async () => {
    const content = `---
name: langchain-rag
description: RAG pipelines with LangChain
---
# LangChain RAG`;
    skillsFetchMocks.readSkillFromGitHub.mockResolvedValueOnce(content);

    const skillsCommand = await loadCommand();
    await skillsCommand.handler({
      command: 'skills',
      args: ['read', 'langchain-ai/langchain-skills/skills/langchain-rag'],
      options: {},
    });

    expect(process.exitCode).toBeUndefined();
    expect(skillsFetchMocks.readSkillFromGitHub).toHaveBeenCalledWith(
      'langchain-ai',
      'langchain-skills',
      'skills/langchain-rag',
      'main'
    );
    const output = consoleSpy.mock.calls.flat().join('\n');
    expect(output).toContain('langchain-rag');
  });

  it('read: GitHub --json output', async () => {
    const content = `---
name: langchain-rag
description: RAG pipelines
---
# LangChain RAG\nContent here.`;
    skillsFetchMocks.readSkillFromGitHub.mockResolvedValueOnce(content);

    const skillsCommand = await loadCommand();
    await skillsCommand.handler({
      command: 'skills',
      args: ['read', 'langchain-ai/langchain-skills/skills/langchain-rag'],
      options: { json: true },
    });

    const jsonLine = consoleSpy.mock.calls.flat().find((line: unknown) => {
      if (typeof line !== 'string') return false;
      try {
        JSON.parse(line);
        return true;
      } catch {
        return false;
      }
    });
    expect(jsonLine).toBeDefined();
    const parsed = JSON.parse(jsonLine as string);
    expect(parsed.success).toBe(true);
    expect(parsed.source).toContain('langchain-ai/langchain-skills');
  });

  it('read: handles GitHub fetch error', async () => {
    skillsFetchMocks.readSkillFromGitHub.mockRejectedValueOnce(
      new Error('SKILL.md not found')
    );

    const skillsCommand = await loadCommand();
    await skillsCommand.handler({
      command: 'skills',
      args: ['read', 'owner/repo/skills/missing'],
      options: {},
    });

    expect(process.exitCode).toBe(1);
  });

  it('read: --json outputs error when fetch fails', async () => {
    skillsFetchMocks.readSkillFromGitHub.mockRejectedValueOnce(
      new Error('SKILL.md not found')
    );

    const skillsCommand = await loadCommand();
    await skillsCommand.handler({
      command: 'skills',
      args: ['read', 'owner/repo/skills/missing'],
      options: { json: true },
    });

    expect(process.exitCode).toBe(1);
    const jsonLine = consoleSpy.mock.calls.flat().find((line: unknown) => {
      if (typeof line !== 'string') return false;
      try {
        JSON.parse(line);
        return true;
      } catch {
        return false;
      }
    });
    expect(jsonLine).toBeDefined();
    const parsed = JSON.parse(jsonLine as string);
    expect(parsed.success).toBe(false);
  });

  it('read: shows description line when frontmatter has description', async () => {
    const { parseSkillFrontmatter } =
      await import('../../../src/utils/parsers/frontmatter.js');
    vi.mocked(parseSkillFrontmatter).mockReturnValueOnce({
      name: 'my-skill',
      description: 'A useful skill',
    });

    const content = '# My Skill\nContent here.';
    fsReadMocks.fileExists.mockReturnValue(true);
    fsReadMocks.readFileContent.mockReturnValue(content);

    const skillsCommand = await loadCommand();
    await skillsCommand.handler({
      command: 'skills',
      args: ['read', '/fake/skills/my-skill'],
      options: {},
    });

    const output = consoleSpy.mock.calls.flat().join('\n');
    expect(output).toContain('A useful skill');
  });

  it('install --local: installs from a local path', async () => {
    fsReadMocks.fileExists.mockReturnValue(true);
    fsUtilsMocks.copyDirectory.mockReturnValue(true);
    fsUtilsMocks.dirExists.mockReturnValue(true);

    const skillsCommand = await loadCommand();
    await skillsCommand.handler({
      command: 'skills',
      args: ['install'],
      options: {
        local: '/fake/custom-skills/my-skill',
        targets: 'claude-code',
        json: true,
      },
    });

    expect(fsUtilsMocks.copyDirectory).toHaveBeenCalled();
    const jsonLine = consoleSpy.mock.calls.flat().find((line: unknown) => {
      if (typeof line !== 'string') return false;
      try {
        JSON.parse(line);
        return true;
      } catch {
        return false;
      }
    });
    expect(jsonLine).toBeDefined();
    const parsed = JSON.parse(jsonLine as string);
    expect(parsed.skill).toBe('my-skill');
  });

  it('install --local: non-json failure path sets exitCode', async () => {
    fsReadMocks.fileExists.mockReturnValue(true);
    fsUtilsMocks.dirExists.mockReturnValue(true);
    fsUtilsMocks.copyDirectory.mockReturnValue(false);

    const skillsCommand = await loadCommand();
    await skillsCommand.handler({
      command: 'skills',
      args: ['install'],
      options: {
        local: '/fake/custom-skills/my-skill',
        targets: 'claude-code',
      },
    });

    expect(process.exitCode).toBe(1);
  });

  it('install --local: non-json success shows installed message', async () => {
    fsReadMocks.fileExists.mockReturnValue(true);
    fsUtilsMocks.copyDirectory.mockReturnValue(true);
    fsUtilsMocks.dirExists.mockReturnValue(true);

    const skillsCommand = await loadCommand();
    await skillsCommand.handler({
      command: 'skills',
      args: ['install'],
      options: {
        local: '/fake/custom-skills/my-skill',
        targets: 'claude-code',
      },
    });

    const output = consoleSpy.mock.calls.flat().join('\n');
    expect(output).toContain('Installed to');
  });

  it('install --local: non-json SKILL.md not found shows error message', async () => {
    fsReadMocks.fileExists.mockReturnValue(false);

    const skillsCommand = await loadCommand();
    await skillsCommand.handler({
      command: 'skills',
      args: ['install'],
      options: {
        local: '/fake/missing-skill',
        targets: 'claude-code',
      },
    });

    expect(process.exitCode).toBe(1);
    const output = consoleSpy.mock.calls.flat().join('\n');
    expect(output).toContain('SKILL.md not found');
  });

  it('install --local: errors when SKILL.md not found', async () => {
    fsReadMocks.fileExists.mockReturnValue(false);

    const skillsCommand = await loadCommand();
    await skillsCommand.handler({
      command: 'skills',
      args: ['install'],
      options: {
        local: '/fake/missing-skill',
        targets: 'claude-code',
        json: true,
      },
    });

    expect(process.exitCode).toBe(1);
    const jsonLine = consoleSpy.mock.calls.flat().find((line: unknown) => {
      if (typeof line !== 'string') return false;
      try {
        JSON.parse(line);
        return true;
      } catch {
        return false;
      }
    });
    expect(jsonLine).toBeDefined();
    const parsed = JSON.parse(jsonLine as string);
    expect(parsed.success).toBe(false);
  });

  it('install --skill: --json outputs structured result for named skill', async () => {
    fsUtilsMocks.copyDirectory.mockReturnValue(true);
    fsUtilsMocks.dirExists.mockReturnValue(true);
    fsUtilsMocks.listSubdirectories.mockReturnValue([
      'octocode-engineer',
      'octocode-roast',
    ]);

    const skillsCommand = await loadCommand();
    await skillsCommand.handler({
      command: 'skills',
      args: ['install'],
      options: {
        skill: 'octocode-engineer',
        targets: 'claude-code',
        json: true,
      },
    });

    const jsonLine = consoleSpy.mock.calls.flat().find((line: unknown) => {
      if (typeof line !== 'string') return false;
      try {
        JSON.parse(line);
        return true;
      } catch {
        return false;
      }
    });
    expect(jsonLine).toBeDefined();
    const parsed = JSON.parse(jsonLine as string);
    expect(parsed.skill).toBe('octocode-engineer');
    expect(typeof parsed.installed).toBe('number');
  });

  it('install: --json outputs structured result for all-skills install', async () => {
    fsUtilsMocks.copyDirectory.mockReturnValue(true);
    fsUtilsMocks.dirExists.mockReturnValue(true);

    const skillsCommand = await loadCommand();
    await skillsCommand.handler({
      command: 'skills',
      args: ['install'],
      options: { targets: 'claude-code', json: true },
    });

    const jsonLine = consoleSpy.mock.calls.flat().find((line: unknown) => {
      if (typeof line !== 'string') return false;
      try {
        JSON.parse(line);
        return true;
      } catch {
        return false;
      }
    });
    expect(jsonLine).toBeDefined();
    const parsed = JSON.parse(jsonLine as string);
    expect(typeof parsed.installed).toBe('number');
    expect(Array.isArray(parsed.targets)).toBe(true);
  });

  it('remove: invalid skill name --json outputs error', async () => {
    const skillsCommand = await loadCommand();
    await skillsCommand.handler({
      command: 'skills',
      args: ['remove'],
      options: { skill: '../evil', targets: 'claude-code', json: true },
    });

    expect(process.exitCode).toBe(1);
    const jsonLine = consoleSpy.mock.calls.flat().find((line: unknown) => {
      if (typeof line !== 'string') return false;
      try {
        JSON.parse(line);
        return true;
      } catch {
        return false;
      }
    });
    expect(jsonLine).toBeDefined();
    const parsed = JSON.parse(jsonLine as string);
    expect(parsed.failed).toBe(1);
  });

  it('remove: invalid skill name without --json shows error message', async () => {
    const skillsCommand = await loadCommand();
    await skillsCommand.handler({
      command: 'skills',
      args: ['remove'],
      options: { skill: '../evil', targets: 'claude-code' },
    });

    expect(process.exitCode).toBe(1);
    const output = consoleSpy.mock.calls.flat().join('\n');
    expect(output).toContain('Invalid skill name');
  });

  it('remove: --json outputs structured result', async () => {
    fsUtilsMocks.removeDirectory.mockReturnValue(true);

    const skillsCommand = await loadCommand();
    await skillsCommand.handler({
      command: 'skills',
      args: ['remove'],
      options: { skill: 'octocode-engineer', targets: 'cursor', json: true },
    });

    const jsonLine = consoleSpy.mock.calls.flat().find((line: unknown) => {
      if (typeof line !== 'string') return false;
      try {
        JSON.parse(line);
        return true;
      } catch {
        return false;
      }
    });
    expect(jsonLine).toBeDefined();
    const parsed = JSON.parse(jsonLine as string);
    expect(typeof parsed.removed).toBe('number');
    expect(parsed.skill).toBe('octocode-engineer');
  });

  it('remove --local: derives skill name from path', async () => {
    const skillsCommand = await loadCommand();
    await skillsCommand.handler({
      command: 'skills',
      args: ['remove'],
      options: {
        local: '/fake/skills/octocode-engineer',
        targets: 'claude-code',
      },
    });

    const output = consoleSpy.mock.calls.flat().join('\n');
    expect(output).toContain('octocode-engineer');
  });

  function findJsonLine(): unknown {
    return consoleSpy.mock.calls.flat().find((line: unknown) => {
      if (typeof line !== 'string') return false;
      try {
        JSON.parse(line);
        return true;
      } catch {
        return false;
      }
    });
  }

  it('read: parses full GitHub tree URL with path', async () => {
    const content = '# Skill\nContent.';
    skillsFetchMocks.readSkillFromGitHub.mockResolvedValueOnce(content);

    const skillsCommand = await loadCommand();
    await skillsCommand.handler({
      command: 'skills',
      args: [
        'read',
        'https://github.com/owner/repo/tree/dev/skills/my-skill/SKILL.md',
      ],
      options: {},
    });

    expect(skillsFetchMocks.readSkillFromGitHub).toHaveBeenCalledWith(
      'owner',
      'repo',
      'skills/my-skill',
      'dev'
    );
    expect(process.exitCode).toBeUndefined();
  });

  it('read: parses bare GitHub repo URL (no path, defaults main)', async () => {
    skillsFetchMocks.readSkillFromGitHub.mockResolvedValueOnce('# Repo skill');

    const skillsCommand = await loadCommand();
    await skillsCommand.handler({
      command: 'skills',
      args: ['read', 'https://github.com/owner/repo'],
      options: {},
    });

    expect(skillsFetchMocks.readSkillFromGitHub).toHaveBeenCalledWith(
      'owner',
      'repo',
      '',
      'main'
    );
    const output = consoleSpy.mock.calls.flat().join('\n');
    expect(output).toContain('repo');
  });

  it('read: unparseable GitHub URL errors', async () => {
    const skillsCommand = await loadCommand();
    await skillsCommand.handler({
      command: 'skills',
      args: ['read', 'https://github.com/'],
      options: {},
    });

    expect(process.exitCode).toBe(1);
    expect(consoleSpy).toHaveBeenCalledWith(
      expect.stringContaining('Cannot parse path')
    );
  });

  it('read: expands ~/ local path', async () => {
    fsReadMocks.fileExists.mockReturnValue(true);
    fsReadMocks.readFileContent.mockReturnValue('# Home skill');

    const skillsCommand = await loadCommand();
    await skillsCommand.handler({
      command: 'skills',
      args: ['read', '~/my-skill'],
      options: {},
    });

    expect(process.exitCode).toBeUndefined();
    const output = consoleSpy.mock.calls.flat().join('\n');
    expect(output).toContain('Home skill');
  });

  it('read: --local flag provides path when no positional arg', async () => {
    fsReadMocks.fileExists.mockReturnValue(true);
    fsReadMocks.readFileContent.mockReturnValue('# Via local flag');

    const skillsCommand = await loadCommand();
    await skillsCommand.handler({
      command: 'skills',
      args: ['read'],
      options: { local: '/fake/skills/my-skill' },
    });

    expect(process.exitCode).toBeUndefined();
    const output = consoleSpy.mock.calls.flat().join('\n');
    expect(output).toContain('Via local flag');
  });

  it('read: empty content from GitHub treated as error', async () => {
    skillsFetchMocks.readSkillFromGitHub.mockResolvedValueOnce('');

    const skillsCommand = await loadCommand();
    await skillsCommand.handler({
      command: 'skills',
      args: ['read', 'owner/repo/skills/empty'],
      options: {},
    });

    expect(process.exitCode).toBe(1);
    expect(consoleSpy).toHaveBeenCalledWith(
      expect.stringContaining('Empty content')
    );
  });

  function directInstallResults() {
    return {
      results: [
        {
          id: 'owner/repo/top-skill',
          skillId: 'top-skill',
          name: 'top-skill',
          installs: 900,
          source: 'owner/repo',
        },
        {
          id: 'owner/repo/other-skill',
          skillId: 'other-skill',
          name: 'other-skill',
          installs: 100,
          source: 'owner/repo',
        },
      ],
      count: 2,
    };
  }

  it('search --direct --install: installs top result to targets', async () => {
    skillsFetchMocks.fetchSkillsShSearch.mockResolvedValueOnce(
      directInstallResults()
    );
    skillsFetchMocks.readSkillFromGitHub.mockResolvedValueOnce('# Top skill');
    fsUtilsMocks.dirExists.mockImplementation((p: string) =>
      p === '/fake/skills/src' ? true : false
    );

    const skillsCommand = await loadCommand();
    await skillsCommand.handler({
      command: 'skills',
      args: ['search', 'thing'],
      options: { direct: true, install: true, targets: 'claude-code' },
    });

    expect(skillsFetchMocks.readSkillFromGitHub).toHaveBeenCalledWith(
      'owner',
      'repo',
      'top-skill/SKILL.md',
      'main'
    );
    const fs = await import('node:fs');
    expect(fs.writeFileSync).toHaveBeenCalled();
    const output = consoleSpy.mock.calls.flat().join('\n');
    expect(output).toContain('Auto-installing top result');
    expect(output).toContain('Installed top-skill');
  });

  it('search --direct --install: skips existing target without --force', async () => {
    skillsFetchMocks.fetchSkillsShSearch.mockResolvedValueOnce(
      directInstallResults()
    );
    skillsFetchMocks.readSkillFromGitHub.mockResolvedValueOnce('# Top skill');
    fsUtilsMocks.dirExists.mockReturnValue(true);

    const skillsCommand = await loadCommand();
    await skillsCommand.handler({
      command: 'skills',
      args: ['search', 'thing'],
      options: { direct: true, install: true, targets: 'claude-code' },
    });

    const fs = await import('node:fs');
    expect(fs.writeFileSync).not.toHaveBeenCalled();
    const output = consoleSpy.mock.calls.flat().join('\n');
    expect(output).toContain('Already installed in all targets');
  });

  it('search --direct --install: --force overwrites existing', async () => {
    skillsFetchMocks.fetchSkillsShSearch.mockResolvedValueOnce(
      directInstallResults()
    );
    skillsFetchMocks.readSkillFromGitHub.mockResolvedValueOnce('# Top skill');
    fsUtilsMocks.dirExists.mockReturnValue(true);

    const skillsCommand = await loadCommand();
    await skillsCommand.handler({
      command: 'skills',
      args: ['search', 'thing'],
      options: {
        direct: true,
        install: true,
        force: true,
        targets: 'claude-code',
      },
    });

    const fs = await import('node:fs');
    expect(fs.writeFileSync).toHaveBeenCalled();
    const output = consoleSpy.mock.calls.flat().join('\n');
    expect(output).toContain('Installed top-skill');
  });

  it('search --direct --install: write failure sets exit code', async () => {
    skillsFetchMocks.fetchSkillsShSearch.mockResolvedValueOnce(
      directInstallResults()
    );
    skillsFetchMocks.readSkillFromGitHub.mockResolvedValueOnce('# Top skill');
    fsUtilsMocks.dirExists.mockImplementation((p: string) =>
      p === '/fake/skills/src' ? true : false
    );
    (fsMocks.mkdirSync as ReturnType<typeof vi.fn>).mockImplementationOnce(
      () => {
        throw new Error('mkdir failed');
      }
    );

    const skillsCommand = await loadCommand();
    await skillsCommand.handler({
      command: 'skills',
      args: ['search', 'thing'],
      options: { direct: true, install: true, targets: 'claude-code' },
    });

    expect(process.exitCode).toBe(1);
    const output = consoleSpy.mock.calls.flat().join('\n');
    expect(output).toContain('Failed to write');
  });

  it('search --direct --install: fetch failure shows cannot-fetch message', async () => {
    skillsFetchMocks.fetchSkillsShSearch.mockResolvedValueOnce(
      directInstallResults()
    );
    skillsFetchMocks.readSkillFromGitHub.mockRejectedValueOnce(
      new Error('boom')
    );

    const skillsCommand = await loadCommand();
    await skillsCommand.handler({
      command: 'skills',
      args: ['search', 'thing'],
      options: { direct: true, install: true, targets: 'claude-code' },
    });

    const output = consoleSpy.mock.calls.flat().join('\n');
    expect(output).toContain('Could not fetch skill');
  });

  it('search --direct: results with zero installs omit installs suffix', async () => {
    skillsFetchMocks.fetchSkillsShSearch.mockResolvedValueOnce({
      results: [
        {
          id: 'owner/repo/zero-skill',
          skillId: 'zero-skill',
          name: 'zero-skill',
          installs: 0,
          source: 'owner/repo',
        },
      ],
      count: 1,
    });

    const skillsCommand = await loadCommand();
    await skillsCommand.handler({
      command: 'skills',
      args: ['search', 'thing'],
      options: { direct: true },
    });

    const output = consoleSpy.mock.calls.flat().join('\n');
    expect(output).toContain('zero-skill');
  });

  it('search: clamps limit from string option', async () => {
    skillsFetchMocks.fetchSkillsShSearch.mockResolvedValueOnce({
      results: [],
      count: 0,
    });

    const skillsCommand = await loadCommand();
    await skillsCommand.handler({
      command: 'skills',
      args: ['search', 'thing'],
      options: { direct: true, limit: '5' },
    });

    expect(skillsFetchMocks.fetchSkillsShSearch).toHaveBeenCalledWith(
      'thing',
      5
    );
  });

  it('list: valid --target filters to one target', async () => {
    const skillsCommand = await loadCommand();
    await skillsCommand.handler({
      command: 'skills',
      args: ['list'],
      options: { target: 'cursor' },
    });

    expect(process.exitCode).toBeUndefined();
    const output = consoleSpy.mock.calls.flat().join('\n');
    expect(output).toContain('cursor');
    expect(output).not.toContain('claude-desktop');
  });

  it('list: invalid --target errors (non-json)', async () => {
    const skillsCommand = await loadCommand();
    await skillsCommand.handler({
      command: 'skills',
      args: ['list'],
      options: { target: 'bogus' },
    });

    expect(process.exitCode).toBe(1);
    expect(consoleSpy).toHaveBeenCalledWith(
      expect.stringContaining('Invalid --target')
    );
  });

  it('list: invalid --target errors (json)', async () => {
    const skillsCommand = await loadCommand();
    await skillsCommand.handler({
      command: 'skills',
      args: ['list'],
      options: { target: 'bogus', json: true },
    });

    expect(process.exitCode).toBe(1);
    const parsed = JSON.parse(findJsonLine() as string);
    expect(parsed.error).toContain('Invalid target');
  });

  it('list: truncates description longer than 200 chars', async () => {
    const { getSkillMetadata } = skillsUtilsMocks;
    (getSkillMetadata as ReturnType<typeof vi.fn>).mockReturnValue({
      name: 'big-skill',
      description: 'd'.repeat(300),
    });
    fsUtilsMocks.listSubdirectories.mockReturnValue(['big-skill']);

    const skillsCommand = await loadCommand();
    await skillsCommand.handler({
      command: 'skills',
      args: ['list'],
      options: { target: 'cursor' },
    });

    const output = consoleSpy.mock.calls.flat().join('\n');
    expect(output).toContain('…');
  });

  it('install --dry-run: non-json shows plan with install/skip/overwrite', async () => {
    fsUtilsMocks.listSubdirectories.mockReturnValue(['skill-a']);
    fsUtilsMocks.dirExists.mockImplementation((p: string) => {
      if (p === '/fake/skills/src') return true;
      return p.endsWith('skill-a');
    });

    const skillsCommand = await loadCommand();
    await skillsCommand.handler({
      command: 'skills',
      args: ['install'],
      options: { 'dry-run': true, targets: 'claude-code', mode: 'copy' },
    });

    expect(process.exitCode).toBeUndefined();
    const output = consoleSpy.mock.calls.flat().join('\n');
    expect(output).toContain('DRY RUN');
    expect(output).toContain('skip (exists)');
    expect(output).toContain('Remove --dry-run to apply.');
    expect(fsUtilsMocks.copyDirectory).not.toHaveBeenCalled();
  });

  it('install --dry-run --force: shows overwrite for existing', async () => {
    fsUtilsMocks.listSubdirectories.mockReturnValue(['skill-a']);
    fsUtilsMocks.dirExists.mockImplementation((p: string) => {
      if (p === '/fake/skills/src') return true;
      return p.endsWith('skill-a');
    });

    const skillsCommand = await loadCommand();
    await skillsCommand.handler({
      command: 'skills',
      args: ['install'],
      options: {
        'dry-run': true,
        force: true,
        targets: 'claude-code',
        mode: 'copy',
      },
    });

    const output = consoleSpy.mock.calls.flat().join('\n');
    expect(output).toContain('overwrite');
  });

  it('install --dry-run --json: outputs plan', async () => {
    fsUtilsMocks.listSubdirectories.mockReturnValue(['skill-a']);

    const skillsCommand = await loadCommand();
    await skillsCommand.handler({
      command: 'skills',
      args: ['install'],
      options: {
        'dry-run': true,
        targets: 'claude-code',
        mode: 'copy',
        json: true,
      },
    });

    const parsed = JSON.parse(findJsonLine() as string);
    expect(parsed.dryRun).toBe(true);
    expect(Array.isArray(parsed.plan)).toBe(true);
  });

  it('install --json: no skills available outputs empty plan', async () => {
    fsUtilsMocks.listSubdirectories.mockReturnValue([]);

    const skillsCommand = await loadCommand();
    await skillsCommand.handler({
      command: 'skills',
      args: ['install'],
      options: { targets: 'claude-code', mode: 'copy', json: true },
    });

    const parsed = JSON.parse(findJsonLine() as string);
    expect(parsed.skills).toEqual([]);
    expect(Array.isArray(parsed.plan)).toBe(true);
  });

  it('sync: missing targets errors (non-json)', async () => {
    const skillsCommand = await loadCommand();
    await skillsCommand.handler({
      command: 'skills',
      args: ['sync'],
      options: {},
    });

    expect(process.exitCode).toBe(1);
    expect(consoleSpy).toHaveBeenCalledWith(
      expect.stringContaining('octocode skills sync')
    );
  });

  it('sync: missing targets errors (json)', async () => {
    const skillsCommand = await loadCommand();
    await skillsCommand.handler({
      command: 'skills',
      args: ['sync', 'cursor'],
      options: { json: true },
    });

    expect(process.exitCode).toBe(1);
    const parsed = JSON.parse(findJsonLine() as string);
    expect(parsed.success).toBe(false);
  });

  it('sync: invalid target errors (non-json)', async () => {
    const skillsCommand = await loadCommand();
    await skillsCommand.handler({
      command: 'skills',
      args: ['sync', 'bogus', 'cursor'],
      options: {},
    });

    expect(process.exitCode).toBe(1);
    expect(consoleSpy).toHaveBeenCalledWith(
      expect.stringContaining('Invalid target')
    );
  });

  it('sync: invalid target errors (json)', async () => {
    const skillsCommand = await loadCommand();
    await skillsCommand.handler({
      command: 'skills',
      args: ['sync', 'bogus', 'alsobad'],
      options: { json: true },
    });

    expect(process.exitCode).toBe(1);
    const parsed = JSON.parse(findJsonLine() as string);
    expect(parsed.error).toContain('Invalid target');
  });

  it('sync: source dir missing errors (non-json)', async () => {
    fsUtilsMocks.dirExists.mockImplementation((p: string) =>
      p === '/fake/skills/src' ? true : false
    );

    const skillsCommand = await loadCommand();
    await skillsCommand.handler({
      command: 'skills',
      args: ['sync', 'cursor', 'codex'],
      options: {},
    });

    expect(process.exitCode).toBe(1);
    expect(consoleSpy).toHaveBeenCalledWith(
      expect.stringContaining('Source target has no skills')
    );
  });

  it('sync: source dir missing errors (json)', async () => {
    fsUtilsMocks.dirExists.mockImplementation((p: string) =>
      p === '/fake/skills/src' ? true : false
    );

    const skillsCommand = await loadCommand();
    await skillsCommand.handler({
      command: 'skills',
      args: ['sync', 'cursor', 'codex'],
      options: { json: true },
    });

    expect(process.exitCode).toBe(1);
    const parsed = JSON.parse(findJsonLine() as string);
    expect(parsed.error).toContain('Source target has no skills');
  });

  it('sync: copies skills successfully (non-json)', async () => {
    fsUtilsMocks.dirExists.mockReturnValue(true);
    fsUtilsMocks.listSubdirectories.mockReturnValue(['skill-a', 'skill-b']);
    fsMocks.existsSync.mockReturnValue(false);
    fsUtilsMocks.copyDirectory.mockReturnValue(true);

    const skillsCommand = await loadCommand();
    await skillsCommand.handler({
      command: 'skills',
      args: ['sync', 'cursor', 'codex'],
      options: {},
    });

    expect(process.exitCode).toBeUndefined();
    expect(fsUtilsMocks.copyDirectory).toHaveBeenCalled();
    const output = consoleSpy.mock.calls.flat().join('\n');
    expect(output).toContain('Synced');
  });

  it('sync: skipped existing prints warning', async () => {
    fsUtilsMocks.dirExists.mockReturnValue(true);
    fsUtilsMocks.listSubdirectories.mockReturnValue(['skill-a']);
    fsMocks.existsSync.mockReturnValue(true);

    const skillsCommand = await loadCommand();
    await skillsCommand.handler({
      command: 'skills',
      args: ['sync', 'cursor', 'codex'],
      options: {},
    });

    const output = consoleSpy.mock.calls.flat().join('\n');
    expect(output).toContain('Skipped');
  });

  it('sync: copy failure sets exit code (non-json)', async () => {
    fsUtilsMocks.dirExists.mockReturnValue(true);
    fsUtilsMocks.listSubdirectories.mockReturnValue(['skill-a']);
    fsMocks.existsSync.mockReturnValue(false);
    fsUtilsMocks.copyDirectory.mockReturnValue(false);

    const skillsCommand = await loadCommand();
    await skillsCommand.handler({
      command: 'skills',
      args: ['sync', 'cursor', 'codex'],
      options: {},
    });

    expect(process.exitCode).toBe(1);
    const output = consoleSpy.mock.calls.flat().join('\n');
    expect(output).toContain('Failed');
  });

  it('sync: --json success output', async () => {
    fsUtilsMocks.dirExists.mockReturnValue(true);
    fsUtilsMocks.listSubdirectories.mockReturnValue(['skill-a']);
    fsMocks.existsSync.mockReturnValue(false);
    fsUtilsMocks.copyDirectory.mockReturnValue(true);

    const skillsCommand = await loadCommand();
    await skillsCommand.handler({
      command: 'skills',
      args: ['sync', 'cursor', 'codex'],
      options: { json: true },
    });

    const parsed = JSON.parse(findJsonLine() as string);
    expect(parsed.success).toBe(true);
    expect(parsed.from).toBe('cursor');
    expect(parsed.to).toBe('codex');
  });

  it('sync: --json failure sets exit code', async () => {
    fsUtilsMocks.dirExists.mockReturnValue(true);
    fsUtilsMocks.listSubdirectories.mockReturnValue(['skill-a']);
    fsMocks.existsSync.mockReturnValue(false);
    fsUtilsMocks.copyDirectory.mockReturnValue(false);

    const skillsCommand = await loadCommand();
    await skillsCommand.handler({
      command: 'skills',
      args: ['sync', 'cursor', 'codex'],
      options: { json: true },
    });

    expect(process.exitCode).toBe(1);
    const parsed = JSON.parse(findJsonLine() as string);
    expect(parsed.success).toBe(false);
  });

  it('sync --dry-run: non-json shows plan with copy/skip statuses', async () => {
    fsUtilsMocks.listSubdirectories.mockReturnValue(['skill-a', 'skill-b']);
    fsUtilsMocks.dirExists.mockImplementation((p: string) => {
      if (p === '/fake/skills/src') return true;
      if (p.includes('.cursor')) return true;
      return p.endsWith('skill-a');
    });

    const skillsCommand = await loadCommand();
    await skillsCommand.handler({
      command: 'skills',
      args: ['sync', 'cursor', 'codex'],
      options: { 'dry-run': true },
    });

    expect(process.exitCode).toBeUndefined();
    const output = consoleSpy.mock.calls.flat().join('\n');
    expect(output).toContain('DRY RUN');
    expect(fsUtilsMocks.copyDirectory).not.toHaveBeenCalled();
  });

  it('sync --dry-run --force: shows overwrite status', async () => {
    fsUtilsMocks.listSubdirectories.mockReturnValue(['skill-a']);
    fsUtilsMocks.dirExists.mockImplementation((p: string) => {
      if (p === '/fake/skills/src') return true;
      if (p.includes('.cursor')) return true;
      return p.endsWith('skill-a');
    });

    const skillsCommand = await loadCommand();
    await skillsCommand.handler({
      command: 'skills',
      args: ['sync', 'cursor', 'codex'],
      options: { 'dry-run': true, force: true },
    });

    const output = consoleSpy.mock.calls.flat().join('\n');
    expect(output).toContain('overwrite');
  });

  it('sync --dry-run --json: outputs plan', async () => {
    fsUtilsMocks.dirExists.mockReturnValue(true);
    fsUtilsMocks.listSubdirectories.mockReturnValue(['skill-a']);

    const skillsCommand = await loadCommand();
    await skillsCommand.handler({
      command: 'skills',
      args: ['sync', 'cursor', 'codex'],
      options: { 'dry-run': true, json: true },
    });

    const parsed = JSON.parse(findJsonLine() as string);
    expect(parsed.dryRun).toBe(true);
    expect(parsed.from).toBe('cursor');
  });

  it('TTY install claude-only preset copies to claude targets', async () => {
    setStdoutTTY(true);
    promptsMocks.select
      .mockResolvedValueOnce('claude-only')
      .mockResolvedValueOnce('copy');

    const skillsCommand = await loadCommand();
    await skillsCommand.handler({
      command: 'skills',
      args: ['install'],
      options: {},
    });

    expect(fsUtilsMocks.copyDirectory).toHaveBeenCalled();
    expect(process.exitCode).toBeUndefined();
  });

  it('TTY install custom with empty checkbox cancels', async () => {
    setStdoutTTY(true);
    promptsMocks.select
      .mockResolvedValueOnce('custom')
      .mockResolvedValueOnce('copy');
    promptsMocks.checkbox.mockResolvedValue([]);

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
});
