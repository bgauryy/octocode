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

function awarenessSkillFile(path: string): string {
  return readFileSync(resolve(PACKAGE_ROOT, 'skills/octocode-awareness', path), 'utf8');
}

function description(markdown: string): string {
  const match = markdown.match(/^---\n[\s\S]*?description:\s*"([^"]+)"[\s\S]*?\n---/);
  return match?.[1] ?? '';
}

describe('skill routing boundaries', () => {
  it('makes awareness the primary workflow skill', () => {
    const text = skill('octocode-awareness');
    const desc = description(text);
    expect(desc).toContain('Use always when working in a workspace');
    expect(desc).toContain('shared-repo coordination');
    expect(desc).toContain('multiple agents');
    expect(desc).toContain('awareness');
    expect(desc).toContain('collaboration');
    expect(desc).toContain('learning');
    expect(desc).toContain('memory');
    expect(desc).toContain('bookkeeping');
    expect(desc).toContain('housekeeping');
    expect(desc).toContain('hooks');
    expect(desc).toContain('reflection');
    expect(desc).not.toContain('dogfood');
    expect(desc).not.toContain('packages/octocode-awareness');
    expect(text).toContain('## Workflow');
    expect(text).toContain('Features → refs');
    expect(text).toContain('ATTEND');
    expect(text).toContain('BOOKKEEP');
    expect(text).toContain('HOUSEKEEP');
    expect(text).toContain('work start');
    expect(text).toContain('ordinary overlap is allowed');
    expect(text).toContain('schema commands --compact');
    expect(text).toContain('## Installation');
    expect(text).toContain('agent-cheatsheet.md');
    expect(text).toContain('Core (most sessions)');
    expect(text).toContain('bookkeeping');
    expect(text).toContain('Housekeeping');
    expect(text).toContain('docs list --compact');
    expect(text).toContain('yarn workspace @octocodeai/octocode-awareness build');
    expect(text).toContain('scripts/smoke-multi-agent.mjs');
    expect(existsSync(resolve(PACKAGE_ROOT, 'skills/octocode-skills/SKILL.md'))).toBe(true);
    expect(existsSync(resolve(PACKAGE_ROOT, 'skills/octocode-skills/scripts/skill-review.mjs'))).toBe(true);
    expect(existsSync(resolve(PACKAGE_ROOT, 'skills/octocode-skills/scripts/skill-lint.mjs'))).toBe(true);
  });

  it('does not ship retired routing stub directories', () => {
    expect(existsSync(resolve(PACKAGE_ROOT, 'skills/octocode-agent-communication'))).toBe(false);
    expect(existsSync(resolve(PACKAGE_ROOT, 'skills/octocode-reflection'))).toBe(false);
  });

  it('keeps generated runtime scripts only in the primary skill', () => {
    expect(existsSync(resolve(PACKAGE_ROOT, 'skills/octocode-awareness/scripts/awareness.mjs'))).toBe(true);
  });

  it('keeps standalone guidance portable outside the monorepo', () => {
    const readme = awarenessSkillFile('README.md');
    const tooling = awarenessSkillFile('references/agent-cheatsheet-tooling.md');
    const octocode = awarenessSkillFile('references/octocode.md');
    const dataModel = awarenessSkillFile('references/data-model.md');
    const repoContext = awarenessSkillFile('references/repo-context-management.md');
    const combined = [readme, tooling, octocode, dataModel, repoContext].join('\n');

    expect(combined).not.toMatch(/<package>|<awareness-package>|default for this monorepo/);
    expect(combined).not.toContain('package migration truth: `docs/DB.md`');
    expect(readme).toContain('$(npm root --global)/@octocodeai/octocode-awareness/dist/skills/octocode-awareness');
    expect(tooling).toContain('$(npm root --global)/@octocodeai/octocode-awareness/dist/skills/octocode-skills');
    expect(octocode).toContain('references/agent-cheatsheet-tooling.md');
  });
});
