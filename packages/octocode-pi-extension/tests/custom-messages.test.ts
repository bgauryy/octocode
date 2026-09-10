import { visibleWidth } from '../src/tui/width.js';
/**
 * Custom message renderer tests — branded cards for compaction checkpoints and
 * awareness handoffs.
 *
 * Pins: (1) card builders' collapsed vs expanded output, (2) emitters send a
 * bounded marked `content` (it enters the LLM context) plus renderer detail,
 * display true, and NO trigger-turn options argument, (3) renderer
 * registration covers both custom types and yields width-safe components,
 * (4) the compaction-hooks wiring emits at most one checkpoint card per
 * compaction event.
 */
import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { afterEach, beforeEach, test } from 'vitest';
import {
  COMPACTION_CHECKPOINT_TYPE,
  buildCompactionCard,
  buildExecutionEventCard,
  buildPeerEventCard,
  buildRecoveryCard,
  emitCompactionCheckpoint,
  renderCompactionContextMarker,
  registerOctocodeMessageRenderers,
  type CompactionCheckpointDetails,
} from '../src/tools/custom-messages.js';
import { EXECUTION_TRANSCRIPT_ENTRY_TYPE, type ExecutionEvent } from '../src/tools/execution-events.js';
import {
  registerCompactionHooks,
  resetCompactionCheckpointDedupe,
} from '../src/tools/compaction-hooks.js';

import { createSessionArtifactContext, readRehydrationLedger, readRehydrationSegmentContents } from '../src/tools/session-artifacts.js';
import { clearCurrentContextSources, registerCurrentContextSource } from '../src/tools/context-source-registry.js';
import { initializeSessionMemory, readSessionMemory, SESSION_MEMORY_RELATIVE_PATH, SESSION_MEMORY_TEMPLATE } from '../src/tools/session-memory.js';
import { SESSION_AUDIT_RELATIVE_PATH } from '../src/tools/session-audit.js';
import type { PiInstance, PiTheme } from '../src/types.js';

const theme = { fg: (c: string, t: string) => '<' + c + '>' + t + '</' + c + '>' } as unknown as PiTheme;

const WIDTH = 200;

let previousHome: string | undefined;
let testHome: string;

type SentMessage = { customType: string; content: string; display?: boolean; details?: unknown };
type Renderer = (message: unknown, options: { expanded?: boolean }, theme: PiTheme) => unknown;
type Handler = (event: unknown, ctx: unknown) => unknown | Promise<unknown>;

function makePi() {
  const sent: { msg: SentMessage; extraArgs: unknown[] }[] = [];
  const renderers = new Map<string, Renderer>();
  const entryRenderers = new Map<string, Renderer>();
  const handlers = new Map<string, Handler[]>();
  const pi = {
    sendMessage: (msg: SentMessage, ...extraArgs: unknown[]) => {
      sent.push({ msg, extraArgs });
    },
    registerMessageRenderer: (type: string, renderer: Renderer) => renderers.set(type, renderer),
    registerEntryRenderer: (type: string, renderer: Renderer) => entryRenderers.set(type, renderer),
    sendUserMessage: () => undefined,
    on: (event: string, handler: Handler) => {
      const arr = handlers.get(event) ?? [];
      arr.push(handler);
      handlers.set(event, arr);
    },
  } as unknown as PiInstance;
  const fire = (event: string, evt: unknown, ctx: unknown) =>
    Promise.all((handlers.get(event) ?? []).map((h) => h(evt, ctx)));
  return { pi, sent, renderers, entryRenderers, fire };
}

function executionEvent(
  type: ExecutionEvent['type'],
  payload: unknown,
): ExecutionEvent {
  return {
    version: 1,
    id: `event-${type}`,
    sessionId: 'session-1',
    runId: 'main',
    sequence: 1,
    timestamp: 1_000,
    visibility: 'transcript',
    type,
    payload,
  } as ExecutionEvent;
}

const compactionDetails: CompactionCheckpointDetails = {
  label: 'entry-42',
  reason: 'threshold',
  tokensBefore: 45000,
  estimatedTokensAfter: 18000,
  reclaimedTokens: 27000,
  reclaimedPercent: 60,
  fromExtension: true,
  readFiles: ['src/a.ts', 'src/b.ts'],
  modifiedFiles: ['src/c.ts'],
  artifactPath: '/tmp/octocode/compaction/entry-42.md',
  summary: 'line one\nline two',
  continuation: {
    version: 1,
    plan: {
      review: {
        phase: 'executing', branchSnapshotId: 'branch-42', generation: 3,
        decisions: [], blockingQuestions: [], comments: [],
      },
      coordination: { mode: 'auto', sourcePlanKey: 'source-42', coordinationWorkspace: '/tmp/workspace' },
      steps: [{ id: 'step-1', text: 'verify compaction flow', status: 'doing' }],
    },
  },
};

beforeEach(() => {
  previousHome = process.env['OCTOCODE_HOME'];
  testHome = fs.mkdtempSync(path.join(os.tmpdir(), 'octocode-compaction-test-home-'));
  process.env['OCTOCODE_HOME'] = testHome;
  resetCompactionCheckpointDedupe();
});

afterEach(() => {
  clearCurrentContextSources();
  if (previousHome === undefined) delete process.env['OCTOCODE_HOME'];
  else process.env['OCTOCODE_HOME'] = previousHome;
  fs.rmSync(testHome, { recursive: true, force: true });
});

// ─── Card builders ────────────────────────────────────────────────────────────

test('buildCompactionCard collapsed: 1-2 branded lines with label and stats', () => {
  const lines = buildCompactionCard(compactionDetails, false, theme, WIDTH);
  assert.ok(lines.length >= 1 && lines.length <= 2, `collapsed card must be 1-2 lines, got ${lines.length}`);
  assert.match(lines[0]!, /◆/);
  assert.match(lines[0]!, /Compaction checkpoint/);
  assert.match(lines[0]!, /entry-42/);
  assert.match(lines[0]!, /<accent>/, 'brand accents via theme.fg');
  assert.match(lines[1]!, /reason: threshold/);
  assert.match(lines[1]!, /tokens before: 45000/);
  assert.match(lines[1]!, /after: ~18000/);
  assert.match(lines[1]!, /reclaimed: ~27000 \(60%\)/);
  // Collapsed must not show the expanded box frame or file lists.
  assert.ok(!lines.some((l) => l.includes('╭─') || l.includes('╰─')));
  assert.ok(!lines.some((l) => l.includes('src/a.ts')));
});

test('buildCompactionCard expanded: full box with files, source, and summary excerpt', () => {
  const lines = buildCompactionCard(compactionDetails, true, theme, WIDTH);
  assert.match(lines[0]!, /╭─/);
  assert.match(lines[0]!, /Compaction checkpoint/);
  assert.match(lines[lines.length - 1]!, /╰─/);
  const body = lines.join('\n');
  assert.match(body, /source: octocode/);
  assert.match(body, /read files \(2\).*src\/a\.ts, src\/b\.ts/);
  assert.match(body, /modified files \(1\).*src\/c\.ts/);
  assert.match(body, /entry-42\.md/);
  assert.match(body, /line one/);
  assert.match(body, /line two/);
});

test('buildCompactionCard omits empty sections and falls back to a label', () => {
  const lines = buildCompactionCard({ label: '' }, true, theme, WIDTH);
  const body = lines.join('\n');
  assert.match(body, /checkpoint/i, 'empty label falls back');
  assert.doesNotMatch(body, /read files/);
  assert.doesNotMatch(body, /modified files/);
  assert.doesNotMatch(body, /reason:/);
});

test('card builders truncate every line to the given width', () => {
  const narrow = 24;
  for (const lines of [
    buildCompactionCard(compactionDetails, true, undefined, narrow),
    buildExecutionEventCard(
      executionEvent('agent.message', {
        id: 'agent-1', name: 'atlas', direction: 'from-agent', action: 'reply',
        preview: '界'.repeat(100), timestamp: 1_000,
      }),
      true,
      undefined,
      narrow,
    ),
  ]) {
    for (const line of lines) {
      assert.ok(visibleWidth(line) <= narrow, `line exceeds width ${narrow}: ${JSON.stringify(line)}`);
    }
  }
});

// ─── Emitters ─────────────────────────────────────────────────────────────────

test('emitCompactionCheckpoint: bounded marker carries recovery pointers without duplicating the native summary', () => {
  const { pi, sent } = makePi();
  emitCompactionCheckpoint(pi, compactionDetails);
  assert.equal(sent.length, 1);
  const { msg, extraArgs } = sent[0]!;
  assert.equal(msg.customType, COMPACTION_CHECKPOINT_TYPE);
  assert.equal(msg.display, true);
  assert.equal(msg.details, compactionDetails);
  assert.match(msg.content, /^<octocode_compaction_context>/);
  assert.match(msg.content, /"summaryAvailable":true/);
  assert.doesNotMatch(msg.content, /"summary":/);
  assert.doesNotMatch(msg.content, /line one/);
  assert.match(msg.content, /"phase":"executing"/);
  assert.match(msg.content, /"activeStepIds":\["step-1"\]/);
  assert.match(msg.content, /<\/octocode_compaction_context>$/);
  assert.ok(!msg.content.includes('\n'), 'context marker stays one bounded line');
  assert.ok(!msg.content.includes('src/a.ts'), 'large file lists remain renderer-only');
  assert.equal(extraArgs.length, 0, 'no options argument → no triggerTurn');
});

test('renderCompactionContextMarker keeps large summary bodies out of model context', () => {
  const marker = renderCompactionContextMarker({
    label: 'bounded',
    summary: 'x'.repeat(10_000),
    continuation: {
      version: 1,
      plan: {
        review: { phase: 'executing', branchSnapshotId: 'b', generation: 1, decisions: [], blockingQuestions: [], comments: [] },
        coordination: { mode: 'auto', sourcePlanKey: 's', coordinationWorkspace: '/workspace' },
        steps: Array.from({ length: 200 }, (_, index) => ({ id: `step-${index}`, text: 'y'.repeat(500), status: 'todo' as const, paths: [`docs/${'z'.repeat(500)}.md`] })),
      },
    },
  });
  assert.match(marker, /^<octocode_compaction_context>/);
  assert.match(marker, /"summaryAvailable":true/);
  assert.doesNotMatch(marker, /"summary":/);
  assert.doesNotMatch(marker, /x{20}/);
  // marker stays a single line (no newlines in JSON payload)
  assert.ok(!marker.includes('\n'));
});

test('emitters are safe when the host lacks sendMessage', () => {
  const bare = {} as unknown as PiInstance;
  assert.doesNotThrow(() => emitCompactionCheckpoint(bare, compactionDetails));
});

// ─── Renderer registration ────────────────────────────────────────────────────

test('registerOctocodeMessageRenderers registers lifecycle and peer types', () => {
  const { pi, renderers, entryRenderers } = makePi();
  registerOctocodeMessageRenderers(pi);
  assert.deepEqual(
    [...renderers.keys()].sort(),
    [COMPACTION_CHECKPOINT_TYPE, 'octocode-peer-event'].sort(),
  );
  assert.deepEqual([...entryRenderers.keys()], [EXECUTION_TRANSCRIPT_ENTRY_TYPE]);
});

test('execution cards render plan progress, agent messages, and terminal transitions', () => {
  const plan = buildExecutionEventCard(
    executionEvent('plan.updated', {
      id: 'plan-1', phase: 'executing', tasks: [
        { id: 'one', title: 'Research', status: 'done' },
        { id: 'two', title: 'Implement footer', status: 'doing' },
        { id: 'three', title: 'Validate Pi', status: 'todo' },
      ],
    }),
    true,
    undefined,
    100,
  ).join('\n');
  assert.match(plan, /Plan.*executing/);
  assert.match(plan, /1 done.*1 active.*1 ready/);
  assert.match(plan, /Implement footer/);

  const message = buildExecutionEventCard(
    executionEvent('agent.message', {
      id: 'agent-1', name: 'atlas', direction: 'from-agent', action: 'reply',
      preview: 'Focused tests now pass', timestamp: 1_000, planStep: 'Implement UI',
    }),
    false,
    undefined,
    100,
  ).join('\n');
  assert.match(message, /Agent message.*atlas.*reply/);
  assert.match(message, /Focused tests now pass/);

  const transition = buildExecutionEventCard(
    executionEvent('agent.transition', {
      id: 'agent-1', name: 'atlas', from: 'running', to: 'done',
      summary: 'Focused tests now pass', updatedAt: 2_000,
    }),
    false,
    undefined,
    100,
  ).join('\n');
  assert.match(transition, /Agent.*atlas.*done/);
  assert.match(transition, /Focused tests now pass/);
});

test('registered execution entry renderer reads durable entry data', () => {
  const { pi, entryRenderers } = makePi();
  registerOctocodeMessageRenderers(pi);
  const component = entryRenderers.get(EXECUTION_TRANSCRIPT_ENTRY_TYPE)!(
    {
      data: executionEvent('agent.message', {
        id: 'agent-1', name: 'atlas', direction: 'to-agent', action: 'steer',
        preview: 'Run the smoke test', timestamp: 1_000,
      }),
    },
    { expanded: false },
    theme,
  ) as { render(width: number): string[] };
  assert.match(component.render(100).join('\n'), /Run the smoke test/);
});

test('peer cards show attribution and bounded previews, with complete expandable content', () => {
  const message = {
    content: '[peer:worker-1; class:blocking; authority:data]\nNeed the test result\n' + '界'.repeat(180),
    details: { messageClass: 'blocking', eventId: 'event-1' },
  };
  const collapsed = buildPeerEventCard(message, false, undefined, 50);
  assert.match(collapsed.join('\n'), /Awareness.*blocking/);
  assert.match(collapsed.join('\n'), /worker-1/);
  assert.match(collapsed.join('\n'), /Need the test result/);
  assert.ok(collapsed.length <= 3);
  const expanded = buildPeerEventCard(message, true, undefined, 50);
  assert.match(expanded.join('\n'), /Peer data/);
  assert.ok(expanded.every((line) => visibleWidth(line) <= 50));
  assert.equal(expanded.join('').match(/界/g)?.length, 180, 'expansion wraps rather than drops long peer content');
});

test('recovery cards distinguish validated, restored, and omitted context without claiming full recovery', () => {
  const receipt = { outcome: 'restored', reason: 'compaction', validated: ['plan', 'skill'], restored: ['plan'], stale: ['old-tool'], corrupt: [], overBudget: ['large-memory'], pendingInteractionIds: ['decision-1'] };
  for (const width of [36, 52, 80, 120, 160]) {
    const lines = buildRecoveryCard(receipt, false, undefined, width);
    assert.ok(lines.every((line) => visibleWidth(line) <= width));
    assert.match(lines.join('\n'), /1 pending decision/);
  }
  const full = buildRecoveryCard(receipt, true, undefined, 120).join('\n');
  assert.match(full, /1 restored/);
  assert.match(full, /2 validated/);
  assert.match(full, /large-memory/);
  assert.match(full, /1 pending decision/);
  assert.match(full, /partial/);
  assert.deepEqual(buildRecoveryCard({ outcome: 'pending-validation' }, false, undefined, 80), []);
  assert.deepEqual(buildRecoveryCard({ outcome: 'missing' }, false, undefined, 80), []);
});

test('registered renderer components render the card lines from message.details', () => {
  const { pi, renderers } = makePi();
  registerOctocodeMessageRenderers(pi);

  const checkpoint = renderers.get(COMPACTION_CHECKPOINT_TYPE)!(
    { role: 'custom', customType: COMPACTION_CHECKPOINT_TYPE, content: 'x', details: compactionDetails },
    { expanded: false },
    theme,
  ) as { render(width: number): string[] };
  const collapsed = checkpoint.render(WIDTH);
  assert.match(collapsed[0]!, /Compaction checkpoint/);
  assert.match(collapsed[0]!, /entry-42/);

  const expanded = renderers.get(COMPACTION_CHECKPOINT_TYPE)!(
    { role: 'custom', customType: COMPACTION_CHECKPOINT_TYPE, content: 'x', details: compactionDetails },
    { expanded: true },
    theme,
  ) as { render(width: number): string[] };
  assert.match(expanded.render(WIDTH).join('\n'), /╭─/);
});

test('registered renderer tolerates a message with no details', () => {
  const { pi, renderers } = makePi();
  registerOctocodeMessageRenderers(pi);
  const component = renderers.get(COMPACTION_CHECKPOINT_TYPE)!(
    { role: 'custom', customType: COMPACTION_CHECKPOINT_TYPE, content: 'x' },
    { expanded: true },
    theme,
  ) as { render(width: number): string[] };
  const lines = component.render(WIDTH);
  assert.ok(lines.length > 0);
  assert.match(lines.join('\n'), /checkpoint/i);
});

test('registerOctocodeMessageRenderers is a no-op on hosts without registerMessageRenderer', () => {
  const bare = {} as unknown as PiInstance;
  assert.doesNotThrow(() => registerOctocodeMessageRenderers(bare));
});

// ─── compaction-hooks wiring + dedupe ─────────────────────────────────────────

function checkpointCards(sent: { msg: SentMessage }[]): SentMessage[] {
  return sent.map((s) => s.msg).filter((m) => m.customType === COMPACTION_CHECKPOINT_TYPE);
}

test('session_compact completion emits exactly one checkpoint card per compaction event and writes markdown artifacts', async () => {
  const { pi, sent, fire } = makePi();
  registerCompactionHooks(pi, (() => undefined) as never);
  const event = {
    compactionEntry: {
      id: 'c-1',
      tokensBefore: 90000,
      estimatedTokensAfter: 30000,
      summary: 'sum',
      details: { readFiles: ['src/a.ts'], modifiedFiles: ['src/b.ts'] },
    },
    fromExtension: false,
    reason: 'threshold',
    willRetry: false,
  };
  const sessionManager = { getSessionId: () => 'compaction-test' };
  const ctx = { hasUI: false, cwd: testHome, sessionManager };
  const artifacts = createSessionArtifactContext({ cwd: testHome, sessionManager });
  initializeSessionMemory(artifacts);
  artifacts.writeText(
    SESSION_MEMORY_RELATIVE_PATH,
    SESSION_MEMORY_TEMPLATE.replace('## Decisions\n', '## Decisions\n- Keep the live memory owner authoritative.\n'),
  );
  registerCurrentContextSource(ctx, {
    version: 1,
    id: 'session-memory',
    kind: 'memory-lead',
    origin: 'session-memory',
    authority: 'external-data',
    scope: 'session',
    visibility: 'inspectable',
    rehydrate: 'always',
    tokenBudget: 1_000,
    readCurrent: () => readSessionMemory(artifacts),
  });
  await fire('session_compact', event, ctx);
  await fire('session_compact', event, ctx);
  const cards = checkpointCards(sent);
  assert.equal(cards.length, 1, 'two hook firings for the same compaction must emit one card');
  assert.match(cards[0]!.content, /^<octocode_compaction_context>/);
  assert.match(cards[0]!.content, /"checkpoint":"c-1"/);
  assert.ok(!cards[0]!.content.includes('\n'));
  const details = cards[0]!.details as CompactionCheckpointDetails;
  assert.equal(details.reason, 'threshold');
  assert.equal(details.tokensBefore, 90000);
  assert.equal(details.estimatedTokensAfter, 30000);
  assert.equal(details.reclaimedTokens, 60000);
  assert.equal(details.reclaimedPercent, 67);
  assert.equal(details.summary, 'sum');
  assert.equal(details.fromExtension, false);
  assert.deepEqual(details.readFiles, ['src/a.ts']);
  assert.deepEqual(details.modifiedFiles, ['src/b.ts']);
  assert.ok(details.artifactPath?.startsWith(`${path.dirname(artifacts.resolve('compaction/latest.md'))}${path.sep}`));
  assert.ok(details.artifactPath?.endsWith('-c-1.md'));
  assert.equal(details.latestArtifactPath, artifacts.resolve('compaction/latest.md'));

  const markdown = fs.readFileSync(details.artifactPath!, 'utf8');
  assert.match(markdown, /# Compaction checkpoint c-1/);
  assert.match(markdown, /Tokens before: 90000/);
  assert.match(markdown, /Estimated tokens after: 30000/);
  assert.match(markdown, /Estimated reclaimed: 60000 \(67%\)/);
  assert.match(markdown, /## Summary\n\nsum/);
  assert.match(markdown, /- src\/a\.ts/);
  assert.match(markdown, /- src\/b\.ts/);
  assert.equal(fs.readFileSync(details.latestArtifactPath!, 'utf8'), markdown);

  const ledger = readRehydrationLedger(artifacts)!;
  assert.ok(ledger.segments.some((segment) => segment.id === 'session-memory'));
  assert.match(readRehydrationSegmentContents(artifacts, ledger)['session-memory'] ?? '', /live memory owner authoritative/);
  const audit = fs.readFileSync(artifacts.resolve(SESSION_AUDIT_RELATIVE_PATH), 'utf8');
  assert.match(audit, /\| compaction\.completed \|/);
  assert.match(audit, /c-1/);
});
test('a distinct compaction event emits its own card', async () => {
  const { pi, sent, fire } = makePi();
  registerCompactionHooks(pi, (() => undefined) as never);
  await fire('session_compact', { compactionEntry: { id: 'c-1' }, fromExtension: false, reason: 'threshold', willRetry: false }, { hasUI: false });
  await fire('session_compact', { compactionEntry: { id: 'c-2' }, fromExtension: false, reason: 'manual', willRetry: false }, { hasUI: false });
  assert.equal(checkpointCards(sent).length, 2);
});

test('overflow checkpoint is emitted before Pi retries the interrupted turn', async () => {
  const { pi, sent, fire } = makePi();
  registerCompactionHooks(pi, (() => undefined) as never);
  await fire('session_compact', { compactionEntry: { id: 'c-1' }, fromExtension: false, reason: 'overflow', willRetry: true }, { hasUI: false });
  assert.equal(checkpointCards(sent).length, 1);
});

test('checkpoint card label falls back to the reason when the entry has no id', async () => {
  const { pi, sent, fire } = makePi();
  registerCompactionHooks(pi, (() => undefined) as never);
  await fire('session_compact', { compactionEntry: {}, fromExtension: false, reason: 'manual', willRetry: false }, { hasUI: false });
  const cards = checkpointCards(sent);
  assert.equal(cards.length, 1);
  assert.match(cards[0]!.content, /"checkpoint":"manual compaction"/);
});
