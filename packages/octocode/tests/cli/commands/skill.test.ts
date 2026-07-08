import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { mkdtempSync, mkdirSync, rmSync } from 'node:fs';
import os from 'node:os';
import path from 'node:path';
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
  fetchMarketplaceSkills: vi.fn().mockResolvedValue([]),
}));

vi.mock('../../../src/utils/skills-fetch.js', () => ({
  readSkillFromGitHub: fetchMocks.readSkillFromGitHub,
  installMarketplaceSkill: fetchMocks.installMarketplaceSkill,
  fetchMarketplaceSkills: fetchMocks.fetchMarketplaceSkills,
}));

const fsMocks = vi.hoisted(() => ({
  fileExists: vi.fn().mockReturnValue(true),
  dirExists: vi.fn().mockReturnValue(true),
}));

vi.mock('../../../src/utils/fs.js', () => ({
  copyDirectory: vi.fn().mockReturnValue(true),
  dirExists: fsMocks.dirExists,
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
      stop: vi.fn(),
      warn: vi.fn(),
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
    fetchMocks.fetchMarketplaceSkills.mockResolvedValue([]);
    skillMocks.installSkillToDestination.mockReturnValue('installed');
    fsMocks.fileExists.mockReturnValue(true);
    fsMocks.dirExists.mockReturnValue(true);
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
      '/octocode-home/skills'
    );
    expect(skillMocks.installSkillToDestination).toHaveBeenCalledWith(
      expect.objectContaining({
        destinationPath: '/targets/cursor/review',
        mode: 'symlink',
      })
    );
    expect(skillMocks.installSkillToDestination).toHaveBeenCalledWith(
      expect.objectContaining({
        destinationPath: '/targets/agents/review',
        mode: 'symlink',
      })
    );
    const output = jsonOutput();
    expect(output).toMatchObject({
      success: true,
      platforms: ['cursor', 'codex'],
      summary: {
        installed: 2,
        skipped: 0,
        failed: 0,
      },
    });
    expect(output).not.toHaveProperty('skill');
    expect(output).not.toHaveProperty('source');
    expect(output).not.toHaveProperty('sourcePath');
    expect(output).not.toHaveProperty('cachePath');
    expect(output).not.toHaveProperty('targets');
    expect(output).not.toHaveProperty('installed');
    expect(output.skills).toMatchObject([
      {
        name: 'review',
        sourcePath: '/octocode-home/skills/review',
        summary: {
          installed: 2,
          skipped: 0,
          failed: 0,
        },
      },
    ]);
  });

  it('adds a named Octocode skill from the canonical skills path', async () => {
    const command = await loadCommand();

    await command.handler({
      command: 'skill',
      args: [],
      options: {
        name: 'octocode-research',
        platform: 'common',
        json: true,
      },
    });

    // Bundled path is available (dirExists mocked true), so GitHub is not consulted.
    expect(fetchMocks.readSkillFromGitHub).not.toHaveBeenCalled();
    expect(fetchMocks.installMarketplaceSkill).toHaveBeenCalledWith(
      expect.objectContaining({
        name: 'octocode-research',
        source: expect.objectContaining({ type: 'local' }),
      }),
      '/octocode-home/skills'
    );
    expect(skillMocks.installSkillToDestination).toHaveBeenCalledWith(
      expect.objectContaining({
        destinationPath: '/targets/agents/octocode-research',
        mode: 'symlink',
      })
    );
    const output = jsonOutput();
    expect(output).toMatchObject({
      success: true,
      platforms: ['common'],
      summary: {
        installed: 1,
        skipped: 0,
        failed: 0,
      },
    });
    expect(output).not.toHaveProperty('skill');
    expect(output).not.toHaveProperty('source');
    expect(output).not.toHaveProperty('sourcePath');
    expect(output).not.toHaveProperty('cachePath');
    expect(output).not.toHaveProperty('targets');
    expect(output).not.toHaveProperty('installed');
    expect(output.skills).toMatchObject([
      {
        name: 'octocode-research',
        // Bundled source is a file:// URL, not a GitHub URL.
        source: expect.stringContaining('file://'),
        sourcePath: '/octocode-home/skills/octocode-research',
        summary: {
          installed: 1,
          skipped: 0,
          failed: 0,
        },
      },
    ]);
  });

  it('adds a local skill folder from --add --path without GitHub lookup', async () => {
    const command = await loadCommand();

    await command.handler({
      command: 'skill',
      args: [],
      options: {
        add: true,
        path: '/agent/known/skills/octocode-awareness',
        platform: 'common',
        json: true,
      },
    });

    expect(fetchMocks.readSkillFromGitHub).not.toHaveBeenCalled();
    expect(fetchMocks.installMarketplaceSkill).toHaveBeenCalledWith(
      expect.objectContaining({
        name: 'octocode-awareness',
        path: 'octocode-awareness',
        source: expect.objectContaining({
          type: 'local',
          skillsPath: '/agent/known/skills',
        }),
      }),
      '/octocode-home/skills'
    );
    expect(skillMocks.installSkillToDestination).toHaveBeenCalledWith(
      expect.objectContaining({
        sourcePath: '/octocode-home/skills/octocode-awareness',
        destinationPath: '/targets/agents/octocode-awareness',
        mode: 'symlink',
      })
    );
    expect(jsonOutput()).toMatchObject({
      success: true,
      platforms: ['common'],
      skills: [
        {
          name: 'octocode-awareness',
          source: 'file:///agent/known/skills/octocode-awareness',
          sourcePath: '/octocode-home/skills/octocode-awareness',
        },
      ],
      summary: {
        installed: 1,
        skipped: 0,
        failed: 0,
      },
    });
  });

  it('adds every direct child skill from a local skills path', async () => {
    const skillsRoot = mkdtempSync(path.join(os.tmpdir(), 'octocode-skills-'));
    mkdirSync(path.join(skillsRoot, 'octocode-awareness'));
    mkdirSync(path.join(skillsRoot, 'octocode-research'));
    fsMocks.fileExists.mockImplementation(
      (p: string) => p !== path.join(skillsRoot, 'SKILL.md')
    );
    const command = await loadCommand();

    try {
      await command.handler({
        command: 'skill',
        args: [],
        options: {
          add: true,
          path: skillsRoot,
          platform: 'common',
          json: true,
        },
      });
    } finally {
      rmSync(skillsRoot, { recursive: true, force: true });
    }

    expect(fetchMocks.readSkillFromGitHub).not.toHaveBeenCalled();
    expect(fetchMocks.installMarketplaceSkill).toHaveBeenCalledTimes(2);
    expect(fetchMocks.installMarketplaceSkill).toHaveBeenCalledWith(
      expect.objectContaining({
        name: 'octocode-awareness',
        source: expect.objectContaining({
          type: 'local',
          skillsPath: skillsRoot,
        }),
      }),
      '/octocode-home/skills'
    );
    expect(fetchMocks.installMarketplaceSkill).toHaveBeenCalledWith(
      expect.objectContaining({
        name: 'octocode-research',
        source: expect.objectContaining({
          type: 'local',
          skillsPath: skillsRoot,
        }),
      }),
      '/octocode-home/skills'
    );
    expect(jsonOutput()).toMatchObject({
      success: true,
      platforms: ['common'],
      summary: {
        installed: 2,
        skipped: 0,
        failed: 0,
      },
    });
  });

  it('defaults to the common platform when none is given', async () => {
    const command = await loadCommand();

    await command.handler({
      command: 'skill',
      args: [],
      options: {
        add: 'owner/repo/skills/review',
        json: true,
      },
    });

    // No rigid "--platform required" gate: it falls back to the platform-agnostic
    // `common` target and installs.
    expect(fetchMocks.installMarketplaceSkill).toHaveBeenCalled();
    expect(skillMocks.installSkillToDestination).toHaveBeenCalledWith(
      expect.objectContaining({
        destinationPath: '/targets/agents/review',
        mode: 'symlink',
      })
    );
    expect(jsonOutput()).toMatchObject({
      success: true,
      platforms: ['common'],
      summary: {
        installed: 1,
        skipped: 0,
        failed: 0,
      },
    });
  });

  it('prints non-redundant dry-run JSON', async () => {
    const command = await loadCommand();

    await command.handler({
      command: 'skill',
      args: [],
      options: {
        add: 'owner/repo/skills/review',
        'dry-run': true,
        json: true,
      },
    });

    expect(fetchMocks.installMarketplaceSkill).not.toHaveBeenCalled();
    expect(skillMocks.installSkillToDestination).not.toHaveBeenCalled();

    const output = jsonOutput();
    expect(output).toMatchObject({
      dryRun: true,
      mode: 'symlink',
      platforms: ['common'],
      skills: [
        {
          name: 'review',
          sourcePath: '/octocode-home/skills/review',
          targets: [
            {
              target: 'agents',
              path: '/targets/agents/review',
              action: 'install',
            },
          ],
        },
      ],
    });
    expect(output).not.toHaveProperty('skill');
    expect(output).not.toHaveProperty('source');
    expect(output).not.toHaveProperty('sourcePath');
    expect(output).not.toHaveProperty('targets');
  });

  it('prints mode, platforms, source, and destination paths for human output', async () => {
    const command = await loadCommand();

    await command.handler({
      command: 'skill',
      args: [],
      options: {
        name: 'octocode-research',
        platform: 'pi',
      },
    });

    const output = consoleSpy.mock.calls.flat().join('\n');
    expect(output).toContain('Mode:');
    expect(output).toContain('symlink');
    expect(output).toContain('Platforms:');
    expect(output).toContain('pi');
    expect(output).toContain('Source:');
    expect(output).toContain('/octocode-home/skills/octocode-research');
    expect(output).toContain('/targets/pi/octocode-research');
  });

  it('adds every skill from a GitHub skills library path', async () => {
    fetchMocks.readSkillFromGitHub.mockRejectedValueOnce(
      new Error('SKILL.md not found')
    );
    const librarySource = {
      id: 'github-owner-repo-main-skills',
      name: 'owner/repo',
      type: 'github',
      owner: 'owner',
      repo: 'repo',
      branch: 'main',
      skillsPath: 'skills',
      skillPattern: 'skill-folders',
      description: 'GitHub skills library',
      url: 'https://github.com/owner/repo/tree/main/skills',
    };
    fetchMocks.fetchMarketplaceSkills.mockResolvedValueOnce([
      {
        name: 'code-review',
        displayName: 'Code Review',
        description: 'Review code',
        path: 'skills/code-review',
        source: librarySource,
      },
      {
        name: 'planning',
        displayName: 'Planning',
        description: 'Plan work',
        path: 'skills/planning',
        source: librarySource,
      },
    ]);
    const command = await loadCommand();

    await command.handler({
      command: 'skill',
      args: [],
      options: {
        add: 'owner/repo/skills',
        json: true,
      },
    });

    expect(fetchMocks.fetchMarketplaceSkills).toHaveBeenCalledWith(
      expect.objectContaining({
        owner: 'owner',
        repo: 'repo',
        branch: 'main',
        skillsPath: 'skills',
      }),
      { skipCache: true }
    );
    expect(fetchMocks.installMarketplaceSkill).toHaveBeenCalledTimes(2);
    expect(fetchMocks.installMarketplaceSkill).toHaveBeenCalledWith(
      expect.objectContaining({ name: 'code-review' }),
      '/octocode-home/skills'
    );
    expect(skillMocks.installSkillToDestination).toHaveBeenCalledWith(
      expect.objectContaining({
        sourcePath: '/octocode-home/skills/code-review',
        destinationPath: '/targets/agents/code-review',
        mode: 'symlink',
      })
    );
    expect(skillMocks.installSkillToDestination).toHaveBeenCalledWith(
      expect.objectContaining({
        sourcePath: '/octocode-home/skills/planning',
        destinationPath: '/targets/agents/planning',
        mode: 'symlink',
      })
    );

    const output = jsonOutput();
    expect(output).toMatchObject({
      success: true,
      platforms: ['common'],
      summary: {
        installed: 2,
        skipped: 0,
        failed: 0,
      },
    });
    expect(output).not.toHaveProperty('installed');
    expect(output.skills as unknown[]).toHaveLength(2);
  });

  it('installs all Octocode skills with the selected platform', async () => {
    const source = {
      id: 'github-bgauryy-octocode-main-skills',
      name: 'bgauryy/octocode',
      type: 'github',
      owner: 'bgauryy',
      repo: 'octocode',
      branch: 'main',
      skillsPath: 'skills',
      skillPattern: 'skill-folders',
      description: 'Official Octocode skills',
      url: 'https://github.com/bgauryy/octocode/tree/main/skills',
    };
    fetchMocks.fetchMarketplaceSkills.mockResolvedValueOnce([
      {
        name: 'octocode-research',
        displayName: 'Octocode Research',
        description: 'Research',
        path: 'skills/octocode-research',
        source,
      },
      {
        name: 'octocode-rfc-generator',
        displayName: 'Octocode RFC Generator',
        description: 'RFC Generator',
        path: 'skills/octocode-rfc-generator',
        source,
      },
    ]);
    const command = await loadCommand();

    await command.handler({
      command: 'skill',
      args: [],
      options: {
        'install-all': true,
        platform: 'pi',
        json: true,
      },
    });

    expect(fetchMocks.readSkillFromGitHub).not.toHaveBeenCalled();
    expect(fetchMocks.fetchMarketplaceSkills).toHaveBeenCalledWith(
      expect.objectContaining({
        owner: 'bgauryy',
        repo: 'octocode',
        skillsPath: 'skills',
      }),
      { skipCache: true }
    );
    expect(fetchMocks.installMarketplaceSkill).toHaveBeenCalledTimes(2);
    expect(skillMocks.installSkillToDestination).toHaveBeenCalledWith(
      expect.objectContaining({
        destinationPath: '/targets/pi/octocode-rfc-generator',
        mode: 'symlink',
      })
    );
    expect(skillMocks.installSkillToDestination).toHaveBeenCalledWith(
      expect.objectContaining({
        destinationPath: '/targets/pi/octocode-research',
        mode: 'symlink',
      })
    );

    const output = jsonOutput();
    expect(output).toMatchObject({
      success: true,
      platforms: ['pi'],
      summary: {
        installed: 2,
        skipped: 0,
        failed: 0,
      },
    });
    expect(output).not.toHaveProperty('installed');
    expect(output.skills as unknown[]).toHaveLength(2);
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

  it('adds a named Octocode skill to Pi', async () => {
    const command = await loadCommand();

    await command.handler({
      command: 'skill',
      args: [],
      options: {
        name: 'octocode-research',
        platform: 'pi',
        json: true,
      },
    });

    expect(skillMocks.installSkillToDestination).toHaveBeenCalledWith(
      expect.objectContaining({
        destinationPath: '/targets/pi/octocode-research',
        mode: 'symlink',
      })
    );
    expect(jsonOutput()).toMatchObject({
      success: true,
      platforms: ['pi'],
      skills: [{ name: 'octocode-research' }],
      summary: {
        installed: 1,
        skipped: 0,
        failed: 0,
      },
    });
  });

  it('adds a skill to GitHub Copilot and Gemini platforms', async () => {
    const command = await loadCommand();

    await command.handler({
      command: 'skill',
      args: [],
      options: {
        add: 'owner/repo/skills/review',
        platform: 'github-copilot,gemini-cli',
        json: true,
      },
    });

    expect(skillMocks.installSkillToDestination).toHaveBeenCalledWith(
      expect.objectContaining({
        destinationPath: '/targets/copilot/review',
        mode: 'symlink',
      })
    );
    expect(skillMocks.installSkillToDestination).toHaveBeenCalledWith(
      expect.objectContaining({
        destinationPath: '/targets/gemini/review',
        mode: 'symlink',
      })
    );
    expect(jsonOutput()).toMatchObject({
      success: true,
      platforms: ['copilot', 'gemini'],
      summary: {
        installed: 2,
        skipped: 0,
        failed: 0,
      },
    });
  });

  it('uses the common default in a TTY without prompting', async () => {
    setStdoutTTY(true);
    const command = await loadCommand();

    await command.handler({
      command: 'skill',
      args: [],
      options: {
        add: 'owner/repo/skills/review',
      },
    });

    // A TTY must not trigger an interactive platform prompt — it proceeds with
    // the `common` default and installs non-interactively.
    expect(process.exitCode).not.toBe(EXIT.USAGE);
    expect(fetchMocks.installMarketplaceSkill).toHaveBeenCalled();
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
        name: 'octocode-research',
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
        'Use only one of --add <github-path>, --add --path <local-skill-or-skills-dir>, --name <octocode-skill>, or --install-all',
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
    // Simulate skill not present in the bundle (dirExists returns false for the
    // specific skill sub-directory) so the command falls back to GitHub lookup.
    fsMocks.dirExists.mockReturnValueOnce(true).mockReturnValueOnce(false);
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
