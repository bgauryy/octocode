import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { EXIT } from '../../../src/cli/exit-codes.js';

const skillMocks = vi.hoisted(() => ({
  getSkillsDirForTarget: vi.fn((target: string) => `/targets/${target}`),
  installSkillToDestination: vi.fn().mockReturnValue('installed'),
}));

vi.mock('@octocodeai/octocode-tools-core/paths', () => ({
  paths: {
    home: '/octocode-home',
    cliConfig: '/octocode-home/config.json',
  },
}));

vi.mock('../../../src/utils/platform.js', () => ({
  HOME: '/home/test',
  isWindows: false,
  getAppDataPath: vi.fn().mockReturnValue('/appdata'),
}));

vi.mock('../../../src/utils/skills.js', async importOriginal => {
  const actual =
    await importOriginal<typeof import('../../../src/utils/skills.js')>();
  return {
    ...actual,
    getSkillsDirForTarget: skillMocks.getSkillsDirForTarget,
    installSkillToDestination: skillMocks.installSkillToDestination,
  };
});

const fetchMocks = vi.hoisted(() => ({
  readSkillFromGitHub: vi.fn().mockResolvedValue('# Skill'),
  installMarketplaceSkill: vi.fn().mockResolvedValue({ success: true }),
}));

vi.mock('../../../src/utils/skills-fetch.js', () => ({
  readSkillFromGitHub: fetchMocks.readSkillFromGitHub,
  installMarketplaceSkill: fetchMocks.installMarketplaceSkill,
}));

const fsMocks = vi.hoisted(() => ({
  fileExists: vi.fn().mockReturnValue(true),
}));

vi.mock('../../../src/utils/fs.js', () => ({
  copyDirectory: vi.fn().mockReturnValue(true),
  dirExists: vi.fn().mockReturnValue(true),
  fileExists: fsMocks.fileExists,
  listSubdirectories: vi.fn().mockReturnValue([]),
  readFileContent: vi.fn().mockReturnValue(null),
}));

vi.mock('../../../src/utils/spinner.js', () => ({
  Spinner: vi.fn(function MockSpinner() {
    const instance = {
      start: vi.fn(),
      succeed: vi.fn(),
      fail: vi.fn(),
      update: vi.fn(),
    };
    instance.start.mockImplementation(() => instance);
    return instance;
  }),
}));

describe('skillCommand', () => {
  let consoleSpy: ReturnType<typeof vi.spyOn>;
  let originalExitCode: typeof process.exitCode;
  let ttyDescriptor: PropertyDescriptor | undefined;

  function setStdoutTTY(value: boolean): void {
    Object.defineProperty(process.stdout, 'isTTY', {
      configurable: true,
      enumerable: true,
      writable: true,
      value,
    });
  }

  async function loadCommand() {
    const mod = await import('../../../src/cli/commands/skill.js');
    return mod.skillCommand;
  }

  beforeEach(() => {
    vi.clearAllMocks();
    consoleSpy = vi.spyOn(console, 'log').mockImplementation(() => {});
    originalExitCode = process.exitCode;
    process.exitCode = undefined;
    ttyDescriptor = Object.getOwnPropertyDescriptor(process.stdout, 'isTTY');
    setStdoutTTY(false);
    fetchMocks.readSkillFromGitHub.mockResolvedValue('# Skill');
    fetchMocks.installMarketplaceSkill.mockResolvedValue({ success: true });
    skillMocks.installSkillToDestination.mockReturnValue('installed');
    fsMocks.fileExists.mockReturnValue(true);
  });

  afterEach(() => {
    consoleSpy.mockRestore();
    process.exitCode = originalExitCode;
    if (ttyDescriptor) {
      Object.defineProperty(process.stdout, 'isTTY', ttyDescriptor);
    } else {
      delete (process.stdout as { isTTY?: boolean }).isTTY;
    }
  });

  function jsonOutput(): Record<string, unknown> {
    const line = consoleSpy.mock.calls
      .flat()
      .find(
        (value: unknown) => typeof value === 'string' && value.startsWith('{')
      );
    expect(line).toBeDefined();
    return JSON.parse(line as string) as Record<string, unknown>;
  }

  it('adds a GitHub skill folder to selected platforms', async () => {
    const command = await loadCommand();

    await command.handler({
      command: 'skill',
      args: [],
      options: {
        add: 'https://github.com/owner/repo/tree/main/skills/review',
        platform: 'cursor,codex',
        json: true,
      },
    });

    expect(fetchMocks.readSkillFromGitHub).toHaveBeenCalledWith(
      'owner',
      'repo',
      'skills/review',
      'main'
    );
    expect(fetchMocks.installMarketplaceSkill).toHaveBeenCalledWith(
      expect.objectContaining({
        name: 'review',
        path: 'skills/review',
        source: expect.objectContaining({
          owner: 'owner',
          repo: 'repo',
          branch: 'main',
        }),
      }),
      expect.stringContaining('/octocode-home/skill-sources/')
    );
    expect(skillMocks.installSkillToDestination).toHaveBeenCalledWith(
      expect.objectContaining({
        destinationPath: '/targets/cursor/review',
        mode: 'copy',
      })
    );
    expect(skillMocks.installSkillToDestination).toHaveBeenCalledWith(
      expect.objectContaining({
        destinationPath: '/targets/codex/review',
        mode: 'copy',
      })
    );
    expect(jsonOutput()).toMatchObject({
      success: true,
      skill: 'review',
      platforms: ['cursor', 'codex'],
      installed: 2,
    });
  });

  it('adds a named Octocode skill from the canonical skills path', async () => {
    const command = await loadCommand();

    await command.handler({
      command: 'skill',
      args: [],
      options: {
        name: 'octocode-engineer',
        platform: 'common',
        json: true,
      },
    });

    expect(fetchMocks.readSkillFromGitHub).toHaveBeenCalledWith(
      'bgauryy',
      'octocode',
      'skills/octocode-engineer',
      'main'
    );
    expect(fetchMocks.installMarketplaceSkill).toHaveBeenCalledWith(
      expect.objectContaining({
        name: 'octocode-engineer',
        path: 'skills/octocode-engineer',
        source: expect.objectContaining({
          owner: 'bgauryy',
          repo: 'octocode',
          branch: 'main',
          skillsPath: 'skills',
        }),
      }),
      expect.stringContaining('/octocode-home/skill-sources/')
    );
    expect(skillMocks.installSkillToDestination).toHaveBeenCalledWith(
      expect.objectContaining({
        destinationPath: '/targets/agents/octocode-engineer',
        mode: 'copy',
      })
    );
    expect(jsonOutput()).toMatchObject({
      success: true,
      skill: 'octocode-engineer',
      source:
        'https://github.com/bgauryy/octocode/tree/main/skills/octocode-engineer',
      platforms: ['common'],
      installed: 1,
    });
  });

  it('requires an explicit platform for agent-safe installs', async () => {
    const command = await loadCommand();

    await command.handler({
      command: 'skill',
      args: [],
      options: {
        add: 'owner/repo/skills/review',
        json: true,
      },
    });

    expect(process.exitCode).toBe(EXIT.USAGE);
    expect(fetchMocks.installMarketplaceSkill).not.toHaveBeenCalled();
    expect(jsonOutput()).toMatchObject({
      success: false,
      error:
        'Missing required option: --platform <common|cursor|claude|codex|all>',
    });
  });

  it('expands Claude to Claude Code and Claude Desktop targets', async () => {
    const command = await loadCommand();

    await command.handler({
      command: 'skill',
      args: [],
      options: {
        add: 'owner/repo/skills/review',
        platform: 'claude',
        mode: 'symlink',
        json: true,
      },
    });

    expect(skillMocks.installSkillToDestination).toHaveBeenCalledWith(
      expect.objectContaining({
        destinationPath: '/targets/claude-code/review',
        mode: 'symlink',
      })
    );
    expect(skillMocks.installSkillToDestination).toHaveBeenCalledWith(
      expect.objectContaining({
        destinationPath: '/targets/claude-desktop/review',
        mode: 'symlink',
      })
    );
  });

  it('does not prompt for platform selection in a TTY', async () => {
    setStdoutTTY(true);
    const command = await loadCommand();

    await command.handler({
      command: 'skill',
      args: [],
      options: {
        add: 'owner/repo/skills/review',
      },
    });

    expect(process.exitCode).toBe(EXIT.USAGE);
    expect(fetchMocks.installMarketplaceSkill).not.toHaveBeenCalled();
  });

  it('rejects unsafe Octocode skill names', async () => {
    const command = await loadCommand();

    await command.handler({
      command: 'skill',
      args: [],
      options: {
        name: '../bad',
        platform: 'common',
        json: true,
      },
    });

    expect(process.exitCode).toBe(EXIT.USAGE);
    expect(fetchMocks.readSkillFromGitHub).not.toHaveBeenCalled();
    expect(fetchMocks.installMarketplaceSkill).not.toHaveBeenCalled();
    expect(jsonOutput()).toMatchObject({
      success: false,
      error: 'Invalid Octocode skill name',
    });
  });

  it('rejects ambiguous --add and --name combinations', async () => {
    const command = await loadCommand();

    await command.handler({
      command: 'skill',
      args: [],
      options: {
        add: 'owner/repo/skills/review',
        name: 'octocode-engineer',
        platform: 'common',
        json: true,
      },
    });

    expect(process.exitCode).toBe(EXIT.USAGE);
    expect(fetchMocks.readSkillFromGitHub).not.toHaveBeenCalled();
    expect(fetchMocks.installMarketplaceSkill).not.toHaveBeenCalled();
    expect(jsonOutput()).toMatchObject({
      success: false,
      error:
        'Use either --add <github-folder> or --name <octocode-skill>, not both',
    });
  });

  it('rejects invalid platforms', async () => {
    const command = await loadCommand();

    await command.handler({
      command: 'skill',
      args: [],
      options: {
        add: 'owner/repo/skills/review',
        platform: 'bad',
        json: true,
      },
    });

    expect(process.exitCode).toBe(EXIT.USAGE);
    expect(jsonOutput()).toMatchObject({
      success: false,
    });
    expect(fetchMocks.installMarketplaceSkill).not.toHaveBeenCalled();
  });

  it('reports missing remote SKILL.md as not found', async () => {
    fetchMocks.readSkillFromGitHub.mockRejectedValueOnce(
      new Error('SKILL.md not found')
    );
    const command = await loadCommand();

    await command.handler({
      command: 'skill',
      args: [],
      options: {
        add: 'owner/repo/skills/review',
        platform: 'common',
        json: true,
      },
    });

    expect(process.exitCode).toBe(EXIT.NOT_FOUND);
    expect(jsonOutput()).toMatchObject({
      success: false,
      error: 'SKILL.md not found',
    });
  });

  it('reports missing named Octocode skills as not found', async () => {
    fetchMocks.readSkillFromGitHub.mockRejectedValueOnce(
      new Error('SKILL.md not found')
    );
    const command = await loadCommand();

    await command.handler({
      command: 'skill',
      args: [],
      options: {
        name: 'missing-skill',
        platform: 'common',
        json: true,
      },
    });

    expect(process.exitCode).toBe(EXIT.NOT_FOUND);
    expect(fetchMocks.installMarketplaceSkill).not.toHaveBeenCalled();
    expect(jsonOutput()).toMatchObject({
      success: false,
      skill: 'missing-skill',
      source:
        'https://github.com/bgauryy/octocode/tree/main/skills/missing-skill',
      error:
        'Octocode skill not found: missing-skill (https://github.com/bgauryy/octocode/tree/main/skills/missing-skill)',
    });
  });
});
