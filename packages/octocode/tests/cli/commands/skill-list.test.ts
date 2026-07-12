import { beforeEach, describe, expect, it, vi } from 'vitest';

const skillsMocks = vi.hoisted(() => ({
  getAllSkillsMetadata: vi.fn(),
}));

vi.mock('../../../src/utils/skills.js', async importOriginal => {
  const actual =
    await importOriginal<typeof import('../../../src/utils/skills.js')>();
  return {
    ...actual,
    getAllSkillsMetadata: skillsMocks.getAllSkillsMetadata,
  };
});

const fetchMocks = vi.hoisted(() => ({
  fetchMarketplaceSkills: vi.fn(),
}));

vi.mock('../../../src/utils/skills-fetch.js', () => ({
  fetchMarketplaceSkills: fetchMocks.fetchMarketplaceSkills,
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

describe('runListCommand', () => {
  let consoleSpy: ReturnType<typeof vi.spyOn>;

  beforeEach(() => {
    vi.clearAllMocks();
    consoleSpy = vi.spyOn(console, 'log').mockImplementation(() => {});
  });

  async function runList(jsonOutput: boolean) {
    const { runListCommand } =
      await import('../../../src/cli/commands/skill/list-command.js');
    await runListCommand(jsonOutput);
  }

  function jsonOutputOf(): unknown {
    return JSON.parse(consoleSpy.mock.calls[0]![0] as string);
  }

  it('prefers the bundled skill list and never calls GitHub when a bundle exists', async () => {
    skillsMocks.getAllSkillsMetadata.mockReturnValue([
      {
        name: 'octocode-research',
        description: 'Research skill',
        folder: 'octocode-research',
      },
      {
        name: 'octocode-awareness',
        description: 'Awareness skill',
        folder: 'octocode-awareness',
      },
    ]);

    await runList(true);

    expect(fetchMocks.fetchMarketplaceSkills).not.toHaveBeenCalled();
    const payload = jsonOutputOf() as {
      success: boolean;
      offline: boolean;
      skills: { name: string }[];
    };
    expect(payload.success).toBe(true);
    expect(payload.offline).toBe(true);
    expect(payload.skills.map(s => s.name).sort()).toEqual([
      'octocode-awareness',
      'octocode-research',
    ]);
  });

  it('falls back to the GitHub marketplace listing when no bundle is present', async () => {
    skillsMocks.getAllSkillsMetadata.mockReturnValue([]);
    fetchMocks.fetchMarketplaceSkills.mockResolvedValue([
      {
        name: 'octocode-research',
        displayName: 'Octocode Research',
        description: 'Research skill',
        path: 'skills/octocode-research',
        source: {
          id: 'x',
          name: 'x',
          type: 'github',
          owner: 'bgauryy',
          repo: 'octocode',
          branch: 'main',
          skillsPath: 'skills',
          skillPattern: 'skill-folders',
          description: '',
          url: '',
        },
      },
    ]);

    await runList(true);

    expect(fetchMocks.fetchMarketplaceSkills).toHaveBeenCalledTimes(1);
    const payload = jsonOutputOf() as {
      success: boolean;
      offline: boolean;
      skills: { name: string }[];
    };
    expect(payload.success).toBe(true);
    expect(payload.offline).toBe(false);
    expect(payload.skills.map(s => s.name)).toEqual(['octocode-research']);
  });

  it('falls back to the known-skills list when both bundle and GitHub are unavailable', async () => {
    skillsMocks.getAllSkillsMetadata.mockReturnValue([]);
    fetchMocks.fetchMarketplaceSkills.mockRejectedValue(new Error('offline'));

    await runList(true);

    const payload = jsonOutputOf() as {
      success: boolean;
      fallback: boolean;
      skills: { name: string }[];
    };
    expect(payload.success).toBe(false);
    expect(payload.fallback).toBe(true);
    // Every known-skills fallback entry must be a real, installable skill
    // folder — no meta-skills that have no folder under skills/.
    expect(payload.skills.map(s => s.name)).not.toContain('octocode');
    expect(payload.skills.map(s => s.name)).not.toContain('octocode-stats');
    expect(payload.skills.map(s => s.name)).toEqual([
      'octocode-awareness',
      'octocode-research',
    ]);
  });

  it('treats an empty GitHub result the same as a failed fetch', async () => {
    skillsMocks.getAllSkillsMetadata.mockReturnValue([]);
    fetchMocks.fetchMarketplaceSkills.mockResolvedValue([]);

    await runList(true);

    const payload = jsonOutputOf() as { success: boolean; fallback: boolean };
    expect(payload.success).toBe(false);
    expect(payload.fallback).toBe(true);
  });
});
