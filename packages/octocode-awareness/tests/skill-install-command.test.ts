import {
  existsSync,
  lstatSync,
  mkdtempSync,
  mkdirSync,
  readFileSync,
  realpathSync,
  rmSync,
  writeFileSync,
} from 'node:fs';
import { spawnSync } from 'node:child_process';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { afterEach, describe, expect, it } from 'vitest';
import { runSkillInstall } from '../src/skill-install-command.js';

const TEST_DIR = dirname(fileURLToPath(import.meta.url));
const PACKAGE_ROOT = resolve(TEST_DIR, '..');
const BUILT_CLI = resolve(PACKAGE_ROOT, 'out/octocode-awareness.js');
const roots: string[] = [];

function tempRoot(): string {
  const root = mkdtempSync(join(tmpdir(), 'octocode-awareness-skill-install-'));
  roots.push(root);
  return root;
}

afterEach(() => {
  for (const root of roots.splice(0))
    rmSync(root, { recursive: true, force: true });
});

describe('skill install command', () => {
  it('locates the packaged skill from the real code-split CLI', () => {
    const root = tempRoot();
    const projectDir = join(root, 'project');
    const octocodeHome = join(root, 'octocode-home');
    mkdirSync(projectDir, { recursive: true });
    const execution = spawnSync(
      process.execPath,
      [
        BUILT_CLI,
        'skill',
        'install',
        '--platform',
        'codex',
        '--project-dir',
        projectDir,
        '--compact',
      ],
      {
        cwd: projectDir,
        encoding: 'utf8',
        env: { ...process.env, OCTOCODE_HOME: octocodeHome },
        timeout: 10_000,
      }
    );
    expect(execution.status, execution.stderr || execution.stdout).toBe(0);
    const payload = JSON.parse(execution.stdout) as {
      skills: Array<{
        source: string;
        destinations: Array<{ destination: string }>;
      }>;
    };
    expect(payload.skills[0]!.source).toContain(
      join('out', 'skills', 'octocode-awareness')
    );
    const destination = payload.skills[0]!.destinations[0]!.destination;
    expect(lstatSync(destination).isSymbolicLink()).toBe(true);
    expect(realpathSync(destination)).toBe(
      realpathSync(join(octocodeHome, 'skills/octocode-awareness'))
    );
  });

  it('previews an explicit global platform destination without writing', () => {
    const root = tempRoot();
    const homeDir = join(root, 'home');
    const canonicalSkillsDir = join(root, 'octocode-home', 'skills');
    const result = runSkillInstall(
      ['--platform', 'pi', '--global', '--dry-run'],
      {
        skillsDir: resolve(PACKAGE_ROOT, 'skills'),
        homeDir,
        canonicalSkillsDir,
        cwd: root,
      }
    );

    expect(result.exitCode).toBe(0);
    expect(result.payload).toMatchObject({
      ok: true,
      action: 'dry-run',
      canonicalSkillsDir,
      skills: [
        {
          name: 'octocode-awareness',
          canonical: join(canonicalSkillsDir, 'octocode-awareness'),
          destinations: [
            {
              platform: 'pi',
              scope: 'global',
              destination: join(homeDir, '.pi/agent/skills/octocode-awareness'),
              mode: 'symlink',
              status: 'linked',
            },
          ],
        },
      ],
    });
    expect(
      existsSync(join(homeDir, '.pi/agent/skills/octocode-awareness'))
    ).toBe(false);
  });

  it('materializes the bundled skill, links it, and refuses destination drift unless forced', () => {
    const root = tempRoot();
    const projectDir = join(root, 'project');
    mkdirSync(projectDir, { recursive: true });
    const argv = ['--platform', 'shared', '--project-dir', projectDir];
    const canonicalSkillsDir = join(root, 'octocode-home', 'skills');
    const options = {
      skillsDir: resolve(PACKAGE_ROOT, 'skills'),
      homeDir: join(root, 'home'),
      canonicalSkillsDir,
      cwd: root,
    };
    const destination = join(projectDir, '.agents/skills/octocode-awareness');
    const canonical = join(canonicalSkillsDir, 'octocode-awareness');
    const sourceSkill = resolve(
      PACKAGE_ROOT,
      'skills/octocode-awareness/SKILL.md'
    );

    const installed = runSkillInstall(argv, options);
    expect(installed.exitCode).toBe(0);
    expect(installed.payload).toMatchObject({
      ok: true,
      action: 'install',
      skills: [
        {
          name: 'octocode-awareness',
          canonical,
          canonicalStatus: 'installed',
          destinations: [
            { destination, status: 'linked', linkTarget: canonical },
          ],
        },
      ],
    });
    expect(lstatSync(canonical).isDirectory()).toBe(true);
    expect(lstatSync(destination).isSymbolicLink()).toBe(true);
    expect(realpathSync(destination)).toBe(realpathSync(canonical));
    expect(readFileSync(join(destination, 'SKILL.md'), 'utf8')).toBe(
      readFileSync(sourceSkill, 'utf8')
    );

    const unchanged = runSkillInstall(argv, options);
    expect(unchanged.exitCode).toBe(0);
    expect(unchanged.payload).toMatchObject({
      ok: true,
      summary: {
        installed: 0,
        linked: 0,
        unchanged: 2,
        conflicts: 0,
        failed: 0,
      },
    });

    rmSync(destination, { recursive: true, force: true });
    mkdirSync(destination, { recursive: true });
    writeFileSync(join(destination, 'SKILL.md'), '# drift\n');
    const conflict = runSkillInstall(argv, options);
    expect(conflict.exitCode).toBe(1);
    expect(conflict.payload).toMatchObject({
      ok: false,
      summary: { conflicts: 1 },
      skills: [{ destinations: [{ destination, status: 'conflict' }] }],
    });
    expect(readFileSync(join(destination, 'SKILL.md'), 'utf8')).toBe(
      '# drift\n'
    );

    const replaced = runSkillInstall([...argv, '--force'], options);
    expect(replaced.exitCode).toBe(0);
    expect(replaced.payload).toMatchObject({
      ok: true,
      skills: [{ destinations: [{ destination, status: 'linked' }] }],
    });
    expect(lstatSync(destination).isSymbolicLink()).toBe(true);
    expect(readFileSync(join(destination, 'SKILL.md'), 'utf8')).toBe(
      readFileSync(sourceSkill, 'utf8')
    );
  });

  it('requires an explicit supported platform and scope', () => {
    const root = tempRoot();
    const options = {
      skillsDir: resolve(PACKAGE_ROOT, 'skills'),
      homeDir: join(root, 'home'),
      canonicalSkillsDir: join(root, 'octocode-home', 'skills'),
      cwd: root,
    };

    expect(runSkillInstall([], options).payload?.error).toContain('--platform');
    expect(
      runSkillInstall(['--platform', 'unknown', '--global'], options).payload
        ?.error
    ).toContain('Unknown platform');
    expect(
      runSkillInstall(['--platform', 'shared'], options).payload?.error
    ).toContain('--global or --project-dir');
    expect(
      runSkillInstall(
        ['--platform', 'shared', '--global', '--project-dir', root],
        options
      ).payload?.error
    ).toContain('either --global or --project-dir');
    const allProject = runSkillInstall(
      ['--platform', 'all', '--project-dir', root, '--dry-run'],
      options
    );
    expect(allProject.exitCode).toBe(0);
    const allPayload = allProject.payload as {
      ok: boolean;
      skills: Array<{ destinations: unknown[] }>;
    };
    expect(allPayload.ok).toBe(true);
    expect(allPayload.skills[0]!.destinations).toHaveLength(7);
  });
});
