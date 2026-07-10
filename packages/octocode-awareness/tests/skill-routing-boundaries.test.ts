import { describe, expect, it } from 'vitest';
import { spawnSync } from 'node:child_process';
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
    expect(desc).toMatch(/^Use when an agent plans, edits, reviews, tests, or hands off work in a code repository/);
    expect(desc).toContain('Homeostatic Awareness Loop');
    expect(desc).toContain('solo across sessions');
    expect(desc).toContain('SQLite');
    expect(desc).toContain('optional hooks');
    expect(desc).toContain('advisory file presence');
    expect(desc).toContain('lock sensitive paths');
    expect(desc).toContain('bounded `.octocode` wiki');
    expect(desc).toContain('verify outcomes');
    expect(desc.length).toBeLessThanOrEqual(1024);
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

  it('ships train, near-miss, and held-out trigger cases', () => {
    const evalPath = resolve(PACKAGE_ROOT, 'skills/octocode-awareness/evals/trigger-cases.json');
    expect(existsSync(evalPath)).toBe(true);
    const cases = JSON.parse(readFileSync(evalPath, 'utf8')) as Record<string, Array<{ prompt: string; expect: boolean }>>;
    expect(cases['train_should_trigger']?.length).toBeGreaterThanOrEqual(10);
    expect(cases['train_near_miss']?.length).toBeGreaterThanOrEqual(10);
    expect(cases['held_out']?.length).toBeGreaterThanOrEqual(8);
    expect(cases['train_should_trigger']?.every((entry) => entry.expect)).toBe(true);
    expect(cases['train_near_miss']?.every((entry) => !entry.expect)).toBe(true);
    expect(cases['held_out']?.some((entry) => entry.expect)).toBe(true);
    expect(cases['held_out']?.some((entry) => !entry.expect)).toBe(true);
  });

  it('passes skill review with graph-routed progressive disclosure', () => {
    const reviewer = resolve(PACKAGE_ROOT, 'skills/octocode-skills/scripts/skill-review.mjs');
    const skillDir = resolve(PACKAGE_ROOT, 'skills/octocode-awareness');
    const result = spawnSync(process.execPath, [reviewer, skillDir, '--json'], { encoding: 'utf8' });
    expect(result.status, result.stderr).toBe(0);
    const report = JSON.parse(result.stdout) as { results: Array<{ findings: unknown[] }> };
    expect(report.results[0]?.findings).toEqual([]);
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
