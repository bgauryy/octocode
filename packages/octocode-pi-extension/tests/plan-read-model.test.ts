import { describe, expect, it } from 'vitest';
import { buildPlanReadModel, renderPlanContext, renderPlanReadModel } from '../src/tools/plan-read-model.js';
import { buildPlanMarkdownFromModel, buildPlanPageHtmlFromModel } from '../src/tools/plan-html.js';
import { renderFooterView } from '../src/tui/footer-view.js';
import { buildPlanFooterSegments } from '../src/extension-ui.js';

describe('plan presentation read model', () => {
  function executionModel(phase: 'executing' | 'complete' | 'failed' = 'executing') {
    return buildPlanReadModel({
      steps: [
        { id: 'current', text: 'Implement core', status: 'doing', awarenessTaskId: 'core' },
        { id: 'blocked', text: 'Integrate core', status: 'todo', dependsOnStepIds: ['current'] },
        { id: 'blocked2', text: 'Verify integration', status: 'todo', dependsOnStepIds: ['blocked'] },
        { id: 'ready', text: 'Document API', status: 'todo' },
      ],
      review: { phase, branchSnapshotId: 'flow', generation: 1, decisions: [], blockingQuestions: [], comments: [] },
      coordination: { mode: 'local', sourcePlanKey: 'flow', coordinationWorkspace: '/repo' },
    });
  }

  it('prioritizes the running task in the persistent footer', () => {
    const lines = renderFooterView({ rows: [buildPlanFooterSegments(executionModel())] }, { width: 80 });
    expect(lines.join('\n')).toContain('Implement core');
    expect(lines.join('\n')).not.toContain('Integrate core');
  });

  it('shows runnable work in the footer when other lanes are blocked', () => {
    const model = executionModel();
    model.tasks[0]!.status = 'blocked';
    model.summary.running = 0;
    model.summary.blocked += 1;
    expect(buildPlanFooterSegments(model)[1]?.text).toContain('Document API');
  });

  it('recomputes dependent readiness from the effective shared task state', () => {
    const model = buildPlanReadModel({
      steps: [
        { id: 'parent', text: 'Parent', status: 'doing', awarenessTaskId: 'p' },
        { id: 'child', text: 'Child', status: 'todo', dependsOnStepIds: ['parent'], awarenessTaskId: 'c' },
      ],
      review: { phase: 'executing', branchSnapshotId: 'shared', generation: 1, decisions: [], blockingQuestions: [], comments: [] },
      coordination: { mode: 'required', sourcePlanKey: 'shared', coordinationWorkspace: '/repo' },
      sharedTaskStatuses: { p: 'DONE', c: 'OPEN' },
    });
    expect(model.tasks[1]?.status).toBe('todo');
    const blocked = buildPlanReadModel({
      steps: [
        { id: 'parent', text: 'Parent', status: 'done', awarenessTaskId: 'p' },
        { id: 'child', text: 'Child', status: 'todo', dependsOnStepIds: ['parent'], awarenessTaskId: 'c' },
      ],
      review: { phase: 'executing', branchSnapshotId: 'shared', generation: 1, decisions: [], blockingQuestions: [], comments: [] },
      coordination: { mode: 'required', sourcePlanKey: 'shared', coordinationWorkspace: '/repo' },
      sharedTaskStatuses: { p: 'FAILED', c: 'OPEN' },
    });
    expect(blocked.tasks[1]?.status).toBe('blocked');
    expect(renderPlanContext(blocked)).toContain('[!] 2. Child');
    expect(renderPlanContext(blocked)).not.toContain('Start the next runnable step');
  });

  it('keeps input gates free of contradictory start or parallel instructions', () => {
    const model = executionModel();
    model.pendingInteractionIds = ['question'];
    const prompt = renderPlanContext(model);
    expect(prompt).toContain('input-needed: pending interactions question');
    expect(prompt).not.toContain('parallel-ready:');
    expect(prompt).not.toContain('Execute active steps');
  });

  it('reports completed and failed outcomes without claiming implementation never started', () => {
    for (const phase of ['complete', 'failed'] as const) {
      const model = executionModel(phase);
      const prompt = renderPlanContext(model);
      expect(prompt).toContain(`phase=${phase}`);
      expect(prompt).not.toContain('Implementation has not started');
      expect(prompt).not.toContain('awaiting Start');
    }
  });

  it('keeps terminal, browser, and RPC projections on one versioned source', () => {
    const model = buildPlanReadModel({
      steps: [
        { id: 's1', text: 'Inspect', status: 'done' },
        { id: 's2', text: 'Implement', activeForm: 'Implementing', status: 'doing', dependsOnStepIds: ['s1'] },
      ],
      review: { phase: 'executing', branchSnapshotId: 'b1', generation: 2, revision: 'rev-1', acceptedRevision: 'rev-1', decisions: [], blockingQuestions: [], comments: [] },
      coordination: { mode: 'required', sourcePlanKey: 'p1', coordinationWorkspace: '/repo', awarenessPlanId: 'shared-1', materializedRevision: 'rev-1' },
    });
    expect(model).toMatchObject({ version: 1, revision: 'rev-1', summary: { total: 2, done: 1, running: 1 } });
    const terminal = renderPlanReadModel(model, 'terminal') as string;
    const browser = renderPlanReadModel(model, 'browser') as string;
    const rpc = renderPlanReadModel(model, 'rpc');
    for (const output of [terminal, browser, JSON.stringify(rpc)]) {
      expect(output).toContain('Inspect');
      expect(output).toContain('Implement');
    }
    expect(browser).toContain('data-plan-read-model="1"');
  });

  it('keeps terminal, browser, Markdown, RPC, and prompt semantics aligned for complex states without renderer mutation', () => {
    const model = buildPlanReadModel({
      steps: [
        { id: 'research', text: 'Research API', status: 'done', paths: ['src/api.ts'], awarenessTaskId: 'task-a' },
        { id: 'build', text: 'Build API', activeForm: 'Building API', status: 'doing', dependsOnStepIds: ['research'], acceptance: 'API passes contract tests', checkCommand: 'yarn test' },
        { id: 'ship', text: 'Ship API', status: 'todo', dependsOnStepIds: ['build'] },
      ],
      review: {
        phase: 'executing', branchSnapshotId: 'snapshot-7', generation: 7,
        rfcPath: '/repo/.octocode/rfc/api/RFC.md', revision: 'rev-7', acceptedRevision: 'rev-7',
        acceptAuthorizationReceiptId: 'accept-7', startAuthorizationReceiptId: 'start-7',
        decisions: [{ q: 'Transport?', a: 'HTTP' }],
        blockingQuestions: [{ id: 'answered', prompt: 'Port?', answer: '443', blocking: true }],
        comments: [{ id: 'resolved', body: 'Add auth', blocking: true, resolved: true }],
      },
      coordination: { mode: 'required', sourcePlanKey: 'source-7', coordinationWorkspace: '/repo', awarenessPlanId: 'plan-7', materializedRevision: 'rev-7' },
      pendingInteractionIds: ['question-2', 'question-1', 'question-1'],
    });
    const before = JSON.stringify(model);
    const terminal = renderPlanReadModel(model, 'terminal') as string;
    const browser = buildPlanPageHtmlFromModel(model);
    const markdown = buildPlanMarkdownFromModel(model, { generatedAt: new Date('2026-08-26T00:00:00.000Z') });
    const prompt = renderPlanContext(model);
    const rpc = JSON.parse(JSON.stringify(model));

    expect(JSON.stringify(model)).toBe(before);
    expect(rpc).toEqual(model);
    expect(model).toMatchObject({
      version: 1, phase: 'executing', revision: 'rev-7', acceptedRevision: 'rev-7',
      authorization: { acceptReceiptId: 'accept-7', startReceiptId: 'start-7' },
      coordination: { mode: 'required', awarenessPlanId: 'plan-7', materializedRevision: 'rev-7' },
      pendingInteractionIds: ['question-1', 'question-2'],
      tasks: [
        { id: 'research', status: 'done' },
        { id: 'build', status: 'doing' },
        { id: 'ship', status: 'blocked' },
      ],
    });
    expect(terminal).toContain('Research API');
    expect(terminal).toContain('Plan 1/3 · executing');
    for (const output of [browser, markdown, prompt]) {
      expect(output).toContain('Research API');
      expect(output).toMatch(/Build(?:ing)? API/);
      expect(output).toContain('Ship API');
    }
    expect(terminal).toMatch(/Build(?:ing)? API/);
    expect(terminal).toContain('Ship API');
    expect(browser).toContain('data-plan-read-model="1"');
    expect(browser).toContain('data-revision="rev-7"');
    expect(browser).toContain('data-task-id="ship" class="blocked"');
    expect(markdown).toContain('Read model: v1');
    expect(markdown).toContain('Phase: executing');
    expect(markdown).toContain('Revision: rev-7');
    expect(prompt).toContain('state: phase=executing snapshot=snapshot-7 generation=7');
    expect(prompt).toContain('coordination: mode=required awareness-plan=plan-7 materialized=rev-7');
    expect(prompt).toContain('contract 2: accept=API passes contract tests | check=yarn test');
    expect(prompt).toContain('input-needed: pending interactions question-1, question-2');
    expect(prompt).not.toContain('do not continue plan execution');
    expect(renderPlanReadModel(model, 'terminal')).toContain('Input needed · question-1, question-2');
    expect(renderPlanReadModel(model, 'browser')).toContain('Input needed · question-1, question-2');
  });

  it('deterministically overlays every shared task status without changing branch-local identity', () => {
    const statuses = ['OPEN', 'CLAIMED', 'IN_PROGRESS', 'BLOCKED', 'VERIFY', 'DONE', 'FAILED', 'CANCELLED'] as const;
    const model = buildPlanReadModel({
      steps: statuses.map((status, index) => ({ id: `s${index}`, text: status, status: index % 2 ? 'done' : 'doing', awarenessTaskId: `task-${status}` })),
      review: { phase: 'executing', branchSnapshotId: 'branch-local', generation: 19, decisions: [], blockingQuestions: [], comments: [] },
      coordination: { mode: 'required', sourcePlanKey: 'source', coordinationWorkspace: '/repo', awarenessPlanId: 'shared' },
      sharedTaskStatuses: Object.fromEntries(statuses.map((status) => [`task-${status}`, status])),
    });

    expect(model.tasks.map((task) => task.status)).toEqual(['todo', 'doing', 'doing', 'blocked', 'doing', 'done', 'blocked', 'blocked']);
    expect(model.review).toMatchObject({ branchSnapshotId: 'branch-local', generation: 19 });
  });

  it('renders an unresolved input gate even when the plan has no tasks', () => {
    const model = buildPlanReadModel({
      steps: [],
      review: { phase: 'needs_answers', branchSnapshotId: 'empty', generation: 1, decisions: [], blockingQuestions: [], comments: [] },
      coordination: { mode: 'local', sourcePlanKey: 'source', coordinationWorkspace: '/repo' },
      pendingInteractionIds: ['pending-1'],
    });

    expect(renderPlanContext(model)).toContain('input-needed: pending interactions pending-1');
    expect(renderPlanReadModel(model, 'terminal')).toContain('Input needed · pending-1');
    expect(renderPlanReadModel(model, 'browser')).toContain('Input needed · pending-1');
  });
});
