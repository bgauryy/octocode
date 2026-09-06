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

function awarenessSkillFile(path: string): string {
  return readFileSync(resolve(PACKAGE_ROOT, 'skills/octocode-awareness', path), 'utf8');
}

function description(markdown: string): string {
  const match = markdown.match(/^---\n[\s\S]*?description:\s*"([^"]+)"[\s\S]*?\n---/);
  return match?.[1] ?? '';
}

// Deterministic held-out proxy for the description boundary. This does not claim
// a model trigger rate; it ensures unseen cases remain separable by actionable
// shared-state signals rather than ordinary repository intent.
function routesAwarenessSignal(prompt: string): boolean {
  const text = prompt.toLowerCase();
  const explicitNearMiss = /(outside (?:a )?repo|conceptually|personal|phone screen|career|favorite restaurant|logo image|email|meeting|slide|blog post|uploaded csv|browse|search the web)/;
  if (explicitNearMiss.test(text)) return false;
  const repositoryContext = /(repo|repository|checkout|package\.json|packages?|migration|pre-edit hook|verification|coding session|\.octocode|gotcha|workers?|subagents?|agnets|same fiel|selectable tasks?)/;
  const actionableSignal = /(another agent|workers?|subagents?|same fiel|selectable tasks?|migration|pre-edit hook|pending verification|verification debt|gotcha|current tasks and memory|last host|resume|awareness hooks?|\.octocode)/;
  return repositoryContext.test(text) && actionableSignal.test(text);
}

describe('skill routing boundaries', () => {
  it('routes awareness on actionable shared-state signals', () => {
    const text = skill('octocode-awareness');
    const desc = description(text);
    expect(desc).toMatch(/^Use when shared repository state can change the next action:/);
    expect(desc).toContain('peers');
    expect(desc).toContain('plans');
    expect(desc).toContain('verification debt');
    expect(desc).toContain('handoffs');
    expect(desc).toContain('Skip routine solo work');
    expect(desc.length).toBeLessThanOrEqual(1024);
    expect(desc).not.toContain('dogfood');
    expect(desc).not.toContain('packages/octocode-awareness');
    expect(text).toContain('NOTICE → SCOPE/IDENTITY → INSPECT → ACT → OBSERVE → SETTLE/VERIFY → LEARN');
    expect(text).toContain('@octocodeai/octocode-awareness');
    expect(text).toContain('npx @octocodeai/octocode-awareness');
    expect(text).toMatch(/public runner[^.]*npx @octocodeai\/octocode-awareness/i);
    expect(text).toContain('npx @octocodeai/octocode-awareness docs list --compact');
    expect(text).toContain('npx @octocodeai/octocode-awareness skill install --platform shared --project-dir "$PWD" --dry-run');
    expect(text).toContain('skill install --help');
    expect(text).not.toContain('npx -p @octocodeai/octocode-awareness octocode-awareness');
    expect(text).not.toContain('node packages/octocode-awareness/out/octocode-awareness.js');
    expect(text).toMatch(/default durable Awareness state is[\s\S]*\$OCTOCODE_HOME\/awareness\/awareness\.sqlite3/i);
    expect(text).toMatch(/repository scope uses[\s\S]*\.octocode\/awareness\.sqlite3/);
    expect(text).toContain('$OCTOCODE_HOME/awareness/awareness.sqlite3');
    expect(text).toMatch(/never uses the Agent control or runtime databases/i);
    expect(text).not.toMatch(/\.octocode\/(?:octocode|agent)\.sqlite3/);
    expect(text).not.toMatch(/octocode-awareness-lite|\/lite\b|Awareness Lite/);
    expect(text).toMatch(/## Operational rules[\s\S]*meaningful shared-state signal/i);
    expect(text).toContain('observed receipts');
    expect(text).toMatch(/CLI[^.]*operational state[^.]*observed records/i);
    expect(text).toContain('npx @octocodeai/octocode-awareness attend');
    expect(text).toContain('scripts/install.mjs');
    expect(text).toMatch(/overlap is advisory/i);
    expect(text).toContain('config schema');
    expect(text).toContain('flow-matrix.md');
    expect(text).toContain('Load detail only when needed');
    expect(text).toContain('yarn workspace @octocodeai/octocode-awareness build');
    expect(awarenessSkillFile('references/hooks.md')).toContain('Smoke:');
    expect(existsSync(resolve(PACKAGE_ROOT, 'skills/octocode-awareness/SKILL.md'))).toBe(true);
    expect(existsSync(resolve(PACKAGE_ROOT, 'skills/octocode-skills'))).toBe(false);
  });

  it('teaches the complete agent lifecycle without assigning judgment to hooks', () => {
    const text = skill('octocode-awareness');
    for (const step of ['meaningful shared-state signal', 'Declare bounded work', 'Run the declared check', 'leave a handoff']) {
      expect(text).toContain(step);
    }
    expect(text).toMatch(/CLI[^.]*advice[^.]*observed records/i);
    expect(text).toMatch(/Hooks[^.]*guard edits[^.]*changed pointers/i);
    expect(text).toMatch(/host owns context[^.]*tools[^.]*budgets[^.]*workers/i);
    expect(text).toMatch(/Unknown sensors stay unknown[^.]*never (?:invent|infer)/i);
    expect(text).toMatch(/Advice neither authorizes action nor proves success/i);
    expect(text).toMatch(/unchanged[^.]*continue authorized work[^.]*not recursive attending/i);
    expect(text).toMatch(/exclusive lock only (?:for )?unsafe, non-mergeable state/i);
    expect(awarenessSkillFile('references/hooks.md')).toMatch(/do not choose tasks or replace\s+`attend`\/verify/i);
    expect(text).toMatch(/search hits, expiry, memories, and peer notes are leads, not proof/i);
  });

  it('shows a lean overview of every Awareness feature family', () => {
    const text = skill('octocode-awareness');
    expect(text).toContain('Load detail only when needed');
    for (const feature of [
      'plan', 'task', 'work', 'lock', 'verification', 'Messages', 'handoffs',
      'Memory', 'reflection', 'hooks', 'schema',
    ]) {
      expect(text.toLowerCase(), `missing lean feature route: ${feature}`).toContain(feature.toLowerCase());
    }
    expect(text).toMatch(/query|queries/i);
    expect(text).toMatch(/plans\/tasks/i);
    expect(awarenessSkillFile('references/hooks.md')).toMatch(/do not choose tasks or replace\s+`attend`\/verify/i);
    expect(text).toContain('config schema');
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
      actual: routesAwarenessSignal(entry.prompt),
    }))).toEqual(heldOut.map((entry) => ({
      prompt: entry.prompt,
      expected: entry.expect,
      actual: entry.expect,
    })));
    expect(heldOut.filter((entry) => !entry.expect).map((entry) => entry.prompt).join('\n')).toMatch(/only agent/i);
    expect(heldOut.filter((entry) => !entry.expect).map((entry) => entry.prompt).join('\n')).toMatch(/read-only security review/i);
    expect(heldOut.filter((entry) => entry.expect).map((entry) => entry.prompt).join('\n')).toMatch(/resume/i);
    expect(heldOut.filter((entry) => !entry.expect).map((entry) => entry.prompt).join('\n')).toMatch(/outside a repo/i);
  });

  it('routes each fresh-agent feature question to one direct owner', () => {
    const text = skill('octocode-awareness');
    const journeys = [
      ['When sharing work', 'flow-matrix.md'],
      ['When storage/ownership matters', 'architecture.md'],
      ['protocol', 'coordination-protocol.md'],
    ] as const;
    for (const [trigger, owner] of journeys) {
      expect(text).toContain(trigger);
      expect(text).toContain(`references/${owner}`);
    }
  });

  it('keeps the awareness skill self-contained after removing sibling skills', () => {
    expect(existsSync(resolve(PACKAGE_ROOT, 'skills/octocode-awareness/SKILL.md'))).toBe(true);
    expect(existsSync(resolve(PACKAGE_ROOT, 'skills/octocode-awareness/scripts/awareness.mjs'))).toBe(true);
    expect(existsSync(resolve(PACKAGE_ROOT, 'skills/octocode-skills'))).toBe(false);
  });

  it('keeps generated runtime scripts only in the primary skill', () => {
    expect(existsSync(resolve(PACKAGE_ROOT, 'skills/octocode-awareness/scripts/awareness.mjs'))).toBe(true);
  });

  it('keeps standalone guidance portable outside the monorepo', () => {
    const readme = awarenessSkillFile('README.md');
    const tooling = awarenessSkillFile('references/agent-cheatsheet.md');
    const octocode = awarenessSkillFile('references/octocode.md');
    const dataModel = awarenessSkillFile('references/data-model.md');
    const combined = [readme, tooling, octocode, dataModel].join('\n');

    expect(combined).not.toMatch(/<package>|<awareness-package>|default for this monorepo/);
    expect(combined).not.toContain('package migration truth: `docs/DB.md`');
    expect(readme).toContain('npx @octocodeai/octocode-awareness maintenance init --compact');
    expect(readme).not.toContain('npm root --global');
    expect(tooling).not.toContain('out/skills/octocode-skills');
    expect(octocode).toContain('references/agent-cheatsheet.md');
  });

  it('uses one portable CLI runner while documenting every supported host', () => {
    const referenceRoot = resolve(PACKAGE_ROOT, 'skills/octocode-awareness/references');
    const hooks = awarenessSkillFile('references/hooks.md');
    const instructional = [
      skill('octocode-awareness'),
      awarenessSkillFile('README.md'),
      awarenessSkillFile('agents/openai.yaml'),
      ...readdirSync(referenceRoot)
        .filter((name) => name.endsWith('.md'))
        .map((name) => readFileSync(resolve(referenceRoot, name), 'utf8')),
    ].join('\n');

    expect(hooks).toMatch(/\bPi\b/);
    expect(instructional).not.toMatch(/node\s+(?:packages\/octocode-awareness\/out\/octocode-awareness\.js|scripts\/awareness\.mjs)/);
    expect(instructional).not.toContain('npx -p @octocodeai/octocode-awareness octocode-awareness');
    expect(instructional).toContain('npx @octocodeai/octocode-awareness');
  });
});
