/**
 * TDD tests for agent-tools.ts reliability fixes:
 * H4: sendRpc EPIPE must mark the agent as 'failed' and notify waiters immediately.
 * M7: Spawning more than MAX_AGENT_RECORDS active agents must throw a hard error.
 *
 * These tests are RED against the un-patched source because:
 * - H4: sendRpc currently swallows EPIPE without changing status or notifying waiters.
 * - M7: MAX_AGENT_RECORDS is not exported and no hard-cap guard exists.
 */
import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { test, beforeEach, afterEach, vi } from 'vitest';
import { Type } from 'typebox';
import {
  spawnRpcAgent,
  setAgentProcessFactoryForTests,
  isSubagentProcess,
  MAX_AGENT_RECORDS,
  MAX_ACTIVE_AGENTS,
  DEFAULT_SPAWN_POLICY,
  evaluateStepBudget,
  formatElapsed,
  formatAgentLedgerDetails,
  getWorkerTranscript,
  listWorkerLedgerEntries,
  refreshAgentLedgerUi,
  isLedgerTickerActiveForTests,
  stopLedgerTickerForTests,
  extractDeltaSummary,
  listVisibleWorkerLedgerEntries,
  handleOctocodeAgentsCommand,
  waitForAgent,
  findLivePlanWorker,
} from '../src/tools/agent-tools.js';
import { registerUnifiedAgentTool } from '../src/tools/unified-agent-tool.js';
import type { ToolDefinition } from '../src/types.js';
import { makeMockAgentProcess } from './helpers/mock-process.js';
import { extensionWorkspaceRoot } from '../src/extension-paths.js';
import { activePlanScope, clearPlan, setPlan } from '../src/tools/active-plan.js';
import { registerPlanTool } from '../src/tools/plan-tool.js';

test('extractDeltaSummary prefers the latest structured worker line', () => {
  const out = '[STATUS] booting\nsome noise\n[ACTION] editing src/foo.ts\ntrailing chatter';
  assert.equal(extractDeltaSummary(out), '[ACTION] editing src/foo.ts');
});

test('extractDeltaSummary falls back to the last non-empty line', () => {
  assert.equal(extractDeltaSummary('line one\n\nline two\n   '), 'line two');
});

test('extractDeltaSummary returns undefined for blank output and truncates long lines', () => {
  assert.equal(extractDeltaSummary('   \n\n'), undefined);
  const long = `[FINDING] ${'x'.repeat(200)}`;
  const out = extractDeltaSummary(long)!;
  assert.ok(out.length <= 120, 'truncated to <=120 chars');
  assert.ok(out.endsWith('…'), 'ellipsized');
});

// ─── Reliability guardrails (research-backed) ─────────────────────────────────

test('fan-out warning threshold is small (~4) per structured-topology research', () => {
  assert.equal(DEFAULT_SPAWN_POLICY.warningActiveAgents, 3);
  assert.equal(DEFAULT_SPAWN_POLICY.maxActiveAgents, MAX_ACTIVE_AGENTS);
  assert.ok(DEFAULT_SPAWN_POLICY.maxStepsPerWorker > 0);
});

test('evaluateStepBudget flags a runaway worker at/over budget', () => {
  assert.equal(evaluateStepBudget(10, 60).exceeded, false);
  const hit = evaluateStepBudget(60, 60);
  assert.equal(hit.exceeded, true);
  assert.match(String(hit.warning), /step budget \(60\/60/);
  assert.equal(evaluateStepBudget(99, 60).exceeded, true);
});

test('evaluateStepBudget disabled for non-positive budget', () => {
  assert.equal(evaluateStepBudget(1000, 0).exceeded, false);
  assert.equal(evaluateStepBudget(1000, Number.NaN).exceeded, false);
});

// ─── Setup / teardown ─────────────────────────────────────────────────────────

beforeEach(() => {
  // Each test starts with a fresh empty agent registry
  setAgentProcessFactoryForTests(null);
});

afterEach(() => {
  // Restore the real factory so later tests are unaffected
  setAgentProcessFactoryForTests(null);
});

// ─── H4: EPIPE causes agent to be marked failed and waiters notified ──────────

test('H4: sendRpc EPIPE marks agent status as "failed"', () => {
  if (isSubagentProcess()) return; // skip inside RPC subprocess environments

  const mock = makeMockAgentProcess({ stdinThrows: true });
  setAgentProcessFactoryForTests(() => mock as never);

  const record = spawnRpcAgent({ task: 'test task', resourceMode: 'lean' });

  // sendRpc is called synchronously inside spawnRpcAgent; EPIPE is thrown and caught.
  // The fix must transition the record to 'failed', not leave it in 'running'.
  assert.equal(record.status, 'failed', `Expected status 'failed' after EPIPE; got '${record.status}'`);
  assert.ok(record.error, 'record.error must be populated after EPIPE');
});

test('agent spawn requires task and rejects the retired prompt alias', () => {
  const factory = vi.fn(() => makeMockAgentProcess() as never);
  setAgentProcessFactoryForTests(factory);
  assert.throws(() => spawnRpcAgent({ prompt: 'retired assignment spelling' } as never), /requires task/);
  assert.equal(factory.mock.calls.length, 0);
});

test('agent CLI uses canonical commands and rejects status and clear aliases', async () => {
  const notices: Array<{ message: string; level?: string }> = [];
  const ctx = { hasUI: false, ui: { notify: (message: string, level?: string) => notices.push({ message, level }) } } as never;
  setAgentProcessFactoryForTests(() => makeMockAgentProcess() as never);
  spawnRpcAgent({ task: 'visible worker' });
  await handleOctocodeAgentsCommand('hide', ctx);
  await handleOctocodeAgentsCommand('status', ctx);
  assert.equal(notices.at(-1)?.level, 'warning');
  assert.equal(listVisibleWorkerLedgerEntries().length, 0);
  await handleOctocodeAgentsCommand('list', ctx);
  assert.equal(listVisibleWorkerLedgerEntries().length, 1);
  await handleOctocodeAgentsCommand('clear', ctx);
  assert.equal(notices.at(-1)?.level, 'warning');
  assert.equal(listVisibleWorkerLedgerEntries().length, 1);
});

test('H4: waiters are resolved immediately when EPIPE transitions agent to failed', async () => {
  if (isSubagentProcess()) return;

  const mock = makeMockAgentProcess({ stdinThrows: true });
  setAgentProcessFactoryForTests(() => mock as never);

  const record = spawnRpcAgent({ task: 'test task', resourceMode: 'lean' });

  // Simulate what AgentMessage action:'wait' does: register a waiter if non-terminal,
  // resolve immediately if already terminal.
  const terminalStatuses = new Set(['idle', 'exited', 'failed', 'killed']);

  const raceResult = await Promise.race([
    new Promise<'resolved'>((resolve) => {
      if (terminalStatuses.has(record.status)) {
        resolve('resolved');
      } else {
        // Waiter would have been notified by notifyWaiters() inside the EPIPE catch.
        // Since the fix calls notifyWaiters synchronously, the Set should be empty by now.
        record.waiters.add(() => resolve('resolved'));
        // Nudge in case we missed the notification (shouldn't happen after fix)
        if (terminalStatuses.has(record.status)) resolve('resolved');
      }
    }),
    new Promise<'timed-out'>((res) => setTimeout(() => res('timed-out'), 500)),
  ]);

  assert.equal(raceResult, 'resolved', 'waiters must be notified; AgentMessage wait must not hang after EPIPE');
});

// ─── M7: Hard cap on active agents ────────────────────────────────────────────

test('M7: MAX_AGENT_RECORDS is exported and has the expected value', () => {
  assert.equal(typeof MAX_AGENT_RECORDS, 'number');
  assert.ok(MAX_AGENT_RECORDS > 0);
});

test('worker lastOutput preserves the complete agent result without truncation', () => {
  if (isSubagentProcess()) return;

  const mock = makeMockAgentProcess({ stdinThrows: false });
  setAgentProcessFactoryForTests(() => mock as never);
  const record = spawnRpcAgent({ task: 'large output', resourceMode: 'lean' });
  const hugeText = `${'x'.repeat(80_000)}\n[DONE] tail`;

  mock._emit(
    'stdout:data',
    Buffer.from(`${JSON.stringify({
      type: 'message_end',
      message: { role: 'assistant', content: [{ type: 'text', text: hugeText }] },
    })}\n`),
  );

  assert.equal(record.lastOutput, hugeText);
  assert.equal(record.normalizedResult?.status, 'done');
});

test('spawnRpcAgent assigns a globally durable workspace-scoped handback and injects it into the worker packet', () => {
  if (isSubagentProcess()) return;

  const tmpDir = fs.realpathSync(fs.mkdtempSync(path.join(os.tmpdir(), 'octocode-handback-test-')));
  try {
    const mock = makeMockAgentProcess({ stdinThrows: false });
    setAgentProcessFactoryForTests(() => mock as never);

    const record = spawnRpcAgent({ task: 'Goal: test\nContext: ctx\nScope: scope\nOwnership: read\nAcceptance: done\nReturn: result', cwd: tmpDir, resourceMode: 'lean' });
    const initialPrompt = String(mock.writes[0]?.['message'] ?? '');

    assert.equal(record.handbackPath, path.join(extensionWorkspaceRoot(tmpDir), 'workers', record.id, 'handback.md'));
    assert.equal(record.handbackPath.startsWith(tmpDir), false);
    assert.equal(fs.existsSync(path.dirname(record.handbackPath)), true, 'handback directory should be created before the worker starts');
    assert.match(initialPrompt, /durable handback file:/);
    assert.match(initialPrompt, new RegExp(record.handbackPath.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')));
    assert.match(initialPrompt, /parent agent id:/);
  } finally {
    fs.rmSync(tmpDir, { recursive: true, force: true });
  }
});

test('spawnRpcAgent keeps handbacks global when the workspace path is invalid', () => {
  if (isSubagentProcess()) return;

  const tmpDir = fs.realpathSync(fs.mkdtempSync(path.join(os.tmpdir(), 'octocode-handback-fallback-test-')));
  const invalidWorkspace = path.join(tmpDir, 'workspace-is-a-file');
  fs.writeFileSync(invalidWorkspace, 'not a directory', 'utf8');
  let fallbackAgentDir: string | undefined;
  try {
    const mock = makeMockAgentProcess({ stdinThrows: false });
    setAgentProcessFactoryForTests(() => mock as never);

    const record = spawnRpcAgent({ task: 'Goal: test fallback\nContext: ctx\nScope: scope\nOwnership: read\nAcceptance: done\nReturn: result', cwd: invalidWorkspace, resourceMode: 'lean' });
    fallbackAgentDir = path.dirname(record.handbackPath);
    const initialPrompt = String(mock.writes[0]?.['message'] ?? '');

    assert.equal(record.handbackPath, path.join(extensionWorkspaceRoot(invalidWorkspace), 'workers', record.id, 'handback.md'));
    assert.match(record.handbackPath, /handback\.md$/);
    assert.equal(fs.existsSync(fallbackAgentDir), true);
    assert.equal(record.policyWarnings.some(warning => /temporary fallback/.test(warning)), false);
    assert.ok(initialPrompt.includes(record.handbackPath));
  } finally {
    if (fallbackAgentDir) fs.rmSync(fallbackAgentDir, { recursive: true, force: true });
    fs.rmSync(tmpDir, { recursive: true, force: true });
  }
});

test('worker ledger and transcript surface handback artifact status and [ARTIFACT] output', () => {
  if (isSubagentProcess()) return;

  const tmpDir = fs.realpathSync(fs.mkdtempSync(path.join(os.tmpdir(), 'octocode-handback-test-')));
  try {
    const mock = makeMockAgentProcess({ stdinThrows: false });
    setAgentProcessFactoryForTests(() => mock as never);
    const record = spawnRpcAgent(
      { task: 'write handback', name: 'Artifact Steward', planStep: '3. Verify artifacts', cwd: tmpDir, resourceMode: 'lean' },
      { cwd: tmpDir, model: { id: 'gpt-5.6', provider: 'openai' } } as never,
    );

    fs.writeFileSync(record.handbackPath, '# Worker handback\n\nVerified result.\n', 'utf8');
    mock._emit(
      'stdout:data',
      Buffer.from(`${JSON.stringify({
        type: 'message_end',
        message: { role: 'assistant', content: [{ type: 'text', text: `[RESULT] stored handback\n[ARTIFACT] ${record.handbackPath}\n[DONE] complete` }] },
      })}\n`),
    );

    const entry = listWorkerLedgerEntries().find((item) => item.agentId === record.id);
    assert.equal(entry?.name, 'Artifact Steward');
    assert.equal(entry?.model, 'gpt-5.6');
    assert.equal(entry?.provider, 'openai');
    assert.equal(entry?.task, 'write handback');
    assert.equal(entry?.planStep, '3. Verify artifacts');
    assert.equal(entry?.artifact, record.handbackPath);
    assert.equal(entry?.handback?.exists, true);
    assert.equal(entry?.handback?.path, record.handbackPath);
    assert.ok((entry?.handback?.bytes ?? 0) > 0);
    const transcript = getWorkerTranscript(record.id) ?? '';
    assert.match(transcript, /artifact:/);
    assert.match(transcript, /handback file:/);
    assert.match(transcript, /bytes/);
  } finally {
    fs.rmSync(tmpDir, { recursive: true, force: true });
  }
});

test('agent_end user prompt echoes do not overwrite assistant worker output', () => {
  if (isSubagentProcess()) return;

  const mock = makeMockAgentProcess({ stdinThrows: false });
  setAgentProcessFactoryForTests(() => mock as never);
  const record = spawnRpcAgent({ task: 'review code', resourceMode: 'lean' });

  mock._emit(
    'stdout:data',
    Buffer.from(`${JSON.stringify({
      type: 'agent_end',
      messages: [
        { role: 'assistant', content: [{ type: 'text', text: '[RESULT] actual findings\n[DONE] reviewed' }] },
        { content: [{ type: 'text', text: 'Goal: echoed original task' }] },
        { role: 'user', content: [{ type: 'text', text: 'Return only your review findings now' }] },
      ],
    })}\n`),
  );

  assert.match(record.lastOutput, /actual findings/);
  assert.doesNotMatch(record.lastOutput, /Goal: echoed original task/);
  assert.doesNotMatch(record.lastOutput, /Return only your review findings/);
  assert.equal(record.normalizedResult?.status, 'done');
});

// ─── Worker launch mode: workers must not re-enter the SDK-embed launcher ─────
//
// getPiInvocation() re-executes process.argv[1], which for any octocode-agent
// process is bin/octocode-agent.mjs. When the parent runs in the default
// SDK-embed launch mode, that env is inherited by the child, so the worker also
// launches via launchWithSdk() — whose custom arg parser (sdk-launcher.ts) does
// not understand --tools/--exclude-tools/-e/--append-system-prompt/--skill/etc.
// Confirmed by live reproduction: a worker spawned with --tools web,MCPTool
// could still call `bash` because the allowlist was silently dropped. Forcing
// OCTOCODE_LAUNCHER_MODE=subprocess routes workers through octocode-agent's
// subprocess path, which forwards argv verbatim to the real Pi CLI — the only
// path that honors the full flag set buildPiArgs() produces.
test('spawnRpcAgent forces OCTOCODE_LAUNCHER_MODE=subprocess so worker --tools/--exclude-tools/-e flags are actually honored', () => {
  if (isSubagentProcess()) return;

  let capturedEnv: NodeJS.ProcessEnv | undefined;
  setAgentProcessFactoryForTests((_command, _args, options) => {
    capturedEnv = (options as { env?: NodeJS.ProcessEnv }).env;
    return makeMockAgentProcess() as never;
  });

  spawnRpcAgent({ task: 'research something', resourceMode: 'octocode', tools: ['web', 'MCPTool'] });

  assert.equal(
    capturedEnv?.['OCTOCODE_LAUNCHER_MODE'],
    'subprocess',
    'worker env must force subprocess launch mode so the curated tool allowlist is not silently dropped',
  );
});

// ─── L1: ledger elapsed time must freeze once an agent is terminal ───────────
//
// formatElapsed(startedAt) used to always compute Date.now() - startedAt, so a
// finished agent's "elapsed" kept growing forever in the footer/widget ledger
// (a 5s task from an hour ago would show "elapsed: 1h"). Terminal records must
// report a fixed end-to-end duration instead of drifting with wall-clock time.

test('L1: formatElapsed freezes at endedAt instead of drifting against Date.now()', () => {
  const started = 1_700_000_000_000;
  assert.equal(formatElapsed(started, started + 500), '500ms');
  assert.equal(formatElapsed(started, started + 5_000), '5s');
  assert.equal(formatElapsed(started, started + 65_000), '1m5s');
  // Without endedAt, falls back to Date.now() — still correct for live agents.
  const liveMs = Date.now() - started;
  assert.ok(liveMs > 0);
});

test('L1: ledger elapsed time is frozen for a terminal agent, not growing with wall-clock time', async () => {
  if (isSubagentProcess()) return;

  const mock = makeMockAgentProcess({ stdinThrows: false, exitImmediately: false });
  setAgentProcessFactoryForTests(() => mock as never);
  spawnRpcAgent({ task: 'finishes quickly', resourceMode: 'lean' });

  // Terminate the agent (status -> 'exited', updatedAt frozen at this moment).
  mock.exitCode = 0;
  mock._emit('close', 0, null);

  const snapshotA = formatAgentLedgerDetails();
  await new Promise((resolve) => setTimeout(resolve, 60));
  const snapshotB = formatAgentLedgerDetails();

  assert.equal(snapshotB, snapshotA, 'elapsed time for a terminal agent must not change after it finished');
});

test('M7: spawning beyond the root active-agent ceiling throws', function () {
  if (isSubagentProcess()) return;

  // Create a mock that keeps agents alive in 'running' state (non-droppable).
  const makePersistentMock = () => makeMockAgentProcess({ stdinThrows: false, exitImmediately: false });
  setAgentProcessFactoryForTests(() => makePersistentMock() as never);

  // Fill the active root-worker budget — all agents remain running.
  for (let i = 0; i < MAX_ACTIVE_AGENTS; i++) {
    spawnRpcAgent({ task: `slot ${i}`, resourceMode: 'lean' });
  }

  // The next spawn must throw a hard-cap error.
  assert.throws(
    () => spawnRpcAgent({ task: 'overflow', resourceMode: 'lean' }),
    /registry.*capacity|too many|at capacity/i,
    'Expected hard-cap error when root workers exceed MAX_ACTIVE_AGENTS',
  );
});

// ─── L2/L3: live ledger ticker (Wave 3 live-progress) ─────────────────────────

test('SEV-1: late worker process events ignore a stale session context', () => {
  if (isSubagentProcess()) return;

  let stale = false;
  const ui = { setStatus: () => {}, setWidget: () => {} };
  const ctx = {
    get hasUI() {
      if (stale) throw new Error('stale context sentinel');
      return true;
    },
    get ui() {
      if (stale) throw new Error('stale context sentinel');
      return ui;
    },
  } as never;
  const mock = makeMockAgentProcess({ stdinThrows: false, exitImmediately: false });
  setAgentProcessFactoryForTests(() => mock as never);
  spawnRpcAgent({ task: 'outlives its session', resourceMode: 'lean' }, ctx);

  stale = true;
  assert.doesNotThrow(() => mock._emit('stdout:data', Buffer.from(`${JSON.stringify({ type: 'agent_end', messages: [] })}\n`)));
  assert.doesNotThrow(() => mock._emit('stderr:data', Buffer.from('late stderr')));
  assert.doesNotThrow(() => mock._emit('error', new Error('late process error')));
  assert.doesNotThrow(() => mock._emit('close', 0, null));
  assert.doesNotThrow(() => refreshAgentLedgerUi(ctx), 'queued ledger ticks must also ignore the stale context');
});

test('SEV-1: AgentMessage wait cleanup ignores a context invalidated while waiting', async () => {
  if (isSubagentProcess()) return;

  let stale = false;
  const statusCalls: Array<[string, string | undefined]> = [];
  const ui = { setStatus: (key: string, value: string | undefined) => statusCalls.push([key, value]) };
  const ctx = {
    get hasUI() {
      if (stale) throw new Error('stale context sentinel');
      return true;
    },
    get ui() {
      if (stale) throw new Error('stale context sentinel');
      return ui;
    },
  } as never;
  const mock = makeMockAgentProcess({ stdinThrows: false, exitImmediately: false });
  setAgentProcessFactoryForTests(() => mock as never);
  const record = spawnRpcAgent({ task: 'wait across replacement', resourceMode: 'lean' }, ctx);
  const tools = new Map<string, ToolDefinition>();
  registerUnifiedAgentTool(
    { registerTool: (definition: ToolDefinition) => tools.set(definition.name, definition) } as never,
    Type,
    new Set<string>(),
    (pi, names, definition) => {
      names.add(definition.name);
      pi.registerTool?.(definition);
    },
  );

  const waiting = tools.get('agent')!.execute(
    'wait-stale-context',
    { queries: [{ reasoning: 'Collect worker output', type: 'wait', agentId: record.id, timeoutMs: 1_000 }] },
    undefined,
    undefined,
    ctx,
  );
  await vi.waitFor(() => assert.equal(statusCalls.length, 1));
  stale = true;
  mock._emit('stdout:data', Buffer.from(`${JSON.stringify({ type: 'agent_end', messages: [] })}\n`));

  await assert.doesNotReject(waiting);
  assert.deepEqual(statusCalls, [['agent-wait', `\u29D7 Waiting for \u201C${record.name}\u201D\u2026`]]);
});

test('agent wait cancellation releases waiters and probes without terminating the worker', async () => {
  if (isSubagentProcess()) return;
  vi.useFakeTimers();
  const mock = makeMockAgentProcess();
  setAgentProcessFactoryForTests(() => mock as never);
  const record = spawnRpcAgent({ task: 'keep working after cancelled wait', resourceMode: 'lean' });
  const tools = new Map<string, ToolDefinition>();
  registerUnifiedAgentTool({}, Type, new Set(), (_pi, _names, definition) => {
    tools.set(definition.name, definition);
  });
  const controller = new AbortController();
  let settled = false;
  const waiting = tools.get('agent')!.execute('cancel-wait', {
    queries: [{ reasoning: 'Collect worker result', type: 'wait', agentId: record.id, timeoutMs: 10, remove: true }],
  }, controller.signal).then(
    () => { settled = true; return 'completed'; },
    (error: Error) => { settled = true; return error.message; },
  );
  try {
    await vi.advanceTimersByTimeAsync(10);
    assert.equal(record.pendingProbes.size, 1, 'silence check has an active RPC probe');
    controller.abort();
    await vi.advanceTimersByTimeAsync(0);
    assert.equal(settled, true, 'cancellation must release the active public tool wait immediately');
    assert.match(await waiting, /abort/i);
    assert.equal(record.waiters.size, 0);
    assert.equal(record.activityListeners.size, 0);
    assert.equal(record.pendingProbes.size, 0);
    assert.equal(record.status, 'running', 'cancelling a wait must not cancel or remove its worker');
    assert.ok(listWorkerLedgerEntries().some((entry) => entry.agentId === record.id));
    assert.equal(mock.writes.some((write) => write.type === 'abort'), false);
  } finally {
    mock._emit('stdout:data', Buffer.from(`${JSON.stringify({ type: 'agent_end', messages: [] })}\n`));
    await waiting;
    vi.useRealTimers();
  }
});

test('pre-aborted agent wait rejects before retaining worker listeners', async () => {
  const mock = makeMockAgentProcess();
  setAgentProcessFactoryForTests(() => mock as never);
  const record = spawnRpcAgent({ task: 'reject cancelled observation', resourceMode: 'lean' });
  await assert.rejects(waitForAgent(record, { signal: AbortSignal.abort() }), { name: 'AbortError' });
  assert.equal(record.waiters.size, 0);
  assert.equal(record.activityListeners.size, 0);
  assert.equal(record.pendingProbes.size, 0);
});

test('plan worker ownership follows live processes within the same session and plan', () => {
  const mock = makeMockAgentProcess();
  setAgentProcessFactoryForTests(() => mock as never);
  const record = spawnRpcAgent({ task: 'assigned task', resourceMode: 'lean', planScope: 'session-one', planId: 'plan-one', planStep: 'step-one' });
  assert.equal(findLivePlanWorker('session-one', 'plan-one', 'step-one'), record.id);
  assert.equal(findLivePlanWorker('session-two', 'plan-one', 'step-one'), undefined);
  assert.equal(findLivePlanWorker('session-one', 'plan-two', 'step-one'), undefined);
  mock._emit('stdout:data', Buffer.from(`${JSON.stringify({ type: 'agent_end', messages: [] })}\n`));
  assert.equal(findLivePlanWorker('session-one', 'plan-one', 'step-one'), record.id, 'idle process still owns the assignment');
  mock.exitCode = 0;
  mock._emit('close', 0, null);
  assert.equal(findLivePlanWorker('session-one', 'plan-one', 'step-one'), undefined);
});

test('public plan assignment reaches the worker RPC prompt and prevents duplicate ownership', async () => {
  if (isSubagentProcess()) return;
  const workspace = fs.mkdtempSync(path.join(os.tmpdir(), 'pi-plan-delegation-'));
  const ctx = { cwd: workspace, sessionManager: { getSessionId: () => 'plan-worker-session' } } as never;
  const scope = activePlanScope(ctx);
  const mock = makeMockAgentProcess();
  setAgentProcessFactoryForTests(() => mock as never);
  const tools = new Map<string, ToolDefinition>();
  registerUnifiedAgentTool({}, Type, new Set(), (_pi, _names, definition) => tools.set(definition.name, definition));
  registerPlanTool({}, Type, new Set(), (_pi, _names, definition) => tools.set(definition.name, definition));
  try {
    setPlan(scope, [{ text: 'Implement endpoint', paths: ['src/endpoint.ts'], acceptance: 'Endpoint verified', checkCommand: 'yarn test endpoint' }]);
    const shown = await tools.get('plan')!.execute('show-plan', { queries: [{ reasoning: 'Read the active task identity', action: 'show' }] }, undefined, undefined, ctx);
    const shownText = shown.content.flatMap((part) => part.type === 'text' ? [part.text] : []).join('\n');
    const taskId = /Task IDs for agent\.planStep: 1=([^\s,]+)/.exec(shownText)?.[1];
    assert.ok(taskId, 'model-visible plan text must provide the stable ID required by agent.planStep');
    const params = { queries: [{ reasoning: 'Delegate independent endpoint work', type: 'spawn', task: 'Implement the endpoint', planStep: taskId }] };
    await tools.get('agent')!.execute('assigned-spawn', params, undefined, undefined, ctx);
    const prompt = String(mock.writes.find((write) => write.type === 'prompt')?.message);
    assert.ok(prompt.includes(taskId));
    assert.ok(prompt.includes('src/endpoint.ts'));
    assert.ok(prompt.includes('Endpoint verified'));
    assert.ok(prompt.includes('yarn test endpoint'));
    await assert.rejects(tools.get('agent')!.execute('duplicate-spawn', params, undefined, undefined, ctx), /already has live worker/);
    assert.equal(listWorkerLedgerEntries().length, 1);
  } finally {
    mock.exitCode = 0;
    mock._emit('close', 0, null);
    clearPlan(scope);
    fs.rmSync(workspace, { recursive: true, force: true });
  }
});

test('cancelling one agent wait preserves another waiter and normal completion', async () => {
  const mock = makeMockAgentProcess();
  setAgentProcessFactoryForTests(() => mock as never);
  const record = spawnRpcAgent({ task: 'independent result observers', resourceMode: 'lean' });
  const controller = new AbortController();
  const cancelled = assert.rejects(waitForAgent(record, { signal: controller.signal }), { name: 'AbortError' });
  const collecting = waitForAgent(record);
  controller.abort();
  await cancelled;
  assert.equal(record.waiters.size, 1);
  assert.equal(record.activityListeners.size, 1);
  mock._emit('stdout:data', Buffer.from(`${JSON.stringify({ type: 'agent_end', messages: [] })}\n`));
  assert.deepEqual(await collecting, { reason: 'terminal', stillRunning: false, probedAlive: false });
  assert.equal(record.waiters.size, 0);
  assert.equal(record.activityListeners.size, 0);
});

test('L2: live ledger ticker runs while a worker is active and stops when it finishes', () => {
  if (isSubagentProcess()) return;
  const ctx = {
    hasUI: true,
    ui: { setStatus: () => {}, setWidget: () => {} },
  } as never;

  const mock = makeMockAgentProcess({ stdinThrows: false, exitImmediately: false });
  setAgentProcessFactoryForTests(() => mock as never);
  spawnRpcAgent({ task: 'long job', resourceMode: 'lean' }, ctx);

  refreshAgentLedgerUi(ctx);
  assert.equal(isLedgerTickerActiveForTests(), true, 'ticker active while a worker runs');

  mock.exitCode = 0;
  mock._emit('close', 0, null);
  refreshAgentLedgerUi(ctx);
  assert.equal(isLedgerTickerActiveForTests(), false, 'ticker stops once no worker is active');

  stopLedgerTickerForTests();
});

test('L3: refreshAgentLedgerUi with no agents stops the ticker without creating duplicate status/widget surfaces', () => {
  if (isSubagentProcess()) return;
  const statusCalls: unknown[] = [];
  const widgetCalls: unknown[] = [];
  const ctx = {
    hasUI: true,
    ui: {
      setStatus: (...args: unknown[]) => statusCalls.push(args),
      setWidget: (...args: unknown[]) => widgetCalls.push(args),
    },
  } as never;

  refreshAgentLedgerUi(ctx); // registry cleared by beforeEach
  assert.equal(isLedgerTickerActiveForTests(), false, 'no ticker without active workers');
  assert.deepEqual(statusCalls, [], 'agent state no longer duplicates into a compact status chip');
  assert.deepEqual(widgetCalls, [], 'agent state does not create a second persistent widget');

  stopLedgerTickerForTests();
});

test('/octocode-agents hide suppresses unified footer detail until list shows it again', async () => {
  if (isSubagentProcess()) return;
  const statusCalls: unknown[] = [];
  const widgetCalls: unknown[] = [];
  const ctx = {
    hasUI: true,
    ui: {
      setStatus: (...args: unknown[]) => statusCalls.push(args),
      setWidget: (...args: unknown[]) => widgetCalls.push(args),
      notify: () => {},
    },
  } as never;
  const mock = makeMockAgentProcess({ stdinThrows: false, exitImmediately: false });
  setAgentProcessFactoryForTests(() => mock as never);
  spawnRpcAgent({ task: 'visible worker', resourceMode: 'lean' }, ctx);
  assert.ok(listVisibleWorkerLedgerEntries().length > 0, 'agent panel starts visible');

  await handleOctocodeAgentsCommand('hide', ctx);
  assert.equal(listVisibleWorkerLedgerEntries().length, 0, 'hide suppresses the shared visible-ledger row builder');
  assert.deepEqual(statusCalls, [], 'hide does not create a duplicate status surface');
  assert.deepEqual(widgetCalls, [], 'worker state never creates a duplicate below-editor surface');

  await handleOctocodeAgentsCommand('list', ctx);
  assert.ok(listVisibleWorkerLedgerEntries().length > 0, 'list shows the footer ledger rows again');

  stopLedgerTickerForTests();
});

test('L4: no ticker is started when the UI is absent (headless)', () => {
  if (isSubagentProcess()) return;
  const ctx = { hasUI: false, ui: { setStatus: () => {}, setWidget: () => {} } } as never;
  const mock = makeMockAgentProcess({ stdinThrows: false, exitImmediately: false });
  setAgentProcessFactoryForTests(() => mock as never);
  spawnRpcAgent({ task: 'headless job', resourceMode: 'lean' }, ctx);
  refreshAgentLedgerUi(ctx);
  assert.equal(isLedgerTickerActiveForTests(), false, 'headless mode never starts the ledger ticker');
  stopLedgerTickerForTests();
});

// ─── SEV-1: workers inherit the parent's model/provider when unset ────────────

test('SEV-1: spawnRpcAgent defaults worker --model/--provider to the parent ctx.model', () => {
  if (isSubagentProcess()) return;
  const mock = makeMockAgentProcess({ stdinThrows: false, exitImmediately: false });
  setAgentProcessFactoryForTests(() => mock as never);
  const ctx = {
    hasUI: false,
    cwd: process.cwd(),
    model: { id: 'claude-haiku-4-5-20251001', provider: 'guy-provider-anthropic' },
    ui: { setStatus: () => {}, setWidget: () => {} },
  } as never;
  const record = spawnRpcAgent({ task: 'inherit model', resourceMode: 'lean' }, ctx);
  const args = record.args;
  const modelIdx = args.indexOf('--model');
  const provIdx = args.indexOf('--provider');
  assert.ok(modelIdx >= 0 && args[modelIdx + 1] === 'claude-haiku-4-5-20251001', 'worker inherits parent model id');
  assert.ok(provIdx >= 0 && args[provIdx + 1] === 'guy-provider-anthropic', 'worker inherits parent provider');
});

test('SEV-1: an explicit model/provider still wins over the parent default', () => {
  if (isSubagentProcess()) return;
  const mock = makeMockAgentProcess({ stdinThrows: false, exitImmediately: false });
  setAgentProcessFactoryForTests(() => mock as never);
  const ctx = { hasUI: false, model: { id: 'parent-model', provider: 'parent-prov' }, ui: { setStatus: () => {}, setWidget: () => {} } } as never;
  const record = spawnRpcAgent({ task: 't', resourceMode: 'lean', model: 'chosen-model', provider: 'chosen-prov' }, ctx);
  assert.equal(record.args[record.args.indexOf('--model') + 1], 'chosen-model');
  assert.equal(record.args[record.args.indexOf('--provider') + 1], 'chosen-prov');
});

test('SEV-1: an explicit model without provider does not inherit an unrelated parent provider', () => {
  if (isSubagentProcess()) return;
  const mock = makeMockAgentProcess({ stdinThrows: false, exitImmediately: false });
  setAgentProcessFactoryForTests(() => mock as never);
  const ctx = { hasUI: false, model: { id: 'parent-model', provider: 'parent-prov' }, ui: { setStatus: () => {}, setWidget: () => {} } } as never;
  assert.throws(
    () => spawnRpcAgent({ task: 't', resourceMode: 'lean', model: 'claude-haiku-4-5' }, ctx),
    /requires an explicit provider/,
  );
});

test('SEV-1: OpenAI GPT-5 tool-calling workers force --thinking off to override inherited defaults', () => {
  if (isSubagentProcess()) return;
  const mock = makeMockAgentProcess({ stdinThrows: false, exitImmediately: false });
  setAgentProcessFactoryForTests(() => mock as never);
  const record = spawnRpcAgent({
    task: 'Goal: test\nContext: test\nScope: test\nOwnership: test\nAcceptance: test\nReturn: test',
    resourceMode: 'octocode',
    model: 'gpt-5.4-mini',
    provider: 'guy-provider-openai',
    thinking: 'low',
    tools: ['web', 'MCPTool'],
  }, { hasUI: false, ui: { setStatus: () => {}, setWidget: () => {} } } as never);

  const thinkingIdx = record.args.indexOf('--thinking');
  assert.ok(thinkingIdx >= 0, 'tool-calling OpenAI GPT-5 worker must override inherited thinking');
  assert.equal(record.args[thinkingIdx + 1], 'off', 'the explicit override prevents reasoning_effort from reaching Chat Completions');
  assert.ok(record.args.includes('--tools'), 'worker still receives its tool allowlist');
  assert.ok(
    record.policyWarnings.some((warning) => /Forced --thinking off for OpenAI GPT-5 tool-calling worker/.test(warning)),
    'spawn policy explains the compatibility override',
  );
});

test('SEV-1: modelRegistry rejects mismatched model/provider pairs before spawning', () => {
  if (isSubagentProcess()) return;
  const mock = makeMockAgentProcess({ stdinThrows: false, exitImmediately: false });
  setAgentProcessFactoryForTests(() => mock as never);
  const ctx = {
    hasUI: false,
    modelRegistry: { find: () => undefined },
    ui: { setStatus: () => {}, setWidget: () => {} },
  } as never;
  assert.throws(
    () => spawnRpcAgent({ task: 't', resourceMode: 'lean', model: 'claude-haiku-4-5', provider: 'guy-provider-openai' }, ctx),
    /model\/provider not found/,
  );
});

test('SEV-1: agent typed profile inherits the parent provider when the caller does not pass one', async () => {
  if (isSubagentProcess()) return;
  const mock = makeMockAgentProcess({ stdinThrows: false, exitImmediately: false });
  let capturedArgs: string[] = [];
  setAgentProcessFactoryForTests((_command, args) => {
    capturedArgs = args;
    return mock as never;
  });
  const tools = new Map<string, ToolDefinition>();
  registerUnifiedAgentTool(
    { registerTool: (def) => tools.set(def.name, def) },
    Type,
    new Set<string>(),
    (pi, names, def) => { names.add(def.name); pi.registerTool?.(def); },
  );
  const ctx = {
    hasUI: false,
    cwd: process.cwd(),
    model: { id: 'claude-haiku-4-5-20251001', provider: 'guy-provider-anthropic' },
    ui: { setStatus: () => {}, setWidget: () => {} },
  } as never;

  await tools.get('agent')!.execute('id', { queries: [{ reasoning: 'Check typed worker provider inheritance', type: 'spawn',
    profile: 'researcher',
    task: 'Goal: test\nContext: test\nScope: test\nOwnership: test\nAcceptance: test\nReturn: test',
  }] }, undefined, undefined, ctx);

  const providerIdx = capturedArgs.indexOf('--provider');
  assert.equal(providerIdx >= 0, true, 'typed profile must pass an inherited --provider');
  assert.equal(capturedArgs[providerIdx + 1], 'guy-provider-anthropic');
});

// ─── SEV-2: worker model/turn errors are captured into record.error ───────────

test('SEV-2: an errored agent_end message surfaces the model error on record.error', () => {
  if (isSubagentProcess()) return;
  const mock = makeMockAgentProcess({ stdinThrows: false, exitImmediately: false });
  setAgentProcessFactoryForTests(() => mock as never);
  const ctx = { hasUI: false, ui: { setStatus: () => {}, setWidget: () => {} } } as never;
  const record = spawnRpcAgent({ task: 'boom', resourceMode: 'lean' }, ctx);
  const frame = JSON.stringify({
    type: 'agent_end',
    messages: [{ role: 'assistant', content: [], stopReason: 'error', errorMessage: '400 Unsupported model: claude-x' }],
  });
  mock._emit('stdout:data', Buffer.from(frame + '\n'));
  assert.equal(record.error, '400 Unsupported model: claude-x', 'model error captured');
  assert.ok(record.ledgerEvents.some((e) => /worker turn error/.test(e.message ?? '')), 'ledger records the turn error');
});
