import { describe, expect, it } from 'vitest';
import { existsSync, readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { dirname } from 'node:path';

const TEST_DIR = dirname(fileURLToPath(import.meta.url));
const PACKAGE_ROOT = resolve(TEST_DIR, '..');

function skill(path: string): string {
  return readFileSync(resolve(PACKAGE_ROOT, 'skills', path, 'SKILL.md'), 'utf8');
}

function description(markdown: string): string {
  const match = markdown.match(/^---\n[\s\S]*?description:\s*"([^"]+)"[\s\S]*?\n---/);
  return match?.[1] ?? '';
}

describe('skill routing boundaries', () => {
  it('makes awareness the primary workflow skill', () => {
    const text = skill('octocode-awareness');
    const desc = description(text);
    expect(desc).toContain('shared repo work needs awareness');
    expect(desc).toContain('before planning');
    expect(desc).toContain('during edits');
    expect(desc).toContain('after verification');
    expect(desc).toContain('hooks');
    expect(desc).toContain('locks/signals');
    expect(desc).toContain('reflection');
    expect(text).toMatch(/Use for shared-repo awareness|Trigger this skill for shared repo work/);
    expect(text).toContain('schema commands --compact');
    expect(text).toContain('signal publish|reply|ack|resolve');
    expect(text).toContain('Installation / Init Flow');
    expect(text).toContain('maintenance init --compact');
    expect(text).toContain('hooks install --host codex');
    expect(text).toContain('Codex/Cursor/Pi need host wiring');
    expect(text).toContain('agent-cheatsheet.md');
    expect(text).toMatch(/Agent smoke|Smoke:/);
    expect(text).toContain('octocode-skills');
    expect(existsSync(resolve(PACKAGE_ROOT, 'skills/octocode-skills/SKILL.md'))).toBe(true);
    expect(existsSync(resolve(PACKAGE_ROOT, 'skills/octocode-skills/scripts/skill-lint.mjs'))).toBe(true);
  });

  it('does not ship retired routing stub directories', () => {
    expect(existsSync(resolve(PACKAGE_ROOT, 'skills/octocode-agent-communication'))).toBe(false);
    expect(existsSync(resolve(PACKAGE_ROOT, 'skills/octocode-reflection'))).toBe(false);
  });

  it('keeps generated runtime scripts only in the primary skill', () => {
    expect(existsSync(resolve(PACKAGE_ROOT, 'skills/octocode-awareness/scripts/awareness.mjs'))).toBe(true);
  });
});
