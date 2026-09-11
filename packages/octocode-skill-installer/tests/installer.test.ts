import {
  existsSync,
  lstatSync,
  mkdtempSync,
  mkdirSync,
  readFileSync,
  realpathSync,
  rmSync,
  symlinkSync,
  writeFileSync,
} from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, relative } from 'node:path';
import { afterEach, describe, expect, it } from 'vitest';
import {
  getCanonicalSkillsDir,
  installBundledSkills,
  parseSkillPlatforms,
  resolveSkillDestination,
  type BundledSkill,
} from '../src/index.js';

const roots: string[] = [];

function tempRoot(): string {
  const root = mkdtempSync(join(tmpdir(), 'octocode-skill-installer-'));
  roots.push(root);
  return root;
}

function fixtureSkill(
  root: string,
  name = 'fixture-skill',
  body = '# Fixture\n'
): BundledSkill {
  const sourcePath = join(root, 'bundle', name);
  mkdirSync(sourcePath, { recursive: true });
  writeFileSync(join(sourcePath, 'SKILL.md'), body);
  return { name, sourcePath };
}

afterEach(() => {
  for (const root of roots.splice(0))
    rmSync(root, { recursive: true, force: true });
});

describe('platform contract', () => {
  it('resolves the durable canonical store through shared config', () => {
    expect(getCanonicalSkillsDir({ OCTOCODE_HOME: '/tmp/octocode-home' })).toBe(
      '/tmp/octocode-home/skills'
    );
  });

  it('normalizes aliases and expands all without duplicate destinations', () => {
    expect(parseSkillPlatforms('shared,common,agents,codex')).toEqual({
      platforms: ['codex'],
    });
    expect(parseSkillPlatforms('all').platforms).toEqual([
      'pi',
      'cursor',
      'claude',
      'claude-desktop',
      'codex',
      'codex-native',
      'opencode',
      'copilot',
      'gemini',
    ]);
    expect(parseSkillPlatforms('')).toEqual({ platforms: [] });
    expect(parseSkillPlatforms('unknown').error).toContain('Unknown platform');
  });

  it('resolves user and workspace destinations per platform', () => {
    const root = tempRoot();
    const homeDir = join(root, 'home');
    const projectDir = join(root, 'project');
    expect(
      resolveSkillDestination({ platform: 'pi', scope: 'global', homeDir })
    ).toBe(join(homeDir, '.pi/agent/skills'));
    expect(
      resolveSkillDestination({
        platform: 'copilot',
        scope: 'project',
        homeDir,
        projectDir,
      })
    ).toBe(join(projectDir, '.github/skills'));
    expect(() =>
      resolveSkillDestination({
        platform: 'claude-desktop',
        scope: 'project',
        homeDir,
        projectDir,
      })
    ).toThrow('does not support project');
  });

  it('owns every supported destination, including Windows special cases', () => {
    const root = tempRoot();
    const homeDir = join(root, 'home');
    const projectDir = join(root, 'project');
    const appDataDir = join(root, 'appdata');
    const globalExpected = new Map([
      ['cursor', join(homeDir, '.cursor/skills')],
      ['claude', join(homeDir, '.claude/skills')],
      ['claude-desktop', join(appDataDir, 'Claude Desktop/skills')],
      ['codex', join(homeDir, '.agents/skills')],
      ['codex-native', join(homeDir, '.codex/skills')],
      ['opencode', join(appDataDir, 'opencode/skills')],
      ['copilot', join(homeDir, '.copilot/skills')],
      ['gemini', join(homeDir, '.gemini/skills')],
    ]);
    for (const [platform, expected] of globalExpected) {
      expect(
        resolveSkillDestination({
          platform: platform as Parameters<
            typeof resolveSkillDestination
          >[0]['platform'],
          scope: 'global',
          homeDir,
          appDataDir,
          operatingSystem: 'win32',
        })
      ).toBe(expected);
    }
    expect(
      resolveSkillDestination({
        platform: 'opencode',
        scope: 'project',
        homeDir,
        projectDir,
      })
    ).toBe(join(projectDir, '.opencode/skills'));
    expect(
      resolveSkillDestination({
        platform: 'opencode',
        scope: 'global',
        homeDir,
        operatingSystem: 'darwin',
      })
    ).toBe(join(homeDir, '.config/opencode/skills'));
    expect(
      resolveSkillDestination({
        platform: 'claude-desktop',
        scope: 'global',
        homeDir,
        operatingSystem: 'darwin',
      })
    ).toBe(join(homeDir, '.claude-desktop/skills'));
    expect(() =>
      resolveSkillDestination({ platform: 'codex', scope: 'project' })
    ).toThrow('projectDir is required');
  });
});

describe('installBundledSkills', () => {
  it('materializes one durable canonical copy and links every platform directory to it', () => {
    const root = tempRoot();
    const skill = fixtureSkill(root);
    const canonicalSkillsDir = join(root, 'octocode-home', 'skills');
    const homeDir = join(root, 'home');
    const result = installBundledSkills({
      skills: [skill],
      canonicalSkillsDir,
      targets: [
        { platform: 'pi', scope: 'global', homeDir },
        { platform: 'codex', scope: 'global', homeDir },
      ],
      mode: 'symlink',
      force: false,
      dryRun: false,
    });

    expect(result.ok).toBe(true);
    expect(result.summary).toMatchObject({
      installed: 1,
      linked: 2,
      conflicts: 0,
      failed: 0,
    });
    const canonical = join(canonicalSkillsDir, skill.name);
    expect(lstatSync(canonical).isDirectory()).toBe(true);
    expect(lstatSync(canonical).isSymbolicLink()).toBe(false);
    expect(readFileSync(join(canonical, 'SKILL.md'), 'utf8')).toBe(
      '# Fixture\n'
    );
    for (const destination of [
      join(homeDir, '.pi/agent/skills', skill.name),
      join(homeDir, '.agents/skills', skill.name),
    ]) {
      expect(lstatSync(destination).isSymbolicLink()).toBe(true);
      expect(realpathSync(destination)).toBe(realpathSync(canonical));
    }
  });

  it('is idempotent and refuses drift unless forced', () => {
    const root = tempRoot();
    const skill = fixtureSkill(root);
    const canonicalSkillsDir = join(root, 'octocode-home', 'skills');
    const homeDir = join(root, 'home');
    const request = {
      skills: [skill],
      canonicalSkillsDir,
      targets: [
        { platform: 'codex' as const, scope: 'global' as const, homeDir },
      ],
      mode: 'symlink' as const,
      dryRun: false,
    };

    expect(installBundledSkills({ ...request, force: false }).ok).toBe(true);
    const unchanged = installBundledSkills({ ...request, force: false });
    expect(unchanged.ok).toBe(true);
    expect(unchanged.summary.unchanged).toBe(2);

    writeFileSync(
      join(canonicalSkillsDir, skill.name, 'SKILL.md'),
      '# Local edit\n'
    );
    const conflict = installBundledSkills({ ...request, force: false });
    expect(conflict.ok).toBe(false);
    expect(conflict.summary.conflicts).toBe(1);
    expect(
      readFileSync(join(canonicalSkillsDir, skill.name, 'SKILL.md'), 'utf8')
    ).toBe('# Local edit\n');

    const replaced = installBundledSkills({ ...request, force: true });
    expect(replaced.ok).toBe(true);
    expect(
      readFileSync(join(canonicalSkillsDir, skill.name, 'SKILL.md'), 'utf8')
    ).toBe('# Fixture\n');
  });

  it('plans without writing and reports an unsupported source', () => {
    const root = tempRoot();
    const canonicalSkillsDir = join(root, 'octocode-home', 'skills');
    const homeDir = join(root, 'home');
    const missing = {
      name: 'missing-skill',
      sourcePath: join(root, 'missing'),
    };
    const result = installBundledSkills({
      skills: [missing],
      canonicalSkillsDir,
      targets: [{ platform: 'codex', scope: 'global', homeDir }],
      mode: 'symlink',
      force: false,
      dryRun: true,
    });
    expect(result.ok).toBe(false);
    expect(result.summary.failed).toBe(1);
    expect(existsSync(canonicalSkillsDir)).toBe(false);
  });

  it('copies only when requested or selected by auto mode', () => {
    const root = tempRoot();
    const skill = fixtureSkill(root);
    const canonicalSkillsDir = join(root, 'octocode-home', 'skills');
    const homeDir = join(root, 'home');
    const result = installBundledSkills({
      skills: [skill],
      canonicalSkillsDir,
      targets: [
        { platform: 'claude', scope: 'global', homeDir },
        { platform: 'cursor', scope: 'global', homeDir },
      ],
      mode: 'auto',
      force: false,
      dryRun: false,
    });
    expect(result.ok).toBe(true);
    const destinations = result.skills[0]!.destinations;
    expect(destinations.map(({ mode, status }) => ({ mode, status }))).toEqual([
      { mode: 'copy', status: 'copied' },
      { mode: 'symlink', status: 'linked' },
    ]);
  });

  it('refuses destination drift and atomically replaces it only when forced', () => {
    const root = tempRoot();
    const skill = fixtureSkill(root);
    const canonicalSkillsDir = join(root, 'octocode-home', 'skills');
    const homeDir = join(root, 'home');
    const request = {
      skills: [skill],
      canonicalSkillsDir,
      targets: [
        { platform: 'codex' as const, scope: 'global' as const, homeDir },
      ],
      mode: 'symlink' as const,
      dryRun: false,
    };
    expect(installBundledSkills({ ...request, force: false }).ok).toBe(true);
    const destination = join(homeDir, '.agents/skills', skill.name);
    const canonical = join(canonicalSkillsDir, skill.name);
    rmSync(destination, { recursive: true, force: true });
    symlinkSync(relative(dirname(destination), canonical), destination, 'dir');
    expect(
      installBundledSkills({ ...request, force: false }).summary.unchanged
    ).toBe(2);
    rmSync(destination, { recursive: true, force: true });
    mkdirSync(destination, { recursive: true });
    writeFileSync(join(destination, 'SKILL.md'), '# Drift\n');

    const conflict = installBundledSkills({ ...request, force: false });
    expect(conflict.skills[0]!.destinations[0]!.status).toBe('conflict');
    expect(readFileSync(join(destination, 'SKILL.md'), 'utf8')).toBe(
      '# Drift\n'
    );

    const replaced = installBundledSkills({ ...request, force: true });
    expect(replaced.skills[0]!.destinations[0]!.status).toBe('linked');
    expect(lstatSync(destination).isSymbolicLink()).toBe(true);
  });

  it('makes copied targets idempotent and replaces copied drift when forced', () => {
    const root = tempRoot();
    const skill = fixtureSkill(root);
    const canonicalSkillsDir = join(root, 'octocode-home', 'skills');
    const homeDir = join(root, 'home');
    const request = {
      skills: [skill],
      canonicalSkillsDir,
      targets: [
        { platform: 'claude' as const, scope: 'global' as const, homeDir },
      ],
      mode: 'copy' as const,
      dryRun: false,
    };
    expect(
      installBundledSkills({ ...request, force: false }).summary.copied
    ).toBe(1);
    expect(
      installBundledSkills({ ...request, force: false }).summary.unchanged
    ).toBe(2);
    const destination = join(homeDir, '.claude/skills', skill.name);
    writeFileSync(join(destination, 'SKILL.md'), '# Drift\n');
    expect(
      installBundledSkills({ ...request, force: false }).summary.conflicts
    ).toBe(1);
    expect(
      installBundledSkills({ ...request, force: true }).summary.copied
    ).toBe(1);
    expect(readFileSync(join(destination, 'SKILL.md'), 'utf8')).toBe(
      '# Fixture\n'
    );
  });

  it('rejects invalid names and symlinked SKILL.md files', () => {
    const root = tempRoot();
    const valid = fixtureSkill(root, 'valid-skill');
    const linkedSource = join(root, 'bundle', 'linked-skill');
    mkdirSync(linkedSource, { recursive: true });
    symlinkSync(
      join(valid.sourcePath, 'SKILL.md'),
      join(linkedSource, 'SKILL.md')
    );
    const result = installBundledSkills({
      skills: [
        { name: '../escape', sourcePath: valid.sourcePath },
        { name: 'linked-skill', sourcePath: linkedSource },
      ],
      canonicalSkillsDir: join(root, 'octocode-home', 'skills'),
      targets: [],
    });
    expect(result.ok).toBe(false);
    expect(result.summary.failed).toBe(2);
    expect(result.skills[0]!.canonicalError).toContain('Invalid skill name');
    expect(result.skills[1]!.canonicalError).toContain('regular file');
  });

  it('reports unsupported project targets and deduplicates repeated destinations', () => {
    const root = tempRoot();
    const skill = fixtureSkill(root);
    const homeDir = join(root, 'home');
    const result = installBundledSkills({
      skills: [skill],
      canonicalSkillsDir: join(root, 'octocode-home', 'skills'),
      targets: [
        { platform: 'codex', scope: 'global', homeDir },
        { platform: 'codex', scope: 'global', homeDir },
        {
          platform: 'claude-desktop',
          scope: 'project',
          projectDir: join(root, 'project'),
        },
      ],
      dryRun: true,
    });
    expect(result.ok).toBe(false);
    expect(result.skills[0]!.destinations).toHaveLength(2);
    expect(result.skills[0]!.destinations[1]).toMatchObject({
      status: 'failed',
      destination: '',
    });
  });
});
