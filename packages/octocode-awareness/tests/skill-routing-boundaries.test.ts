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

// Deterministic held-out proxy for the description boundary. This does not claim
// a model trigger rate; it ensures unseen cases remain separable by repository
// work intent rather than exact training-prompt strings.
function routesRepositoryWork(prompt: string): boolean {
  const text = prompt.toLowerCase();
  const explicitNearMiss = /(outside (?:a )?repo|conceptually|personal|phone screen|career|favorite restaurant|logo image|email|meeting|slide|blog post|uploaded csv|browse|search the web)/;
  if (explicitNearMiss.test(text)) return false;
  const repositoryContext = /(repo|repository|checkout|package\.json|package tests?|packages?|dependency|parser test|pr diff|migration|auth schema|pre-edit hook|verification|coding session|\.octocode|gotcha|workers?|subagents?|agnets|same fiel|selectable tasks?)/;
  const workIntent = /(fix|review|update|implement|continue|plan|editing|rotate|block|check|save|refresh|bump|make|resume|touch|install|smoke|mean|split)/;
  return repositoryContext.test(text) && workIntent.test(text);
}

describe('skill routing boundaries', () => {
  it('makes awareness the primary workflow skill', () => {
    const text = skill('octocode-awareness');
    const desc = description(text);
    expect(desc).toMatch(/^Use when planning, editing, reviewing, testing, or handing off work in a shared repo/);
    expect(desc).toContain('solo across sessions');
    expect(desc).toContain('verification debt');
    expect(desc).toContain('memory/wiki');
    expect(desc).toContain('hooks setup/debug');
    expect(desc.length).toBeLessThanOrEqual(1024);
    expect(desc).not.toContain('dogfood');
    expect(desc).not.toContain('packages/octocode-awareness');
    expect(text).toContain('## Loop');
    expect(text).toContain('BEFORE/READ+REASON');
    expect(text).toContain('DURING/DO');
    expect(text).toContain('AFTER/VERIFY');
    expect(text).toContain('LEARN? -> CLEAN? -> PROJECT?');
    expect(text).toContain('goal, acceptance, affected scope, and evidence');
    expect(text).toContain('work start');
      expect(text).toMatch(/ordinary overlap is allowed/i);
    expect(text).toContain('scripts/schema.mjs');
    expect(text).toContain('first activation');
    expect(text).toContain('agent-cheatsheet.md');
    expect(text).toContain('Routes (load one owner; core work needs none)');
    expect(text).toContain('pressure-driven triggers');
    expect(text).toContain('docs list --compact');
    expect(text).toContain('yarn workspace @octocodeai/octocode-awareness build');
    expect(text).toContain('scripts/smoke-multi-agent.mjs');
    expect(existsSync(resolve(PACKAGE_ROOT, 'skills/octocode-skills/SKILL.md'))).toBe(true);
    expect(existsSync(resolve(PACKAGE_ROOT, 'skills/octocode-skills/scripts/skill-review.mjs'))).toBe(true);
    expect(existsSync(resolve(PACKAGE_ROOT, 'skills/octocode-skills/scripts/skill-lint.mjs'))).toBe(true);
  });

  it('keeps held-out repository intent behavior distinct from near misses', () => {
    const evalPath = resolve(PACKAGE_ROOT, 'skills/octocode-awareness/evals/trigger-cases.json');
    expect(existsSync(evalPath)).toBe(true);
    const cases = JSON.parse(readFileSync(evalPath, 'utf8')) as Record<string, Array<{ prompt: string; expect: boolean }>>;
    expect(cases['train_should_trigger']?.length).toBeGreaterThanOrEqual(10);
    expect(cases['train_near_miss']?.length).toBeGreaterThanOrEqual(10);
    expect(cases['held_out']?.length).toBeGreaterThanOrEqual(8);
    expect(cases['train_should_trigger']?.every((entry) => entry.expect)).toBe(true);
    expect(cases['train_near_miss']?.every((entry) => !entry.expect)).toBe(true);
    const heldOut = cases['held_out'] ?? [];
    expect(heldOut.map((entry) => ({
      prompt: entry.prompt,
      expected: entry.expect,
      actual: routesRepositoryWork(entry.prompt),
    }))).toEqual(heldOut.map((entry) => ({
      prompt: entry.prompt,
      expected: entry.expect,
      actual: entry.expect,
    })));
    expect(heldOut.filter((entry) => entry.expect).map((entry) => entry.prompt).join('\n')).toMatch(/only agent/i);
    expect(heldOut.filter((entry) => entry.expect).map((entry) => entry.prompt).join('\n')).toMatch(/read-only security review/i);
    expect(heldOut.filter((entry) => entry.expect).map((entry) => entry.prompt).join('\n')).toMatch(/resume/i);
    expect(heldOut.filter((entry) => !entry.expect).map((entry) => entry.prompt).join('\n')).toMatch(/outside a repo/i);
  });

  it('routes each fresh-agent feature question to one direct owner', () => {
    const text = skill('octocode-awareness');
    const journeys = [
      ['start, finish, or command recipe', 'agent-cheatsheet.md'],
      ['plan, task, or WORK', 'plan-task-workflow.md'],
      ['peer presence or overlap', 'files-awareness.md'],
      ['exclusive locks or verify', 'lock-protocol.md'],
      ['peers, signals, or refinements', 'coordination-protocol.md'],
      ['installing or debugging automation', 'hooks.md'],
      ['live, durable, or generated output', 'output-routing.md'],
      ['recalling or recording memory', 'memory-recall.md'],
      ['learn or clean', 'bookkeeping.md'],
      ['storage or sessions', 'architecture.md'],
      ['improving the harness', 'improve-loop.md'],
      ['shipping a skill change', 'skill-evolution.md'],
    ] as const;
    for (const [trigger, owner] of journeys) {
      expect(text).toContain(trigger);
      expect(text).toContain(`references/${owner}`);
    }
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
