import {
  mkdtempSync,
  mkdirSync,
  readFileSync,
  existsSync,
  writeFileSync,
  rmSync,
  symlinkSync,
} from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { execFileSync } from 'node:child_process';
import { expect, it } from 'vitest';

it('stages skills cleanly and excludes local artifacts on every run', () => {
  const root = mkdtempSync(join(tmpdir(), 'octocode-skills-'));
  const source = join(root, 'source');
  const target = join(root, 'target');
  try {
    mkdirSync(join(source, 'research', '__pycache__'), { recursive: true });
    mkdirSync(join(target, 'removed-skill'), { recursive: true });
    writeFileSync(join(target, 'removed-skill', 'SKILL.md'), 'stale');
    writeFileSync(join(source, 'research', 'SKILL.md'), 'current');
    writeFileSync(join(source, 'research', '.env'), 'private fixture');
    writeFileSync(join(source, 'research', '.env.example'), 'example');
    writeFileSync(
      join(source, 'research', '__pycache__', 'cache.pyc'),
      'cache'
    );
    symlinkSync(join(source, 'research', 'SKILL.md'), join(source, 'linked'));
    execFileSync(process.execPath, [
      '--input-type=module',
      '-e',
      'const {stageSkills} = await import(process.argv[1]); stageSkills(process.argv[2], process.argv[3]);',
      resolve('scripts/stage-skills.mjs'),
      source,
      target,
    ]);
    expect(readFileSync(join(target, 'research', 'SKILL.md'), 'utf8')).toBe(
      'current'
    );
    for (const path of [
      'removed-skill',
      'linked',
      'research/.env',
      'research/.env.example',
      'research/__pycache__',
    ]) {
      expect(existsSync(join(target, path)), path).toBe(false);
    }
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});
