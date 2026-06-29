import { describe, it, expect, vi, beforeEach } from 'vitest';

const dirExists = vi.fn();
const listSubdirectories = vi.fn();
const removeDirectory = vi.fn();
const getSkillsDirForTarget = vi.fn();
const installSkillToDestination = vi.fn();
const isSafeSkillName = vi.fn();
const normalizeSkillTarget = vi.fn();
const resolveModeForTarget = vi.fn();
const resolveSkillDestination = vi.fn();

vi.mock('../../src/utils/fs.js', () => ({
  dirExists: (...args: unknown[]) => dirExists(...args),
  listSubdirectories: (...args: unknown[]) => listSubdirectories(...args),
  removeDirectory: (...args: unknown[]) => removeDirectory(...args),
}));

vi.mock('../../src/utils/skills.js', () => ({
  getSkillsDirForTarget: (...args: unknown[]) => getSkillsDirForTarget(...args),
  installSkillToDestination: (...args: unknown[]) =>
    installSkillToDestination(...args),
  isSafeSkillName: (...args: unknown[]) => isSafeSkillName(...args),
  normalizeSkillTarget: (...args: unknown[]) => normalizeSkillTarget(...args),
  resolveModeForTarget: (...args: unknown[]) => resolveModeForTarget(...args),
  resolveSkillDestination: (...args: unknown[]) =>
    resolveSkillDestination(...args),
  USER_SKILL_PLATFORM_TARGETS: {
    common: ['agents'],
    cursor: ['cursor'],
    claude: ['claude-code', 'claude-desktop'],
    codex: ['agents'],
    opencode: ['opencode'],
    pi: ['pi'],
    copilot: ['copilot'],
    gemini: ['gemini'],
  },
}));

import {
  getAvailableSkillNames,
  getSkillTargetDestinations,
  installAllSkillsForTargets,
  installSkillForTargets,
  parseUserSkillPlatformList,
  parseSkillTargetList,
  removeSkillFromTargets,
} from '../../src/features/skills.js';

describe('features/skills', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    normalizeSkillTarget.mockImplementation((target: string) =>
      target === 'bad' ? null : target
    );
    getSkillsDirForTarget.mockImplementation(
      (target: string, defaultDir: string) => `${defaultDir}/${target}`
    );
    listSubdirectories.mockReturnValue(['good-skill', '../bad', 'other']);
    isSafeSkillName.mockImplementation((name: string) => !name.includes('/'));
    resolveSkillDestination.mockImplementation(
      (destDir: string, skillName: string) =>
        skillName === 'bad/name' ? null : `${destDir}/${skillName}`
    );
    resolveModeForTarget.mockReturnValue('copy');
    installSkillToDestination.mockReturnValue('installed');
    dirExists.mockReturnValue(true);
    removeDirectory.mockReturnValue(true);
  });

  it('parses and de-duplicates valid targets', () => {
    const result = parseSkillTargetList('codex,bad,codex,cursor');

    expect(result).toEqual({ targets: ['codex', 'cursor'] });
  });

  it('reports an error when no target survives normalization', () => {
    normalizeSkillTarget.mockReturnValue(null);

    expect(parseSkillTargetList('bad,also-bad')).toEqual({
      targets: [],
      error: 'No valid targets provided',
    });
  });

  it('parses user-facing platforms to low-level targets', () => {
    expect(parseUserSkillPlatformList('common,cursor,claude,pi')).toEqual({
      platforms: ['common', 'cursor', 'claude', 'pi'],
      targets: ['agents', 'cursor', 'claude-code', 'claude-desktop', 'pi'],
    });
  });

  it('expands all user-facing platforms', () => {
    expect(parseUserSkillPlatformList('all')).toEqual({
      platforms: [
        'common',
        'cursor',
        'claude',
        'codex',
        'opencode',
        'pi',
        'copilot',
        'gemini',
      ],
      targets: [
        'agents',
        'cursor',
        'claude-code',
        'claude-desktop',
        'opencode',
        'pi',
        'copilot',
        'gemini',
      ],
    });
  });

  it('maps targets to destination directories', () => {
    expect(getSkillTargetDestinations(['codex', 'cursor'], '/skills')).toEqual([
      { target: 'codex', destDir: '/skills/codex' },
      { target: 'cursor', destDir: '/skills/cursor' },
    ]);
  });

  it('filters available skill names through the safety guard', () => {
    expect(getAvailableSkillNames('/source')).toEqual(['good-skill', 'other']);
  });

  it('summarizes install results across destinations', () => {
    installSkillToDestination
      .mockReturnValueOnce('installed')
      .mockReturnValueOnce('skipped')
      .mockReturnValueOnce('failed');

    const result = installSkillForTargets({
      skillName: 'skill-a',
      sourceDir: '/source',
      destinations: [
        { target: 'codex', destDir: '/a' },
        { target: 'cursor', destDir: '/b' },
        { target: 'agents', destDir: '/c' },
      ],
      strategy: 'hybrid',
      force: true,
    });

    expect(result).toEqual({
      installed: 1,
      skipped: 1,
      failed: 1,
      targetCount: 3,
    });
    expect(installSkillToDestination).toHaveBeenCalledWith({
      sourcePath: '/source/skill-a',
      destinationPath: '/a/skill-a',
      mode: 'copy',
      force: true,
    });
  });

  it('counts invalid install destinations as failures', () => {
    const result = installSkillForTargets({
      skillName: 'bad/name',
      sourceDir: '/source',
      destinations: [{ target: 'codex', destDir: '/a' }],
      strategy: 'copy',
      force: false,
    });

    expect(result.failed).toBe(1);
    expect(installSkillToDestination).not.toHaveBeenCalled();
  });

  it('accumulates install-all summaries across skills', () => {
    installSkillToDestination
      .mockReturnValueOnce('installed')
      .mockReturnValueOnce('skipped');

    expect(
      installAllSkillsForTargets({
        skillNames: ['one', 'two'],
        sourceDir: '/source',
        destinations: [{ target: 'codex', destDir: '/dest' }],
        strategy: 'copy',
        force: false,
      })
    ).toEqual({
      installed: 1,
      skipped: 1,
      failed: 0,
      targetCount: 1,
    });
  });

  it('summarizes remove results and failure reasons', () => {
    dirExists.mockReturnValueOnce(false).mockReturnValueOnce(true);
    removeDirectory.mockReturnValueOnce(false);

    expect(
      removeSkillFromTargets({
        skillName: 'skill-a',
        destinations: [
          { target: 'codex', destDir: '/missing' },
          { target: 'cursor', destDir: '/present' },
        ],
      })
    ).toEqual({
      removed: 0,
      missing: 1,
      failed: 1,
      targetCount: 2,
      failures: [
        {
          target: 'cursor',
          path: '/present/skill-a',
          reason: 'remove-failed',
        },
      ],
    });
  });

  it('reports invalid remove destinations without touching the filesystem', () => {
    const result = removeSkillFromTargets({
      skillName: 'bad/name',
      destinations: [{ target: 'codex', destDir: '/dest' }],
    });

    expect(result.failures).toEqual([
      { target: 'codex', reason: 'invalid-skill-name' },
    ]);
    expect(dirExists).not.toHaveBeenCalled();
  });
});
