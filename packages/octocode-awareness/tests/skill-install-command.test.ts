import { existsSync, mkdtempSync, mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { afterEach, describe, expect, it } from 'vitest';
import { runSkillInstall } from '../src/skill-install-command.js';

const TEST_DIR = dirname(fileURLToPath(import.meta.url));
const PACKAGE_ROOT = resolve(TEST_DIR, '..');
const roots: string[] = [];

function tempRoot(): string {
  const root = mkdtempSync(join(tmpdir(), 'octocode-awareness-skill-install-'));
  roots.push(root);
  return root;
}

afterEach(() => {
  for (const root of roots.splice(0)) rmSync(root, { recursive: true, force: true });
});

describe('skill install command', () => {
  it('previews an explicit global platform destination without writing', () => {
    const root = tempRoot();
    const homeDir = join(root, 'home');
    const result = runSkillInstall(
      ['--platform', 'pi', '--global', '--dry-run'],
      { skillsDir: resolve(PACKAGE_ROOT, 'skills'), homeDir, cwd: root },
    );

    expect(result.exitCode).toBe(0);
    expect(result.payload).toMatchObject({
      ok: true,
      action: 'dry-run',
      skill: 'octocode-awareness',
      platform: 'pi',
      scope: 'global',
      destination: join(homeDir, '.pi/agent/skills/octocode-awareness'),
      changed: true,
    });
    expect(existsSync(join(homeDir, '.pi/agent/skills/octocode-awareness'))).toBe(false);
  });

  it('copies the bundled skill, is idempotent, and refuses drift unless forced', () => {
    const root = tempRoot();
    const projectDir = join(root, 'project');
    mkdirSync(projectDir, { recursive: true });
    const argv = ['--platform', 'shared', '--project-dir', projectDir];
    const options = { skillsDir: resolve(PACKAGE_ROOT, 'skills'), homeDir: join(root, 'home'), cwd: root };
    const destination = join(projectDir, '.agents/skills/octocode-awareness');
    const sourceSkill = resolve(PACKAGE_ROOT, 'skills/octocode-awareness/SKILL.md');

    const installed = runSkillInstall(argv, options);
    expect(installed.exitCode).toBe(0);
    expect(installed.payload).toMatchObject({ ok: true, action: 'install', changed: true, destination });
    expect(readFileSync(join(destination, 'SKILL.md'), 'utf8')).toBe(readFileSync(sourceSkill, 'utf8'));

    const unchanged = runSkillInstall(argv, options);
    expect(unchanged.exitCode).toBe(0);
    expect(unchanged.payload).toMatchObject({ ok: true, action: 'install', changed: false, destination });

    writeFileSync(join(destination, 'SKILL.md'), '# drift\n');
    const conflict = runSkillInstall(argv, options);
    expect(conflict.exitCode).toBe(1);
    expect(conflict.payload).toMatchObject({ ok: false, destination });
    expect(String(conflict.payload?.error)).toContain('--force');
    expect(readFileSync(join(destination, 'SKILL.md'), 'utf8')).toBe('# drift\n');

    const replaced = runSkillInstall([...argv, '--force'], options);
    expect(replaced.exitCode).toBe(0);
    expect(replaced.payload).toMatchObject({ ok: true, action: 'install', changed: true, destination });
    expect(readFileSync(join(destination, 'SKILL.md'), 'utf8')).toBe(readFileSync(sourceSkill, 'utf8'));
  });

  it('requires an explicit supported platform and scope', () => {
    const root = tempRoot();
    const options = { skillsDir: resolve(PACKAGE_ROOT, 'skills'), homeDir: join(root, 'home'), cwd: root };

    expect(runSkillInstall([], options).payload?.error).toContain('--platform');
    expect(runSkillInstall(['--platform', 'unknown', '--global'], options).payload?.error).toContain('unsupported platform');
    expect(runSkillInstall(['--platform', 'shared'], options).payload?.error).toContain('--global or --project-dir');
    expect(runSkillInstall(['--platform', 'shared', '--global', '--project-dir', root], options).payload?.error).toContain('either --global or --project-dir');
  });
});
