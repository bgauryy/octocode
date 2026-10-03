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
    mkdirSync(join(source, 'compiled', 'src'), { recursive: true });
    mkdirSync(join(source, 'compiled', 'scripts'), { recursive: true });
    writeFileSync(
      join(source, 'compiled', 'package.json'),
      JSON.stringify({ files: ['SKILL.md', 'scripts'] })
    );
    writeFileSync(join(source, 'compiled', 'SKILL.md'), 'compiled skill');
    writeFileSync(join(source, 'compiled', 'src', 'main.rs'), 'source');
    writeFileSync(join(source, 'compiled', 'scripts', 'launcher'), 'runtime');
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
    expect(
      readFileSync(join(target, 'compiled', 'scripts', 'launcher'), 'utf8')
    ).toBe('runtime');
    expect(existsSync(join(target, 'compiled', 'SKILL.md'))).toBe(true);
    for (const path of [
      'removed-skill',
      'linked',
      'research/.env',
      'research/.env.example',
      'research/__pycache__',
      'compiled/src',
      'compiled/package.json',
    ]) {
      expect(existsSync(join(target, path)), path).toBe(false);
    }
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});
