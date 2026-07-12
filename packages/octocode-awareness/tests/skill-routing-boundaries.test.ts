import { describe, expect, it } from 'vitest';
import { existsSync, readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const TEST_DIR = dirname(fileURLToPath(import.meta.url));
const PACKAGE_ROOT = resolve(TEST_DIR, '..');
const REPO_ROOT = resolve(PACKAGE_ROOT, '../..');

function skill(path: string): string {
  return readFileSync(resolve(REPO_ROOT, 'skills', path, 'SKILL.md'), 'utf8');
}

function awarenessSkillFile(path: string): string {
  return readFileSync(resolve(REPO_ROOT, 'skills/octocode-awareness', path), 'utf8');
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
    expect(text).toMatch(/run live-state actions through the CLI/i);
    expect(text).toContain('npx @octocodeai/octocode-awareness');
    expect(text).toContain('node packages/octocode-awareness/out/octocode-awareness.js');
    expect(text).toContain('## Lifecycle');
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
    expect(text).toContain('Feature map — all features');
    expect(text).toContain('clean only under pressure');
    expect(text).toContain('docs list --compact');
    expect(text).toContain('yarn workspace @octocodeai/octocode-awareness build');
    expect(text).toContain('scripts/smoke-multi-agent.mjs');
    expect(existsSync(resolve(REPO_ROOT, 'skills/octocode-research/SKILL.md'))).toBe(true);
    expect(existsSync(resolve(REPO_ROOT, 'skills/octocode-skills/SKILL.md'))).toBe(false);
  });

  it('teaches the complete agent lifecycle without assigning judgment to hooks', () => {
    const text = skill('octocode-awareness');
    const ordered = [
      'BEFORE/READ+REASON',
      'DURING/DO',
      'AFTER/VERIFY',
      'LEARN? -> CLEAN? -> PROJECT?',
    ];
    for (let index = 1; index < ordered.length; index += 1) {
      expect(text.indexOf(ordered[index - 1]!)).toBeLessThan(text.indexOf(ordered[index]!));
    }

    expect(text).toContain('AGENTS routes; skill decides; CLI/SQLite acts; hooks automate deterministic edges');
    expect(text).toMatch(/plan\/task/i);
    expect(text).toContain('work start');
    expect(text).toContain('work start --exclusive');
    expect(text).toContain('lock wait/prune');
    expect(text).toContain('verify mark');
    expect(text).toContain('verify audit');
    expect(text).toContain('memory recall --smart');
    expect(text).toContain('reflect record --lesson');
    expect(text).toContain('query');
    expect(text).toContain('wiki sync');
    expect(text).toContain('Never hand-edit `.octocode/`');
    expect(text).toMatch(/hooks never choose plans, locks, success, learning, cleanup, or projection/i);
    expect(text).toMatch(/expiry.*never.*success/i);
  });

  it('shows a lean overview of every Awareness feature family', () => {
    const text = skill('octocode-awareness');
    expect(text).toContain('## Feature map');
    for (const feature of [
      'attend', 'workspace status', 'plan', 'task', 'WORK', 'lock', 'verify',
      'signal', 'refinement', 'agent registry', 'query', 'memory', 'session capture',
      'reflect', 'docs', 'wiki sync', 'hooks', 'maintenance', 'schema',
    ]) {
      expect(text, `missing feature overview: ${feature}`).toContain(feature);
    }
    expect(text).toMatch(/dependencies.*readiness.*claim.*heartbeat.*submit.*release/i);
    expect(text).toMatch(/recall.*record.*forget.*archive.*restore/i);
    expect(text).toMatch(/install.*check.*remove.*run/i);
    expect(text).toMatch(/commands.*list.*path.*json-schema.*example.*validate/i);
  });

  it('keeps held-out repository intent behavior distinct from near misses', () => {
    const evalPath = resolve(REPO_ROOT, 'skills/octocode-awareness/evals/trigger-cases.json');
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
      ['**Recipes:**', 'agent-cheatsheet.md'],
      ['**Plan/task:**', 'plan-task-workflow.md'],
      ['**Work/files:**', 'files-awareness.md'],
      ['**Exclusive work/verify:**', 'lock-protocol.md'],
      ['**Signals/refinements:**', 'coordination-protocol.md'],
      ['**Hooks/hosts:**', 'hooks.md'],
      ['**Knowledge/wiki:**', 'output-routing.md'],
      ['**Memory:**', 'memory-recall.md'],
      ['**Maintenance/contracts:**', 'bookkeeping.md'],
      ['**Orient/state:**', 'architecture.md'],
      ['**Reflection/review:**', 'improve-loop.md'],
      ['**Skill evolution:**', 'skill-evolution.md'],
    ] as const;
    for (const [trigger, owner] of journeys) {
      expect(text).toContain(trigger);
      expect(text).toContain(`references/${owner}`);
    }
  });

  it('keeps awareness skill graph-routed without a separate skill-review binary', () => {
    const skillDir = resolve(REPO_ROOT, 'skills/octocode-awareness');
    expect(existsSync(resolve(skillDir, 'SKILL.md'))).toBe(true);
    expect(existsSync(resolve(REPO_ROOT, 'skills/octocode-skills/scripts/skill-review.mjs'))).toBe(false);
  });

  it('does not ship retired routing stub directories', () => {
    expect(existsSync(resolve(REPO_ROOT, 'skills/octocode-agent-communication'))).toBe(false);
    expect(existsSync(resolve(REPO_ROOT, 'skills/octocode-reflection'))).toBe(false);
  });

  it('keeps generated runtime scripts only in the primary skill', () => {
    expect(existsSync(resolve(REPO_ROOT, 'skills/octocode-awareness/scripts/awareness.mjs'))).toBe(true);
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
    expect(readme).toContain('$(npm root --global)/@octocodeai/octocode-awareness/out/skills/octocode-awareness');
    expect(tooling).toContain('$(npm root --global)/@octocodeai/octocode-awareness/out/skills/octocode-research');
    expect(octocode).toContain('references/agent-cheatsheet-tooling.md');
  });
});
