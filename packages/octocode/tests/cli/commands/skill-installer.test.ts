import fs from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';

import { installSkill } from '../../../src/cli/commands/skills/installer.js';

describe('skill installer canonical home', () => {
  let root: string;
  let sourceDir: string;

  beforeEach(() => {
    root = fs.mkdtempSync(path.join(tmpdir(), 'octocode-skill-installer-'));
    sourceDir = path.join(root, 'source', 'fixture-skill');
    fs.mkdirSync(sourceDir, { recursive: true });
    fs.writeFileSync(path.join(sourceDir, 'SKILL.md'), '# Fixture\n');
  });

  afterEach(() => {
    fs.rmSync(root, { recursive: true, force: true });
  });

  it('copies once to canonical home and symlinks every vendor to it', () => {
    const outcome = installSkill({
      sourcePath: sourceDir,
      skillName: 'fixture-skill',
      platforms: ['claude', 'cursor', 'codex'],
      workspace: false,
      customPath: null,
      mode: 'symlink',
      force: true,
      dryRun: false,
      homeDir: path.join(root, 'home'),
      canonicalSkillsDir: path.join(root, '.octocode', 'skills'),
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

    for (const platform of ['claude', 'cursor', 'codex']) {
      const platformDir = platform === 'codex' ? '.agents' : `.${platform}`;
      const linkPath = path.join(
        root,
        'home',
        platformDir,
        'skills',
        'fixture-skill'
      );
      expect(fs.lstatSync(linkPath).isSymbolicLink()).toBe(true);
      expect(fs.realpathSync(linkPath)).toBe(fs.realpathSync(expectedHome));
    }
  });
});
