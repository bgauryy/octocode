import { describe, expect, it } from 'vitest';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const SCRIPT = resolve(dirname(fileURLToPath(import.meta.url)), '../out/octocode-awareness.js');

function run(db: string, args: string[]) {
  const result = spawnSync(process.execPath, [SCRIPT, '--db', db, ...args], {
    encoding: 'utf8', timeout: 30_000,
  });
  let parsed: Record<string, unknown> | null = null;
  try { parsed = JSON.parse(result.stdout) as Record<string, unknown>; } catch { /* asserted by caller */ }
  return { status: result.status ?? 1, stdout: result.stdout, parsed };
}

function ok(db: string, args: string[]): Record<string, unknown> {
  const result = run(db, args);
  expect(result.status, result.stdout).toBe(0);
  expect(result.parsed?.['ok'], result.stdout).not.toBe(false);
  return result.parsed!;
}

describe('CLI token efficiency', () => {
  it('bounds compact list surfaces and preserves explicit deep retrieval', () => {
    const workspace = mkdtempSync(join(tmpdir(), 'oc-token-budget-'));
    const db = join(workspace, 'awareness.sqlite3');
    try {
      const created = ok(db, [
        'plan', 'create', '--name', 'Compact plan', '--objective', 'Bound every list',
        '--lead-agent-id', 'lead', '--workspace', workspace,
      ]);
      const planId = (created['plan'] as Record<string, unknown>)['plan_id'] as string;
      ok(db, ['plan', 'status', '--plan-id', planId, '--agent-id', 'lead', '--status', 'ACTIVE']);

      for (let index = 0; index < 8; index += 1) {
        const paths = Array.from({ length: 8 }, (_, pathIndex) => [
          '--path', `packages/feature-${index}/src/very-long-path-${pathIndex}.ts`,
        ]).flat();
        ok(db, [
          'task', 'create', '--plan-id', planId, '--title', `Task ${index}`,
          '--reasoning', `Reason ${index}`, '--acceptance', `Acceptance ${index}`,
          '--agent-id', 'lead', ...paths,
        ]);
        ok(db, [
          'signal', 'publish', '--agent-id', 'sender', '--to-agent', 'reader',
          '--kind', 'fyi', '--subject', `Signal ${index}`,
          '--body', `body-${index}-${'x'.repeat(240)}`, '--workspace', workspace,
          '--file', `src/signal-${index}.ts`,
        ]);
        ok(db, [
          'agent', 'register', '--agent-id', `agent-${index}`, '--agent-name', `Agent ${index}`,
          '--workspace', workspace, '--context', `context-${index}-${'c'.repeat(200)}`,
        ]);
        ok(db, [
          'refinement', 'set', '--agent-id', `agent-${index}`,
          '--reasoning', `reason-${index}-${'r'.repeat(260)}`,
          '--remember', `remember-${index}-${'m'.repeat(520)}`,
          '--workspace', workspace, '--file', `src/refinement-${index}.ts`,
        ]);
      }

      const tasks = run(db, ['task', 'list', '--workspace', workspace, '--compact']);
      expect(tasks.status).toBe(0);
      expect(tasks.parsed).toMatchObject({ count: 5, total_count: 8, omitted_count: 3 });
      const taskRows = tasks.parsed?.['tasks'] as Array<Record<string, unknown>>;
      expect(taskRows).toHaveLength(5);
      expect(taskRows.every((row) => (row['paths'] as unknown[]).length <= 3)).toBe(true);
      expect(taskRows[0]).toMatchObject({ path_count: 8, path_omitted_count: 5 });
      expect(Buffer.byteLength(tasks.stdout, 'utf8')).toBeLessThanOrEqual(4 * 1024);

      const signals = run(db, [
        'signal', 'list', '--agent-id', 'reader', '--workspace', workspace, '--compact',
      ]);
      expect(signals.status).toBe(0);
      expect(signals.parsed).toMatchObject({ count: 3, has_more: true, bodies: 'omitted' });
      const signalRows = signals.parsed?.['signals'] as Array<Record<string, unknown>>;
      expect(signalRows).toHaveLength(3);
      expect(signalRows.every((row) => !Object.hasOwn(row, 'body'))).toBe(true);
      expect(Buffer.byteLength(signals.stdout, 'utf8')).toBeLessThanOrEqual(3 * 1024);

      const agents = run(db, ['agent', 'list', '--workspace', workspace, '--compact']);
      expect(agents.status).toBe(0);
      expect(agents.parsed).toMatchObject({ count: 5, total_count: 8, omitted_count: 3 });
      expect(Buffer.byteLength(agents.stdout, 'utf8')).toBeLessThanOrEqual(2 * 1024);

      const refinements = run(db, [
        'refinement', 'get', '--workspace', workspace, '--state', 'open', '--compact',
      ]);
      expect(refinements.status).toBe(0);
      expect(refinements.parsed).toMatchObject({ count: 3, has_more: true });
      const refinementRows = refinements.parsed?.['refinements'] as Array<Record<string, unknown>>;
      expect(refinementRows[0]).toHaveProperty('reasoning_summary');
      expect(refinementRows[0]).not.toHaveProperty('reasoning');
      expect(Buffer.byteLength(refinements.stdout, 'utf8')).toBeLessThanOrEqual(4 * 1024);

      const workboard = run(db, ['query', 'workboard', '--workspace', workspace, '--compact']);
      expect(workboard.status).toBe(0);
      expect(workboard.parsed?.['is_partial']).toBe(true);
      expect(Buffer.byteLength(workboard.stdout, 'utf8')).toBeLessThanOrEqual(6 * 1024);

      const fullTasks = ok(db, ['task', 'list', '--workspace', workspace, '--limit', '8', '--full', '--compact']);
      expect(((fullTasks['tasks'] as Array<Record<string, unknown>>)[0]!['paths'] as unknown[])).toHaveLength(8);
      const fullSignals = ok(db, [
        'signal', 'list', '--agent-id', 'reader', '--workspace', workspace,
        '--limit', '8', '--include-bodies', '--compact',
      ]);
      expect(String((fullSignals['signals'] as Array<Record<string, unknown>>)[0]!['body'])).toContain('x'.repeat(200));
      const fullAgents = ok(db, ['agent', 'list', '--workspace', workspace, '--limit', '8']);
      expect(String((fullAgents['agents'] as Array<Record<string, unknown>>)[0]!['context'])).toContain('c'.repeat(100));
      const fullRefinements = ok(db, [
        'refinement', 'get', '--workspace', workspace, '--state', 'open',
        '--limit', '8', '--full', '--compact',
      ]);
      expect(String((fullRefinements['refinements'] as Array<Record<string, unknown>>)[0]!['remember'])).toContain('m'.repeat(400));
    } finally {
      rmSync(workspace, { recursive: true, force: true });
    }
  });

  it('routes an inbox to a bounded signal read', () => {
    const workspace = mkdtempSync(join(tmpdir(), 'oc-token-next-'));
    const db = join(workspace, 'awareness.sqlite3');
    try {
      ok(db, [
        'signal', 'publish', '--agent-id', 'sender', '--to-agent', 'reader',
        '--kind', 'question', '--subject', 'Need input', '--workspace', workspace,
      ]);
      const attend = ok(db, [
        'attend', '--workspace', workspace, '--agent-id', 'reader', '--compact',
      ]);
      expect(String(attend['next'])).toContain('signal list');
      expect(String(attend['next'])).toContain('--limit 3');
      expect(attend).not.toHaveProperty('artifact');
      expect(attend).not.toHaveProperty('repo');
      expect(attend).not.toHaveProperty('ref');
    } finally {
      rmSync(workspace, { recursive: true, force: true });
    }
  });

  it('caps repeated memory fields and omits lean markers when recall is empty', () => {
    const workspace = mkdtempSync(join(tmpdir(), 'oc-token-memory-'));
    const db = join(workspace, 'awareness.sqlite3');
    try {
      const repeated = Array.from({ length: 8 }, (_, index) => [`--tag`, `tag-${index}`, `--reference`, `doc:ref-${index}`]).flat();
      ok(db, [
        'memory', 'record', '--agent-id', 'writer', '--task-context', 'bounded memory fields',
        '--observation', 'Return only the fields needed for the next decision.', '--importance', '8',
        '--workspace', workspace, ...repeated,
      ]);
      const recall = run(db, ['memory', 'recall', '--query', 'bounded memory fields', '--workspace', workspace, '--compact']);
      expect(recall.status).toBe(0);
      const memory = (recall.parsed?.['memories'] as Array<Record<string, unknown>>)[0]!;
      expect(memory['tags']).toHaveLength(3);
      expect(memory['references']).toHaveLength(3);
      expect(memory).toMatchObject({ tag_count: 8, tag_omitted_count: 5, reference_count: 8, reference_omitted_count: 5 });
      expect(Buffer.byteLength(recall.stdout, 'utf8')).toBeLessThanOrEqual(2 * 1024);

      const empty = run(db, ['memory', 'recall', '--query', 'definitely-absent-term', '--workspace', workspace, '--compact']);
      expect(empty.status).toBe(0);
      expect(empty.parsed).toEqual({ count: 0, memories: [], ok: true });
      expect(Buffer.byteLength(empty.stdout, 'utf8')).toBeLessThanOrEqual(64);
    } finally {
      rmSync(workspace, { recursive: true, force: true });
    }
  });

  it('keeps discovery, exact contracts, hooks, and wiki receipts byte-lean', () => {
    const workspace = mkdtempSync(join(tmpdir(), 'oc-token-contract-'));
    const db = join(workspace, 'awareness.sqlite3');
    try {
      const commands = run(db, ['schema', 'commands', '--compact']);
      expect(commands.status).toBe(0);
      expect(commands.parsed?.['commands']).toMatchObject({
        core: { wiki: ['sync'], task: expect.arrayContaining(['create', 'claim']) },
        advanced: { lock: expect.arrayContaining(['acquire', 'release']) },
      });
      expect(Buffer.byteLength(commands.stdout, 'utf8')).toBeLessThanOrEqual(2 * 1024);

      const taskCreate = run(db, ['schema', 'command', 'task', 'create', '--compact']);
      expect(taskCreate.status).toBe(0);
      expect(taskCreate.parsed?.['required']).toEqual(expect.arrayContaining(['plan_id', 'path', 'agent_id']));
      expect((taskCreate.parsed?.['properties'] as Record<string, unknown>)).not.toHaveProperty('action');
      expect((taskCreate.parsed?.['properties'] as Record<string, unknown>)).not.toHaveProperty('run_id');
      expect(Buffer.byteLength(taskCreate.stdout, 'utf8')).toBeLessThanOrEqual(2 * 1024);

      const wiki = run(db, ['wiki', 'sync', '--workspace', workspace, '--out', join(workspace, '.octocode'), '--compact']);
      expect(wiki.status).toBe(0);
      expect(wiki.parsed).toMatchObject({ ok: true, written: expect.any(Number), warning_count: 0 });
      expect(wiki.parsed).not.toHaveProperty('files');
      expect(Buffer.byteLength(wiki.stdout, 'utf8')).toBeLessThanOrEqual(768);

      const hooks = run(db, ['hooks', 'install', '--host', 'claude', '--project-dir', workspace, '--dry-run', '--compact']);
      expect(hooks.status).toBe(0);
      expect(hooks.parsed).toMatchObject({ ok: true, action: 'dry-run', host: 'claude', hook_count: 9 });
      expect(hooks.parsed).not.toHaveProperty('resultingSettings');
      expect(Buffer.byteLength(hooks.stdout, 'utf8')).toBeLessThanOrEqual(512);

      expect(run(db, ['hooks', 'install', '--host', 'claude', '--project-dir', workspace, '--compact']).status).toBe(0);
      const hookHealth = run(db, ['hooks', 'check', '--host', 'claude', '--project-dir', workspace, '--strict', '--compact']);
      expect(hookHealth.status).toBe(0);
      expect(hookHealth.parsed).toMatchObject({
        ok: true,
        health: { config: 'ready', runtime: 'unverified' },
      });
      expect(hookHealth.parsed).not.toHaveProperty('next');
      expect(hookHealth.parsed).not.toHaveProperty('ready');
      expect(Buffer.byteLength(hookHealth.stdout, 'utf8')).toBeLessThanOrEqual(256);

      const plan = run(db, ['plan', 'create', '--name', 'Lean', '--objective', 'Bound receipts', '--lead-agent-id', 'lead', '--workspace', workspace, '--compact']);
      expect(plan.status).toBe(0);
      expect(plan.parsed).toMatchObject({ ok: true, plan_id: expect.stringMatching(/^plan_/), status: 'ACTIVE' });
      expect(plan.parsed).not.toHaveProperty('plan');
      expect(Buffer.byteLength(plan.stdout, 'utf8')).toBeLessThanOrEqual(512);

      const task = run(db, ['task', 'create', '--plan-id', String(plan.parsed?.['plan_id']), '--title', 'Lean task', '--reasoning', 'Keep the receipt bounded', '--acceptance', 'token test passes', '--path', 'src/a.ts', '--agent-id', 'lead', '--compact']);
      expect(task.status).toBe(0);
      const exactFile = run(db, ['query', 'files', '--workspace', workspace, '--file', 'src/a.ts', '--compact']);
      expect(exactFile.status).toBe(0);
      expect(exactFile.parsed).toMatchObject({ count: 1, rows: [{ file_path: join(workspace, 'src/a.ts') }] });
      expect(exactFile.parsed).not.toHaveProperty('total');
      expect(exactFile.parsed).not.toHaveProperty('omitted_count');
      expect(exactFile.parsed).not.toHaveProperty('is_partial');
      expect(exactFile.parsed?.['filters']).toEqual({ file: 'src/a.ts' });
      expect(Buffer.byteLength(exactFile.stdout, 'utf8')).toBeLessThanOrEqual(768);
      const missingFile = run(db, ['query', 'files', '--workspace', workspace, '--file', 'missing.ts', '--compact']);
      expect(missingFile.status).toBe(0);
      expect(missingFile.parsed).toMatchObject({ count: 0, rows: [] });
      expect(Buffer.byteLength(missingFile.stdout, 'utf8')).toBeLessThanOrEqual(256);

      ok(db, [
        'memory', 'record', '--agent-id', 'writer', '--task-context', 'compact query scope',
        '--observation', 'Do not repeat the top-level workspace on every row.', '--importance', '7',
        '--workspace', workspace,
      ]);
      const scoped = run(db, ['query', 'memories', '--workspace', workspace, '--query', 'compact query scope', '--compact']);
      expect(scoped.status).toBe(0);
      expect(scoped.parsed?.['filters']).toEqual({ query: 'compact query scope' });
      expect(scoped.parsed).not.toHaveProperty('total');
      expect(scoped.parsed).not.toHaveProperty('omitted_count');
      expect(scoped.parsed).not.toHaveProperty('is_partial');
      expect((scoped.parsed?.['rows'] as Array<Record<string, unknown>>)[0]).not.toHaveProperty('workspace_path');
      const claim = run(db, ['task', 'claim', '--task-id', String(task.parsed?.['task_id']), '--agent-id', 'worker', '--compact']);
      expect(claim.status).toBe(0);
      expect(claim.parsed).toMatchObject({ ok: true, task_id: task.parsed?.['task_id'], run_id: expect.stringMatching(/^run_/), status: 'ACTIVE' });
      expect(claim.parsed).not.toHaveProperty('task');
      expect(Buffer.byteLength(claim.stdout, 'utf8')).toBeLessThanOrEqual(512);
    } finally {
      rmSync(workspace, { recursive: true, force: true });
    }
  });
});
