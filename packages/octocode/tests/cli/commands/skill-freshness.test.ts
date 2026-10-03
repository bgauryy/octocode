import fs from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { contentFreshness } from '../../../src/cli/commands/skills/freshness.js';
import {
  checkSkill,
  overallStatus,
  type SkillCheckResult,
} from '../../../src/cli/commands/skills/checker.js';

import * as registry from '../../../src/cli/commands/skills/registry.js';
import * as home from '../../../src/cli/commands/skills/home.js';

describe('skill content freshness', () => {
  let root: string;
  let bundled: string;
  let installed: string;

  beforeEach(() => {
    root = fs.mkdtempSync(path.join(tmpdir(), 'octocode-skill-freshness-'));
    bundled = path.join(root, 'bundled', 'fixture-skill');
    installed = path.join(root, 'installed', 'fixture-skill');
    for (const dir of [bundled, installed]) {
      fs.mkdirSync(path.join(dir, 'references'), { recursive: true });
      fs.writeFileSync(path.join(dir, 'SKILL.md'), '# Fixture\n');
      fs.writeFileSync(path.join(dir, 'references', 'a.md'), 'alpha\n');
    }
  });

  afterEach(() => {
    vi.restoreAllMocks();
    fs.rmSync(root, { recursive: true, force: true });
  });

  it('identical copies read fresh', () => {
    expect(contentFreshness(bundled, installed)).toBe('fresh');
  });

  it('a symlink into the bundled source is fresh without hashing', () => {
    const link = path.join(root, 'link');
    fs.symlinkSync(bundled, link);
    expect(contentFreshness(bundled, link)).toBe('fresh');
  });

  it('edited installed content reads stale; re-materializing converges', () => {
    fs.writeFileSync(path.join(installed, 'references', 'a.md'), 'drifted\n');
    expect(contentFreshness(bundled, installed)).toBe('stale');
    // --fix re-materializes: copying bundled over installed converges.
    fs.cpSync(bundled, installed, { recursive: true, force: true });
    expect(contentFreshness(bundled, installed)).toBe('fresh');
  });

  it('an extra or missing file reads stale', () => {
    fs.writeFileSync(path.join(installed, 'extra.md'), 'new\n');
    expect(contentFreshness(bundled, installed)).toBe('stale');
    fs.rmSync(path.join(installed, 'extra.md'));
    fs.rmSync(path.join(installed, 'references', 'a.md'));
    expect(contentFreshness(bundled, installed)).toBe('stale');
  });

  it('rejects path or size differences before reading file bytes', () => {
    fs.writeFileSync(path.join(installed, 'extra.bin'), 'extra');
    const read = vi.spyOn(fs, 'readFileSync');
    expect(contentFreshness(bundled, installed)).toBe('stale');
    expect(read).not.toHaveBeenCalled();
    fs.rmSync(path.join(installed, 'extra.bin'));
    fs.writeFileSync(path.join(installed, 'SKILL.md'), 'different size');
    expect(contentFreshness(bundled, installed)).toBe('stale');
    expect(read).not.toHaveBeenCalled();
  });

  it('detects same-size edits and does not retain stale cache results', () => {
    expect(contentFreshness(bundled, installed)).toBe('fresh');
    fs.writeFileSync(path.join(installed, 'references', 'a.md'), 'bravo\n');
    expect(contentFreshness(bundled, installed)).toBe('stale');
    fs.writeFileSync(path.join(installed, 'references', 'a.md'), 'alpha\n');
    expect(contentFreshness(bundled, installed)).toBe('fresh');
  });

  it('treats directory symlink cycles as undetermined', () => {
    fs.symlinkSync(installed, path.join(installed, 'loop'), 'dir');
    expect(contentFreshness(bundled, installed)).toBeUndefined();
  });

  it('compares aliased install locations once per check without a persistent cache', () => {
    vi.spyOn(registry, 'getSkill').mockReturnValue({
      name: 'fixture-skill',
      folder: 'fixture-skill',
      description: 'fixture',
      dir: bundled,
    });
    vi.spyOn(home, 'getSkillsHome').mockReturnValue(path.dirname(installed));
    vi.spyOn(process, 'cwd').mockReturnValue(root);
    const workspace = path.join(root, '.agents', 'skills');
    fs.mkdirSync(workspace, { recursive: true });
    fs.symlinkSync(installed, path.join(workspace, 'fixture-skill'), 'dir');
    const read = vi.spyOn(fs, 'readFileSync');
    const check = checkSkill('fixture-skill', []);
    expect(check.home.content).toBe('fresh');
    expect(check.workspace.content).toBe('fresh');
    expect(read).toHaveBeenCalledTimes(4);
    fs.writeFileSync(path.join(installed, 'references', 'a.md'), 'bravo\n');
    expect(checkSkill('fixture-skill', []).home.content).toBe('stale');
  });

  it('missing paths are undetermined, not stale', () => {
    expect(contentFreshness(bundled, path.join(root, 'nope'))).toBeUndefined();
    expect(
      contentFreshness(path.join(root, 'nope'), installed)
    ).toBeUndefined();
  });

  it('overallStatus reports stale for an installed-but-drifted skill', () => {
    const location = (status: string, content?: 'fresh' | 'stale') => ({
      label: 'home',
      path: '/x',
      status: status as 'installed',
      ...(content ? { content } : {}),
    });
    const result: SkillCheckResult = {
      skillName: 'fixture',
      home: location('installed', 'stale'),
      platforms: [location('linked', 'fresh')],
      workspace: location('missing'),
    };
    expect(overallStatus(result)).toBe('stale');
    result.home.content = 'fresh';
    expect(overallStatus(result)).toBe('ok');
  });
});
