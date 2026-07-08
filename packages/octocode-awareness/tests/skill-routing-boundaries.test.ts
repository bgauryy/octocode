import { describe, expect, it } from 'vitest';
import { existsSync, readFileSync, readdirSync } from 'node:fs';
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

function referenceMarkdownFiles(skillName: string): string[] {
  const dir = resolve(PACKAGE_ROOT, 'skills', skillName, 'references');
  if (!existsSync(dir)) return [];
  return readdirSync(dir).filter((entry) => entry.endsWith('.md'));
}

describe('skill routing boundaries', () => {
  it('makes awareness the primary workflow skill', () => {
    const text = skill('octocode-awareness');
    const desc = description(text);
    expect(desc).toContain('Recall memory');
    expect(desc).toContain('claim locks');
    expect(desc).toContain('handle signals');
    expect(desc).toContain('record durable lessons');
    expect(text).toContain('primary skill for awareness, communication, reflection, learning, and hook guidance');
    expect(text).toContain('signal publish|list|reply|ack|resolve');
  });

  it('keeps communication as a route-only compatibility stub', () => {
    const text = skill('octocode-agent-communication');
    const desc = description(text);
    expect(desc).toContain('Compatibility alias');
    expect(text).toContain('Load `octocode-awareness` for message work');
    expect(text).toContain('Do not add operational script logic here');
    expect(existsSync(resolve(PACKAGE_ROOT, 'skills/octocode-agent-communication/scripts'))).toBe(false);
  });

  it('keeps reflection as a route-only compatibility stub', () => {
    const text = skill('octocode-reflection');
    const desc = description(text);
    expect(desc).toContain('Compatibility alias');
    expect(text).toContain('Load `octocode-awareness` for reflection');
    expect(text).toContain('Do not add operational script logic here');
    expect(existsSync(resolve(PACKAGE_ROOT, 'skills/octocode-reflection/scripts'))).toBe(false);
  });

  it('keeps generated runtime scripts only in the primary skill', () => {
    expect(existsSync(resolve(PACKAGE_ROOT, 'skills/octocode-awareness/scripts/awareness.mjs'))).toBe(true);
    expect(existsSync(resolve(PACKAGE_ROOT, 'skills/octocode-agent-communication/scripts/awareness.mjs'))).toBe(false);
    expect(existsSync(resolve(PACKAGE_ROOT, 'skills/octocode-reflection/scripts/awareness.mjs'))).toBe(false);
  });

  it('keeps compatibility stubs free of operational references', () => {
    expect(referenceMarkdownFiles('octocode-agent-communication')).toEqual([]);
    expect(referenceMarkdownFiles('octocode-reflection')).toEqual([]);
  });
});
