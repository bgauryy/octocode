import fs from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

vi.mock('@octocodeai/config', () => ({
  getOctocodeHome: () => process.env['OCTOCODE_SKILL_TEST_HOME'],
}));

vi.mock('../../../src/cli/commands/skills/platforms.js', () => ({
  getPlatformSkillsDir: (platform: string) =>
    path.join(
      process.env['OCTOCODE_SKILL_TEST_ROOT']!,
      'vendors',
      platform,
      'skills'
    ),
}));

import { installSkill } from '../../../src/cli/commands/skills/installer.js';

describe('skill installer canonical home', () => {
  let root: string;
  let sourceDir: string;

  beforeEach(() => {
    root = fs.mkdtempSync(path.join(tmpdir(), 'octocode-skill-installer-'));
    sourceDir = path.join(root, 'source', 'fixture-skill');
    fs.mkdirSync(sourceDir, { recursive: true });
    fs.writeFileSync(path.join(sourceDir, 'SKILL.md'), '# Fixture\n');
    process.env['OCTOCODE_SKILL_TEST_ROOT'] = root;
    process.env['OCTOCODE_SKILL_TEST_HOME'] = path.join(root, '.octocode');
  });

  afterEach(() => {
    delete process.env['OCTOCODE_SKILL_TEST_ROOT'];
    delete process.env['OCTOCODE_SKILL_TEST_HOME'];
    fs.rmSync(root, { recursive: true, force: true });
  });

  it('copies once to canonical home and symlinks every vendor to it', () => {
    const outcome = installSkill({
      sourcePath: sourceDir,
      skillName: 'fixture-skill',
      platforms: ['claude', 'cursor', 'codex-native'],
      workspace: false,
      customPath: null,
      mode: 'symlink',
      force: true,
      dryRun: false,
    });

    const expectedHome = path.join(
      root,
      '.octocode',
      'skills',
      'fixture-skill'
    );
    expect(outcome.homePath).toBe(expectedHome);
    expect(outcome.homeStatus).toBe('installed');
    expect(fs.lstatSync(expectedHome).isDirectory()).toBe(true);
    expect(fs.lstatSync(expectedHome).isSymbolicLink()).toBe(false);

    for (const platform of ['claude', 'cursor', 'codex-native']) {
      const linkPath = path.join(
        root,
        'vendors',
        platform,
        'skills',
        'fixture-skill'
      );
      expect(fs.lstatSync(linkPath).isSymbolicLink()).toBe(true);
      expect(fs.realpathSync(linkPath)).toBe(fs.realpathSync(expectedHome));
    }
  });
});
