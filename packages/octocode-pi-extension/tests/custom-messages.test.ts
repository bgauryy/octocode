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
  AWARENESS_HANDOFF_TYPE,
  COMPACTION_CHECKPOINT_TYPE,
  buildCompactionCard,
  buildHandoffCard,
  emitAwarenessHandoff,
  emitCompactionCheckpoint,
  renderCompactionContextMarker,
  registerOctocodeMessageRenderers,
  type AwarenessHandoffDetails,
  type CompactionCheckpointDetails,
} from '../src/tools/custom-messages.js';
import {
  registerCompactionHooks,
  resetCompactionCheckpointDedupe,
} from '../src/tools/compaction-hooks.js';
import { visibleWidth } from '../src/tools/render-helpers.js';
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
  const handlers = new Map<string, Handler[]>();
  const pi = {
    sendMessage: (msg: SentMessage, ...extraArgs: unknown[]) => {
      sent.push({ msg, extraArgs });
    },
    registerMessageRenderer: (type: string, renderer: Renderer) => renderers.set(type, renderer),
    sendUserMessage: () => undefined,
    on: (event: string, handler: Handler) => {
      const arr = handlers.get(event) ?? [];
      arr.push(handler);
      handlers.set(event, arr);
    },
  } as unknown as PiInstance;
  const fire = (event: string, evt: unknown, ctx: unknown) =>
    Promise.all((handlers.get(event) ?? []).map((h) => h(evt, ctx)));
  return { pi, sent, renderers, fire };
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

const handoffDetails: AwarenessHandoffDetails = {
  label: 'session-7',
  from: 'main',
  to: 'successor',
  goal: 'finish the renderer feature',
  status: 'in-progress',
  notes: ['tests pass', 'wiring pending'],
  artifacts: ['docs/plan.md'],
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

test('buildHandoffCard collapsed: 1-2 lines with label and route', () => {
  const lines = buildHandoffCard(handoffDetails, false, theme, WIDTH);
  assert.ok(lines.length >= 1 && lines.length <= 2);
  assert.match(lines[0]!, /◆/);
  assert.match(lines[0]!, /Awareness handoff/);
  assert.match(lines[0]!, /session-7/);
  assert.match(lines[1]!, /main → successor/);
  assert.match(lines[1]!, /status: in-progress/);
  assert.ok(!lines.some((l) => l.includes('finish the renderer feature')), 'goal is expanded-only');
});

test('buildHandoffCard expanded: full box with goal, notes, and artifacts', () => {
  const lines = buildHandoffCard(handoffDetails, true, theme, WIDTH);
  assert.match(lines[0]!, /╭─/);
  assert.match(lines[lines.length - 1]!, /╰─/);
  const body = lines.join('\n');
  assert.match(body, /goal:.*finish the renderer feature/);
  assert.match(body, /- tests pass/);
  assert.match(body, /- wiring pending/);
  assert.match(body, /artifacts \(1\).*docs\/plan\.md/);
});

test('card builders truncate every line to the given width', () => {
  const narrow = 24;
  for (const lines of [
    buildCompactionCard(compactionDetails, true, undefined, narrow),
    buildHandoffCard(handoffDetails, true, undefined, narrow),
  ]) {
    for (const line of lines) {
      assert.ok(visibleWidth(line) <= narrow, `line exceeds width ${narrow}: ${JSON.stringify(line)}`);
    }
  }
});

// ─── Emitters ─────────────────────────────────────────────────────────────────

test('emitCompactionCheckpoint: bounded marker carries recovery pointers and inline summary snapshot', () => {
  const { pi, sent } = makePi();
  emitCompactionCheckpoint(pi, compactionDetails);
  assert.equal(sent.length, 1);
  const { msg, extraArgs } = sent[0]!;
  assert.equal(msg.customType, COMPACTION_CHECKPOINT_TYPE);
  assert.equal(msg.display, true);
  assert.equal(msg.details, compactionDetails);
  assert.match(msg.content, /^<octocode_compaction_context>/);
  // Summary is now inlined (≤3500 chars, covers Next Steps) so the agent can resume without a file read.
  assert.match(msg.content, /"summaryAvailable":true/);
  assert.match(msg.content, /"summary":"line one/);
  assert.match(msg.content, /"phase":"executing"/);
  assert.match(msg.content, /"activeStepIds":\["step-1"\]/);
  assert.match(msg.content, /<\/octocode_compaction_context>$/);
  assert.ok(!msg.content.includes('\n'), 'context marker stays one bounded line');
  assert.ok(!msg.content.includes('src/a.ts'), 'large file lists remain renderer-only');
  assert.equal(extraArgs.length, 0, 'no options argument → no triggerTurn');
});

test('renderCompactionContextMarker caps provider summary text at 3500 chars', () => {
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
  // Summary is bounded at 3500 chars (covers Next Steps at ~2094 chars in typical summaries).
  assert.match(marker, /^<octocode_compaction_context>/);
  assert.match(marker, /"summaryAvailable":true/);
  // 3500 x's must be in the marker; the full 10000 must not appear.
  assert.match(marker, /"summary":"x{3500}"/);
  assert.doesNotMatch(marker, /x{3501}/);
  // marker stays a single line (no newlines in JSON payload)
  assert.ok(!marker.includes('\n'));
});

test('emitAwarenessHandoff: one-line content, details payload, display true, no triggerTurn', () => {
  const { pi, sent } = makePi();
  emitAwarenessHandoff(pi, handoffDetails);
  assert.equal(sent.length, 1);
  const { msg, extraArgs } = sent[0]!;
  assert.equal(msg.customType, AWARENESS_HANDOFF_TYPE);
  assert.equal(msg.display, true);
  assert.equal(msg.details, handoffDetails);
  assert.match(msg.content, /Awareness handoff recorded: session-7/);
  assert.ok(!msg.content.includes('\n'));
  assert.ok(!msg.content.includes('finish the renderer feature'));
  assert.equal(extraArgs.length, 0);
});

test('emitters are safe when the host lacks sendMessage', () => {
  const bare = {} as unknown as PiInstance;
  assert.doesNotThrow(() => emitCompactionCheckpoint(bare, compactionDetails));
  assert.doesNotThrow(() => emitAwarenessHandoff(bare, handoffDetails));
});

// ─── Renderer registration ────────────────────────────────────────────────────

test('registerOctocodeMessageRenderers registers both custom types', () => {
  const { pi, renderers } = makePi();
  registerOctocodeMessageRenderers(pi);
  assert.deepEqual(
    [...renderers.keys()].sort(),
    [AWARENESS_HANDOFF_TYPE, COMPACTION_CHECKPOINT_TYPE].sort(),
  );
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

  const handoff = renderers.get(AWARENESS_HANDOFF_TYPE)!(
    { role: 'custom', customType: AWARENESS_HANDOFF_TYPE, content: 'x', details: handoffDetails },
    { expanded: true },
    theme,
  ) as { render(width: number): string[] };
  const expanded = handoff.render(WIDTH);
  assert.match(expanded.join('\n'), /finish the renderer feature/);
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
