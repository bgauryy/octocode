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
import { spawnSync } from 'node:child_process';
import { homedir, tmpdir } from 'node:os';
import { dirname, join, relative, resolve } from 'node:path';
import { afterEach, describe, expect, it } from 'vitest';
import {
  formatSkillPlatformHelp,
  getCanonicalSkillsDir,
  installBundledSkills,
  parseSkillPlatforms,
  resolveSkillDestination,
  SKILL_PLATFORMS,
  type BundledSkill,
} from '../src/index.js';

const roots: string[] = [];

const SKILL_SYNC_SCRIPT = resolve(
  import.meta.dirname,
  '../../../skills/octocode-skills/scripts/skill-sync.mjs'
);

describe('portable skill-sync registry contract', () => {
  it('matches the shared canonical platform registry', () => {
    const result = spawnSync(
      process.execPath,
      [SKILL_SYNC_SCRIPT, '--list-vendors', '--json'],
      { encoding: 'utf8' }
    );
    expect(result.status, result.stderr || result.stdout).toBe(0);
    const payload = JSON.parse(result.stdout) as {
      vendors: Array<{
        id: string;
        canonical: string;
        userRelativePath: string;
        project: string;
      }>;
      aliases: Record<string, string>;
    };
    const canonical = payload.vendors.filter(
      vendor => vendor.id === vendor.canonical
    );
    expect(
      canonical.map(({ id, userRelativePath, project }) => ({
        platform: id,
        globalRelativePath: userRelativePath,
        projectRelativePath: project,
      })).sort((left, right) => left.platform.localeCompare(right.platform))
    ).toEqual(
      SKILL_PLATFORMS.map(
        ({ platform, globalRelativePath, projectRelativePath }) => ({
          platform,
          globalRelativePath,
          projectRelativePath,
        })
      ).sort((left, right) => left.platform.localeCompare(right.platform))
    );
    expect(payload.aliases).toEqual(
      Object.fromEntries(
        SKILL_PLATFORMS.flatMap(({ platform, aliases }) =>
          aliases.map(alias => [alias, platform])
        )
      )
    );
  });
});

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
  it('formats canonical platform values and aliases from the shared registry', () => {
    expect(formatSkillPlatformHelp()).toBe(
      'pi | cursor | claude | codex | opencode | copilot | gemini | all (aliases: claude-desktop -> claude; shared, common, agents, codex-native -> codex)'
    );
  });

  it('resolves the durable canonical store through shared config', () => {
    expect(getCanonicalSkillsDir({ OCTOCODE_HOME: '/tmp/octocode-home' })).toBe(
      '/tmp/octocode-home/skills'
    );
    expect(
      resolveSkillDestination({ platform: 'codex', scope: 'global' })
    ).toBe(join(homedir(), '.agents/skills'));
  });

  it('normalizes aliases and expands all without duplicate destinations', () => {
    expect(parseSkillPlatforms('shared,common,agents,codex')).toEqual({
      platforms: ['codex'],
    });
    expect(
      parseSkillPlatforms('claude-desktop,claude,codex-native,codex')
    ).toEqual({ platforms: ['claude', 'codex'] });
    expect(parseSkillPlatforms('all').platforms).toEqual([
      'pi',
      'cursor',
      'claude',
      'codex',
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
        platform: 'pi',
        scope: 'project',
        homeDir,
        projectDir,
      })
    ).toBe(join(projectDir, '.pi/skills'));
    expect(
      resolveSkillDestination({
        platform: 'copilot',
        scope: 'project',
        homeDir,
        projectDir,
      })
    ).toBe(join(projectDir, '.github/skills'));
    expect(parseSkillPlatforms('claude-desktop').platforms).toEqual(['claude']);
    expect(parseSkillPlatforms('codex-native').platforms).toEqual(['codex']);
  });

  it('owns every supported global destination consistently across operating systems', () => {
    const root = tempRoot();
    const homeDir = join(root, 'home');
    const projectDir = join(root, 'project');
    const globalExpected = new Map([
      ['cursor', join(homeDir, '.cursor/skills')],
      ['claude', join(homeDir, '.claude/skills')],
      ['codex', join(homeDir, '.agents/skills')],
      ['opencode', join(homeDir, '.config/opencode/skills')],
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
      })
    ).toBe(join(homeDir, '.config/opencode/skills'));
    expect(() =>
      resolveSkillDestination({ platform: 'codex', scope: 'project' })
    ).toThrow('projectDir is required');
  });
});

describe('installBundledSkills', () => {
  it('installs links for every global destination under an isolated home', () => {
    const root = tempRoot();
    const skill = fixtureSkill(root);
    const canonicalSkillsDir = join(root, 'octocode-home', 'skills');
    const homeDir = join(root, 'home');
    const platforms = parseSkillPlatforms('all').platforms;
    const result = installBundledSkills({
      skills: [skill],
      canonicalSkillsDir,
      targets: platforms.map(platform => ({
        platform,
        scope: 'global' as const,
        homeDir,
      })),
    });

    expect(result.ok).toBe(true);
    expect(result.summary).toMatchObject({ installed: 1, linked: 7 });
    expect(result.skills[0]!.destinations).toHaveLength(7);
    expect(
      new Set(
        result.skills[0]!.destinations.map(({ destination }) => destination)
      ).size
    ).toBe(7);
    for (const platform of platforms) {
      const destination = join(
        resolveSkillDestination({ platform, scope: 'global', homeDir }),
        skill.name
      );
      expect(lstatSync(destination).isSymbolicLink()).toBe(true);
      expect(realpathSync(destination)).toBe(
        realpathSync(join(canonicalSkillsDir, skill.name))
      );
    }
  });

  it('uses the Windows junction seam for global link installation', () => {
    const root = tempRoot();
    const skill = fixtureSkill(root);
    const canonicalSkillsDir = join(root, 'octocode-home', 'skills');
    const homeDir = join(root, 'home');
    const result = installBundledSkills({
      skills: [skill],
      canonicalSkillsDir,
      targets: [
        {
          platform: 'codex',
          scope: 'global',
          homeDir,
          operatingSystem: 'win32',
        },
      ],
      mode: 'symlink',
    });

    const destination = join(homeDir, '.agents/skills', skill.name);
    expect(result.skills[0]!.destinations[0]).toMatchObject({
      mode: 'symlink',
      status: 'linked',
      linkTarget: join(canonicalSkillsDir, skill.name),
    });
    expect(lstatSync(destination).isSymbolicLink()).toBe(true);
    expect(realpathSync(destination)).toBe(
      realpathSync(join(canonicalSkillsDir, skill.name))
    );
  });

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

  it('upgrades canonical bundled content without requiring force', () => {
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
    };
    expect(installBundledSkills(request).ok).toBe(true);
    writeFileSync(join(skill.sourcePath, 'SKILL.md'), '# Upgraded\n');

    const upgraded = installBundledSkills({ ...request, upgrade: true });

    expect(upgraded).toMatchObject({
      ok: true,
      action: 'upgrade',
      upgrade: true,
      summary: { upgraded: 1, unchanged: 1, conflicts: 0, failed: 0 },
    });
    expect(upgraded.skills[0]).toMatchObject({
      canonicalStatus: 'upgraded',
      destinations: [{ status: 'unchanged', mode: 'symlink' }],
    });
    expect(
      readFileSync(join(canonicalSkillsDir, skill.name, 'SKILL.md'), 'utf8')
    ).toBe('# Upgraded\n');
  });

  it('does not use upgrade to replace an arbitrary platform link', () => {
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
    };
    expect(installBundledSkills(request).ok).toBe(true);
    const destination = join(homeDir, '.agents/skills', skill.name);
    const unrelated = join(root, 'unrelated-skill');
    mkdirSync(unrelated, { recursive: true });
    writeFileSync(join(unrelated, 'SKILL.md'), '# User-owned\n');
    rmSync(destination, { recursive: true, force: true });
    symlinkSync(unrelated, destination, 'dir');
    writeFileSync(join(skill.sourcePath, 'SKILL.md'), '# Incoming\n');

    const upgraded = installBundledSkills({ ...request, upgrade: true });

    expect(upgraded.ok).toBe(false);
    expect(upgraded.summary).toMatchObject({ upgraded: 1, conflicts: 1 });
    expect(upgraded.skills[0]!.destinations[0]!.status).toBe('conflict');
    expect(realpathSync(destination)).toBe(realpathSync(unrelated));
    expect(readFileSync(join(destination, 'SKILL.md'), 'utf8')).toBe(
      '# User-owned\n'
    );
  });

  it('upgrades only managed copy targets and preserves arbitrary destination drift', () => {
    const root = tempRoot();
    const managedSkill = fixtureSkill(root, 'managed-skill');
    const driftedSkill = fixtureSkill(root, 'drifted-skill');
    const canonicalSkillsDir = join(root, 'octocode-home', 'skills');
    const homeDir = join(root, 'home');
    const request = {
      skills: [managedSkill, driftedSkill],
      canonicalSkillsDir,
      targets: [
        { platform: 'claude' as const, scope: 'global' as const, homeDir },
      ],
      mode: 'copy' as const,
    };
    expect(installBundledSkills(request).ok).toBe(true);
    writeFileSync(join(managedSkill.sourcePath, 'SKILL.md'), '# Managed v2\n');
    writeFileSync(join(driftedSkill.sourcePath, 'SKILL.md'), '# Bundled v2\n');
    const driftedDestination = join(
      homeDir,
      '.claude/skills',
      driftedSkill.name,
      'SKILL.md'
    );
    writeFileSync(driftedDestination, '# User edit\n');

    const upgraded = installBundledSkills({ ...request, upgrade: true });

    expect(upgraded.ok).toBe(false);
    expect(upgraded.summary).toMatchObject({
      upgraded: 2,
      copied: 1,
      conflicts: 1,
    });
    expect(upgraded.skills[0]!.destinations[0]!.status).toBe('copied');
    expect(upgraded.skills[1]!.destinations[0]!.status).toBe('conflict');
    expect(
      readFileSync(
        join(homeDir, '.claude/skills', managedSkill.name, 'SKILL.md'),
        'utf8'
      )
    ).toBe('# Managed v2\n');
    expect(readFileSync(driftedDestination, 'utf8')).toBe('# User edit\n');
    expect(
      readFileSync(
        join(canonicalSkillsDir, driftedSkill.name, 'SKILL.md'),
        'utf8'
      )
    ).toBe('# Bundled v2\n');
  });

  it('dry-runs an upgrade without changing canonical or managed copy content', () => {
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
    };
    expect(installBundledSkills(request).ok).toBe(true);
    writeFileSync(join(skill.sourcePath, 'SKILL.md'), '# Incoming\n');

    const result = installBundledSkills({
      ...request,
      upgrade: true,
      dryRun: true,
    });

    expect(result).toMatchObject({
      ok: true,
      action: 'dry-run',
      upgrade: true,
      summary: { upgraded: 1, copied: 1 },
    });
    expect(
      readFileSync(join(canonicalSkillsDir, skill.name, 'SKILL.md'), 'utf8')
    ).toBe('# Fixture\n');
    expect(
      readFileSync(
        join(homeDir, '.claude/skills', skill.name, 'SKILL.md'),
        'utf8'
      )
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

  it('uses symlinks in auto mode and copies only when explicitly requested', () => {
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
      { mode: 'symlink', status: 'linked' },
      { mode: 'symlink', status: 'linked' },
    ]);

    const copied = installBundledSkills({
      skills: [skill],
      canonicalSkillsDir,
      targets: [{ platform: 'claude', scope: 'global', homeDir }],
      mode: 'copy',
      force: true,
      dryRun: false,
    });
    expect(copied.skills[0]!.destinations[0]).toMatchObject({
      mode: 'copy',
      status: 'copied',
    });
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

  it('reports invalid project targets and deduplicates repeated destinations', () => {
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
          platform: 'claude',
          scope: 'project',
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
