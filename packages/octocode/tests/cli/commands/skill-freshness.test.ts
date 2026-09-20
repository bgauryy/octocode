import fs from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';

import {
  contentFreshness,
  hashDirContent,
} from '../../../src/cli/commands/skills/freshness.js';
import {
  overallStatus,
  type SkillCheckResult,
} from '../../../src/cli/commands/skills/checker.js';

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
    fs.rmSync(root, { recursive: true, force: true });
  });

  it('identical copies hash equal and read fresh', () => {
    expect(hashDirContent(bundled)).toBe(hashDirContent(installed));
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
