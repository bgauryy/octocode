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
    expect(desc).toContain('Run the compact Awareness CLI');
    expect(desc).toContain('memory recall');
    expect(desc).toContain('file locks');
    expect(desc).toContain('signals');
    expect(desc).toContain('repo context');
    expect(text).toContain('single operational awareness skill');
    expect(text).toContain('schema commands --compact');
    expect(text).toContain('signal publish|list|reply|ack|resolve');
  });

  it('does not ship retired routing stub directories', () => {
    expect(existsSync(resolve(PACKAGE_ROOT, 'skills/octocode-agent-communication'))).toBe(false);
    expect(existsSync(resolve(PACKAGE_ROOT, 'skills/octocode-reflection'))).toBe(false);
  });

  it('keeps generated runtime scripts only in the primary skill', () => {
    expect(existsSync(resolve(PACKAGE_ROOT, 'skills/octocode-awareness/scripts/awareness.mjs'))).toBe(true);
  });
});
