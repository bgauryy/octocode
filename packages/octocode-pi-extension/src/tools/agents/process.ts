/**
 * Agent process management: spawn, RPC protocol, sendRpc, and process lifecycle.
 *
 * Depends on: types, registry, ledger, normalization, policy, worktree, kill, node builtins.
 * No imports from agent-tools (uses wireProcessCallbacks for UI refresh).
 *
 * Invariants:
 * - EPIPE in sendRpc: touch(record, 'failed'); notifyWaiters(record) — abort propagation from pipe failure.
 * - proc.on('close') ordering: pendingMessages=0 → touch() → removePromptFiles → cleanupRecordWorktree
 *   → syncWorkerRegistry('leave') → notifyWaiters → _refreshUi(ctx).
 * - proc.on('error') same ordering without touch transition override.
 * - stdout RPC/probe resolution: processRpcLine resolves record.pendingProbes via the record field.
 */
import { createHash, randomUUID } from 'node:crypto';
import { StringDecoder } from 'node:string_decoder';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { withPeerCoordination } from './coordination.js';
export { withPeerCoordination } from './coordination.js';
import { getInstallSource } from '../../assets.js';
import { extensionTmpRoot, extensionWorkspaceRoot } from '../../extension-paths.js';
import { inspectWorkerAwarenessAutomatically } from '../awareness-worker-audit.js';
import { getRandomAgentName } from '../../agentNames.js';
import {
  cleanupWorktreeIfNoWork,
  createAgentWorktree,
  removeAgentWorktree,
  type InternalWorktreeState,
} from '../worktree.js';
import type { PiContext } from '../../types.js';
import {
  type AgentRecord,
  type SpawnAgentParams,
  HANDBACK_ARTIFACT_FILENAME,
  SUBAGENT_ENV_VAR,
  AWARENESS_AGENT_ENV_VAR,
  EXIT_SIGNALS,
  DEFAULT_SPAWN_POLICY,
} from './types.js';
import {
  normalizeWorkerOutput,
  evaluateWorkerRecoveryRisk,
  extractDeltaSummary,
  extractTextFromMessage,
  isAssistantOutputMessage,
} from './normalization.js';
import {
  FORBIDDEN_WORKER_TOOLS,
  resolveSpawnPolicy,
  evaluateStepBudget,
  getWorkerTools,
  shouldForceThinkingOffForToolCallingWorker,
  buildInitialPrompt,
  resolveWorkerModelParams,
  validateWorkerModelParams,
  evaluateSpawnPolicy,
} from './policy.js';
import {
  agents,
  getProcessFactory,
  setLedgerHidden,
  isSubagentProcess,
  isProcessAlive,
  activeAgentCount,
  evictStaleAgents,
} from './registry.js';
import {
  touch,
  pushLedgerEvent,
  pushCapped,
  notifyWaiters,
  recordMessageActivity,
  previewMessage,
} from './ledger.js';
import { killAgent, syncWorkerRegistry, removePromptFiles } from './kill.js';
import { buildAwarenessContext } from '../awareness-context.js';
import { bindSpawnedWorkerCapabilities, revokeWorkerCapabilities } from '../worker-capabilities.js';

// ─── UI callback wiring ────────────────────────────────────────────────────────

// process.ts must not import from agent-tools.ts (cycle). The rendering layer
// injects these callbacks once at module-init via wireProcessCallbacks().
let _refreshUi: (ctx?: PiContext) => void = () => {};
let _stopTicker: () => void = () => {};

/**
 * Wire the rendering-layer callbacks into the spawn system.
 * Called once at module init from agent-tools.ts.
 */
export function wireProcessCallbacks(
  refreshUi: (ctx?: PiContext) => void,
  stopTicker: () => void
): void {
  _refreshUi = refreshUi;
  _stopTicker = stopTicker;
}

// ─── Process cleanup handlers ─────────────────────────────────────────────────────

let processCleanupHandlersInstalled = false;

/**
 * Kill every live worker on process exit/signal. Called by installProcessCleanupHandlers
 * and exported so callers (e.g. src/index.ts) can trigger manual shutdown.
 */
export function cleanupSpawnedAgentsForShutdown(): number {
  // Kill every worker whose process is still alive — including idle ones, whose
  // process stays up between turns and would otherwise survive as an orphan.
  const alive = [...agents.values()].filter(record => isProcessAlive(record));
  for (const record of alive) killAgent(record, { forceKillDelayMs: 0 });
  // The killed children's close/stderr events fire on later ticks and call
  // _refreshUi; hide the ledger so those callbacks clear rather than
  // resurrect the status/widget into the next session. Spawning (or an explicit
  // list/status action) un-hides it again.
  setLedgerHidden(true);
  _stopTicker();
  return alive.length;
}

function installProcessCleanupHandlers(): void {
  if (processCleanupHandlersInstalled || isSubagentProcess()) return;
  processCleanupHandlersInstalled = true;
  const cleanup = () => {
    cleanupSpawnedAgentsForShutdown();
  };
  process.once('beforeExit', cleanup);
  process.once('exit', cleanup);
  for (const signal of EXIT_SIGNALS) {
    process.once(signal, () => {
      cleanup();
      process.kill(process.pid, signal);
    });
  }
}

// ─── Invocation helpers ───────────────────────────────────────────────────────────

function getPiInvocation(args: string[]): { command: string; args: string[] } {
  const currentScript = process.argv[1];
  const isBunVirtualScript = currentScript?.startsWith('/$bunfs/root/');
  if (currentScript && !isBunVirtualScript && fs.existsSync(currentScript)) {
    return { command: process.execPath, args: [currentScript, ...args] };
  }

  const execName = path.basename(process.execPath).toLowerCase();
  const isGenericRuntime = /^(node|bun)(\.exe)?$/.test(execName);
  if (!isGenericRuntime) return { command: process.execPath, args };

  return { command: 'pi', args };
}

function safeName(value: string): string {
  return value.replace(/[^\w.-]+/g, '_').slice(0, 80) || 'agent';
}

function writeTempPromptFile(name: string, text: string): string {
  const root = path.join(extensionTmpRoot(), 'prompts');
  fs.mkdirSync(root, { recursive: true, mode: 0o700 });
  const dir = fs.mkdtempSync(path.join(root, 'worker-'));
  const filePath = path.join(dir, `${safeName(name)}.md`);
  fs.writeFileSync(filePath, text, { encoding: 'utf8', mode: 0o600 });
  return filePath;
}

function buildHandbackPath(workspace: string, agentId: string): string {
  return path.join(
    extensionWorkspaceRoot(workspace),
    'workers',
    agentId,
    HANDBACK_ARTIFACT_FILENAME
  );
}

function ensureHandbackDir(filePath: string): void {
  fs.mkdirSync(path.dirname(filePath), { recursive: true, mode: 0o700 });
}

function prepareHandbackPath(
  workspace: string,
  agentId: string
): { path: string; warning?: string } {
  const preferredPath = buildHandbackPath(workspace, agentId);
  try {
    ensureHandbackDir(preferredPath);
    return { path: preferredPath };
  } catch (preferredError) {
    const fallbackPath = path.join(
      extensionTmpRoot(),
      'handbacks',
      agentId,
      HANDBACK_ARTIFACT_FILENAME
    );
    try {
      ensureHandbackDir(fallbackPath);
    } catch (fallbackError) {
      const preferredMessage =
        preferredError instanceof Error
          ? preferredError.message
          : String(preferredError);
      const fallbackMessage =
        fallbackError instanceof Error
          ? fallbackError.message
          : String(fallbackError);
      throw new Error(
        `Unable to prepare worker handback directory (${preferredMessage}); fallback also failed (${fallbackMessage}).`
      );
    }
    const preferredMessage =
      preferredError instanceof Error
        ? preferredError.message
        : String(preferredError);
    return {
      path: fallbackPath,
      warning: `Parent workspace handback directory is unavailable; using temporary fallback ${fallbackPath} (${preferredMessage}).`,
    };
  }
}

function workerAwarenessAgentId(workerId: string): string {
  const parentId = process.env[AWARENESS_AGENT_ENV_VAR]?.trim() || 'pi-agent';
  return `${parentId}:worker:${workerId.slice(0, 8)}`;
}

function cleanupPromptFiles(promptFiles: string[]): void {
  for (const filePath of promptFiles) {
    try {
      fs.rmSync(path.dirname(filePath), { recursive: true, force: true });
    } catch {
      /* best-effort */
    }
  }
}

export function getActiveAgentUi(
  ctx?: PiContext
): NonNullable<PiContext['ui']> | undefined {
  try {
    if (!ctx?.hasUI) return undefined;
    return ctx.ui;
  } catch {
    // Pi invalidates every ctx getter after session replacement/reload. Long-lived
    // worker callbacks may still drain afterward, so best-effort UI work must stop.
    return undefined;
  }
}

async function approveWorktreeIsolation(
  params: SpawnAgentParams,
  ctx?: PiContext
): Promise<SpawnAgentParams> {
  if (params.isolation !== 'worktree') return params;
  const ui = getActiveAgentUi(ctx);
  if (typeof ui?.select !== 'function') {
    throw new Error(
      'isolation:"worktree" requires an interactive UI approval; non-interactive hosts fail closed. Re-run with isolation:"shared" to use the current cwd intentionally.'
    );
  }
  const create = 'Create isolated worktree';
  const shared = 'Use current repo / shared cwd';
  const cancel = 'Cancel spawn';
  const picked = await ui.select(
    'Spawn this worker in an isolated git worktree?',
    [create, shared, cancel]
  );
  if (picked === create) return { ...params, worktreeDecision: 'create' };
  if (picked === shared)
    return { ...params, isolation: 'shared', worktreeDecision: 'shared' };
  throw new Error('Spawn cancelled before creating a worktree.');
}

function withWorktreePromptContext(
  params: SpawnAgentParams,
  worktree: InternalWorktreeState
): SpawnAgentParams {
  const preamble = [
    'Worktree isolation is active for this worker.',
    `- Worktree path: ${worktree.path}`,
    `- Branch: ${worktree.branch}`,
    `- Base commit: ${worktree.baseCommit}`,
    '- Report repo-relative paths in handback; the parent ledger exposes the isolated path for review.',
  ].join('\n');
  return {
    ...params,
    cwd: worktree.path,
    context: params.context ? `${preamble}\n\n${params.context}` : preamble,
  };
}

function cleanupRecordWorktree(record: AgentRecord): void {
  if (
    !record.worktree ||
    record.worktree.mergeState === 'discarded' ||
    record.worktree.mergeState === 'merged'
  )
    return;
  try {
    const outcome = cleanupWorktreeIfNoWork(record.worktree);
    pushLedgerEvent(
      record,
      'worktree',
      outcome === 'removed'
        ? 'removed clean worktree'
        : 'kept unmerged worktree',
      record.worktree
    );
  } catch (cleanupError) {
    pushLedgerEvent(
      record,
      'worktree',
      `worktree cleanup failed: ${cleanupError instanceof Error ? cleanupError.message : String(cleanupError)}`,
      record.worktree
    );
  }
}

// ─── Awareness helpers ────────────────────────────────────────────────────────────

/** Awareness ids of other still-alive workers, for peer-messaging discovery. */
function collectPeerAwarenessIds(excludeId: string): string[] {
  const ids: string[] = [];
  for (const rec of agents.values()) {
    if (rec.id === excludeId || !rec.awarenessAgentId) continue;
    if (
      rec.status !== 'exited' &&
      rec.status !== 'failed' &&
      rec.status !== 'killed'
    )
      ids.push(rec.awarenessAgentId);
  }
  return ids;
}

// ─── Pi argument builder ───────────────────────────────────────────────────────────

function buildPiArgs(
  params: SpawnAgentParams,
  name: string,
  promptFiles: string[]
): string[] {
  const resourceMode = params.resourceMode ?? 'lean';
  const args = ['--mode', 'rpc'];
  const workerTools = getWorkerTools(params);

  if (params.noSession !== false) args.push('--no-session');
  // Load specific skills even when --no-skills is active (additive)
  if (resourceMode !== 'lean') {
    for (const skillPath of params.skills ?? []) args.push('--skill', skillPath);
  }
  args.push('--name', name);
  args.push('--exclude-tools', [...FORBIDDEN_WORKER_TOOLS].join(','));

  if (params.provider) args.push('--provider', params.provider);
  if (params.model) args.push('--model', params.model);
  if (shouldForceThinkingOffForToolCallingWorker(params, workerTools))
    args.push('--thinking', 'off');
  else if (params.thinking) args.push('--thinking', params.thinking);
  if (workerTools.length) args.push('--tools', workerTools.join(','));
  else args.push('--no-tools');
  args.push('--no-context-files');

  if (resourceMode === 'lean') {
    const guardPath = fileURLToPath(new URL('../../worker-guard.js', import.meta.url));
    args.push(
      '--no-extensions',
      '-e', fs.existsSync(guardPath) ? guardPath : guardPath.replace(/\.js$/, '.ts'),
      '--no-skills',
      '--no-prompt-templates',
      '--no-themes'
    );
  } else {
    args.push(
      '--no-extensions',
      '-e',
      getInstallSource(),
      '--no-skills',
      '--no-prompt-templates',
      '--no-themes'
    );
  }

  const systemPrompt = String(params.systemPrompt ?? '').trim();
  if (systemPrompt) {
    const filePath = writeTempPromptFile(name, systemPrompt);
    promptFiles.push(filePath);
    args.push('--append-system-prompt', filePath);
  }

  return args;
}

// ─── Normalized result refresh ─────────────────────────────────────────────────────

export function refreshNormalizedResult(record: AgentRecord): void {
  const previousStatus = record.normalizedResult?.status;
  const previousWarnings = record.recoveryRisk.warnings.join('\n');
  const output = record.lastOutput || record.stderr || record.error || '';
  record.normalizedResult = normalizeWorkerOutput(output);
  record.recoveryRisk = evaluateWorkerRecoveryRisk(output);
  // Step-budget circuit-breaker: surface a warning when a worker's completed tool calls
  // reach the budget, so the parent can abort/steer a runaway worker.
  const steps = record.toolCalls.filter(
    call => call.status !== 'running'
  ).length;
  const budget = evaluateStepBudget(
    steps,
    resolveSpawnPolicy(DEFAULT_SPAWN_POLICY).maxStepsPerWorker
  );
  if (
    budget.exceeded &&
    budget.warning &&
    !record.recoveryRisk.warnings.includes(budget.warning)
  ) {
    record.recoveryRisk.warnings.push(budget.warning);
  }
  if (
    record.normalizedResult.status !== 'unknown' &&
    record.normalizedResult.status !== previousStatus
  ) {
    pushLedgerEvent(
      record,
      'handback',
      `handback status: ${record.normalizedResult.status}`
    );
  }
  const newWarnings = record.recoveryRisk.warnings.join('\n');
  if (newWarnings && newWarnings !== previousWarnings) {
    pushLedgerEvent(
      record,
      'policy',
      `recovery risk: ${record.recoveryRisk.warnings.join('; ')}`
    );
  }
}

// ─── RPC protocol helpers ──────────────────────────────────────────────────────────

function captureMessageError(record: AgentRecord, message: unknown): void {
  const m = message as { stopReason?: string; errorMessage?: string };
  const errMsg =
    typeof m.errorMessage === 'string' ? m.errorMessage.trim() : '';
  if (m.stopReason !== 'error' && !errMsg) return;
  const text = errMsg || 'worker model turn failed';
  if (!record.error) record.error = text;
  pushLedgerEvent(record, 'error', `worker turn error: ${text}`);
  touch(record);
}

function updateLastOutput(record: AgentRecord, message: unknown): void {
  if (!isAssistantOutputMessage(message)) return;
  const text = extractTextFromMessage(message);
  if (text) {
    record.lastOutput = text;
    const delta = extractDeltaSummary(text);
    if (delta) record.deltaSummary = delta;
    refreshNormalizedResult(record);
  }
}

function recordInboundMessage(record: AgentRecord, message: unknown): void {
  if (!isAssistantOutputMessage(message)) return;
  const text = extractTextFromMessage(message);
  if (!text) return;
  const fingerprint = createHash('sha256').update(text).digest('hex');
  if (record.lastInboundMessageFingerprint === fingerprint) return;
  record.lastInboundMessageFingerprint = fingerprint;
  recordMessageActivity(
    record,
    'from-agent',
    'reply',
    text,
    `reply received: ${previewMessage(text)}`
  );
}

function getEventToolName(event: Record<string, unknown>): string {
  return String(
    event['toolName'] ??
      event['tool_name'] ??
      event['tool'] ??
      event['name'] ??
      ''
  ).trim();
}

function getEventToolCallId(
  event: Record<string, unknown>
): string | undefined {
  const id = event['toolCallId'] ?? event['tool_call_id'] ?? event['id'];
  return typeof id === 'string' && id.trim() ? id : undefined;
}

function recordToolStart(
  record: AgentRecord,
  event: Record<string, unknown>
): void {
  const toolName = getEventToolName(event);
  if (!toolName) return;
  pushCapped(record.toolCalls, {
    toolCallId: getEventToolCallId(event),
    toolName,
    status: 'running',
    startedAt: Date.now(),
  });
  pushLedgerEvent(record, 'tool', `tool started: ${toolName}`);
  touch(record, 'running');
}

function recordToolEnd(
  record: AgentRecord,
  event: Record<string, unknown>
): void {
  const toolName = getEventToolName(event);
  const toolCallId = getEventToolCallId(event);
  if (!toolName && !toolCallId) return;
  const call = [...record.toolCalls]
    .reverse()
    .find(
      item =>
        (toolCallId
          ? item.toolCallId === toolCallId
          : item.toolName === toolName) && item.status === 'running'
    );
  const isError = Boolean(
    event['isError'] ?? event['is_error'] ?? event['error']
  );
  if (call) {
    call.status = isError ? 'error' : 'done';
    call.finishedAt = Date.now();
    call.isError = isError;
  } else if (toolName) {
    pushCapped(record.toolCalls, {
      toolCallId,
      toolName,
      status: isError ? 'error' : 'done',
      startedAt: Date.now(),
      finishedAt: Date.now(),
      isError,
    });
  }
  if (toolName)
    pushLedgerEvent(
      record,
      'tool',
      `tool ${isError ? 'failed' : 'finished'}: ${toolName}`
    );
  touch(record);
}

function processRpcLine(record: AgentRecord, line: string): void {
  if (!line.trim()) return;
  let event: unknown;
  try {
    event = JSON.parse(line);
  } catch {
    return;
  }

  pushCapped(record.events, event);
  const eventObject = event as Record<string, unknown>;
  const eventType = (event as { type?: string }).type;
  if (eventType === 'tool_call' || eventType === 'tool_execution_start') {
    recordToolStart(record, eventObject);
  } else if (
    eventType === 'tool_result' ||
    eventType === 'tool_execution_end'
  ) {
    recordToolEnd(record, eventObject);
  } else if (eventType === 'response') {
    pushCapped(record.responses, event);
    const resp = event as {
      id?: string;
      success?: boolean;
      command?: string;
      error?: string;
    };
    // A correlated reply to a liveness probe: resolve the pending probe so the
    // waiter learns the worker is alive-but-quiet (not hung). Any response at all
    // proves the RPC channel is live, so it also counts as a heartbeat below.
    if (resp.id && record.pendingProbes.has(resp.id)) {
      const resolveProbe = record.pendingProbes.get(resp.id)!;
      record.pendingProbes.delete(resp.id);
      resolveProbe();
    }
    if (resp.success === false) {
      if (!record.error)
        record.error =
          resp.error ?? `RPC command failed: ${resp.command ?? 'unknown'}`;
      pushLedgerEvent(record, 'error', record.error);
    }
    // Heartbeat on every response (success or not) so a blocking wait resets its
    // silence watchdog whenever the channel proves live.
    touch(record);
  } else if (eventType === 'agent_start') {
    record.awarenessInspection = undefined;
    // A structured result belongs to the turn that just ended. Clear it before
    // exposing the new turn as running, otherwise a prior [DONE]/[BLOCKED]
    // overrides the live process state in the footer and ledger.
    record.normalizedResult = undefined;
    record.deltaSummary = undefined;
    record.lastInboundMessageFingerprint = undefined;
    // ONE queued turn has started: decrement (never hard-reset) the pending
    // counter, so when two follow-ups are queued the ledger keeps showing
    // 'queued' work and agent_end after turn 1 does not resolve `wait` while
    // turn 2 has yet to run.
    record.pendingMessages = Math.max(0, (record.pendingMessages ?? 0) - 1);
    touch(record, 'running');
  } else if (
    eventType === 'message_end' &&
    (event as { message?: unknown }).message
  ) {
    const message = (event as { message: unknown }).message;
    pushCapped(record.messages, message);
    captureMessageError(record, message);
    updateLastOutput(record, message);
    recordInboundMessage(record, message);
    touch(record);
  } else if (eventType === 'agent_end') {
    const messages = (event as { messages?: unknown[] }).messages;
    if (Array.isArray(messages)) {
      for (const message of messages) {
        captureMessageError(record, message);
        updateLastOutput(record, message);
        recordInboundMessage(record, message);
      }
    }
    // agent_end {willRetry:true} means the worker aborted on context overflow
    // and Pi is compacting + retrying the turn — it is still working, so a
    // pending wait must not resolve with the incomplete lastOutput.
    // willRetry (context-overflow retry) or a still-pending queued turn both mean the
    // worker is not actually done — keep it non-terminal and do not resolve waiters.
    if ((event as { willRetry?: boolean }).willRetry === true) {
      touch(record);
    } else if (record.pendingMessages > 0) {
      // This turn ended; a queued follow-up is not running until agent_start.
      // pendingMessages keeps wait() blocking through this idle process boundary.
      touch(record, 'idle');
    } else {
      record.awarenessInspection = inspectWorkerAwarenessAutomatically(record);
      touch(record, 'idle');
      notifyWaiters(record);
    }
  }
}

/**
 * Send an RPC message to the spawned agent process.
 * Returns true on success, false on failure (EPIPE / ERR_STREAM_WRITE_AFTER_END).
 * On failure the record is transitioned to 'failed' and all waiters are notified
 * so agent type:'wait' resolves immediately instead of hanging to timeout.
 *
 * Invariant (EPIPE): touch(record, 'failed'); notifyWaiters(record) — abort
 * propagation from pipe failure so wait() resolves immediately.
 */
export function sendRpc(
  record: AgentRecord,
  payload: Record<string, unknown>,
  explicitId?: string
): boolean {
  const id = explicitId ?? `${record.id}-${record.nextRequestId++}`;
  try {
    record.process.stdin.write(`${JSON.stringify({ id, ...payload })}\n`);
    return true;
  } catch (error) {
    // Writing to a destroyed/closed stdin throws EPIPE / ERR_STREAM_WRITE_AFTER_END.
    // H4: Transition to 'failed' and notify waiters — without this, any pending
    // action:'wait' would hang until timeout because the record stays in 'starting'.
    record.error = error instanceof Error ? error.message : String(error);
    touch(record, 'failed');
    notifyWaiters(record);
    return false;
  }
}

// ─── Spawn ────────────────────────────────────────────────────────────────────────────

export async function prepareSpawnAgentParams(
  params: SpawnAgentParams,
  ctx?: PiContext
): Promise<SpawnAgentParams> {
  return approveWorktreeIsolation(params, ctx);
}

export function spawnRpcAgent(
  params: SpawnAgentParams,
  ctx?: PiContext
): AgentRecord {
  if (!buildInitialPrompt(params))
    throw new Error('agent spawn requires task.');
  installProcessCleanupHandlers();

  const id = randomUUID();
  const name = params.name ? String(params.name) : getRandomAgentName();
  const requestedCwd = path.resolve(
    String(params.cwd ?? ctx?.cwd ?? process.cwd())
  );
  const promptFiles: string[] = [];
  // SEV-1: workers resolve models against the same catalog as the parent, but Pi's
  // bare default (google/grok) is often unconfigured/unreachable — an unset worker
  // model silently errors every turn (0 tools run). Inherit the parent's known-working
  // model+provider when the caller didn't pin one, so delegation works by default.
  const effectiveParams = resolveWorkerModelParams(
    { ...params, cwd: requestedCwd },
    ctx
  );
  validateWorkerModelParams(effectiveParams, ctx);
  const awarenessAgentId = workerAwarenessAgentId(id);

  // M7: Enforce a hard cap on active (non-droppable) agents before spawning a new process.
  // Evict droppable (exited/failed/killed) agents first to reclaim slots, then refuse if
  // non-droppable agents still fill the registry. Checked before processFactory to ensure
  // no process is leaked when the cap is exceeded.
  evictStaleAgents();
  const policyResult = evaluateSpawnPolicy(effectiveParams, activeAgentCount());
  if (!policyResult.allowed) {
    revokeWorkerCapabilities(id);
    cleanupPromptFiles(promptFiles);
    throw new Error(
      `${policyResult.reason} Kill or wait for existing agents before spawning more.`
    );
  }

  let worktree: InternalWorktreeState | undefined;
  let spawnParams = effectiveParams;
  let cwd = requestedCwd;
  if (effectiveParams.isolation === 'worktree') {
    if (effectiveParams.worktreeDecision !== 'create') {
      revokeWorkerCapabilities(id);
      cleanupPromptFiles(promptFiles);
      throw new Error(
        'isolation:"worktree" requires explicit user approval before creating a git worktree.'
      );
    }
    worktree = createAgentWorktree({
      parentCwd: requestedCwd,
      agentId: id,
      name,
      includeUncommitted: effectiveParams.includeUncommitted,
    });
    spawnParams = withWorktreePromptContext(effectiveParams, worktree);
    cwd = worktree.path;
  }
  const peerIds = collectPeerAwarenessIds(id);
  const awarenessWorkspace = cwd;
  const awarenessDatabase = buildAwarenessContext({ cwd: requestedCwd }).database;
  const parentAwarenessAgentId =
    process.env[AWARENESS_AGENT_ENV_VAR]?.trim() || 'pi-agent';
  const handback = prepareHandbackPath(ctx?.cwd ?? requestedCwd, id);
  const handbackPath = handback.path;
  const task = withPeerCoordination(
    buildInitialPrompt(spawnParams),
    awarenessAgentId,
    peerIds,
    {
      parentId: parentAwarenessAgentId,
      handbackPath,
    }
  );

  let childCapabilities: ReturnType<typeof bindSpawnedWorkerCapabilities>;
  let invocation: ReturnType<typeof getPiInvocation>;
  try {
    childCapabilities = bindSpawnedWorkerCapabilities(id, effectiveParams);
    invocation = getPiInvocation(buildPiArgs(childCapabilities.params, name, promptFiles));
  } catch (error) {
    revokeWorkerCapabilities(id);
    cleanupPromptFiles(promptFiles);
    if (worktree) removeAgentWorktree(worktree, { force: true });
    throw error;
  }
  let proc;
  try {
    proc = getProcessFactory()(invocation.command, invocation.args, {
      cwd,
      shell: false,
      stdio: ['pipe', 'pipe', 'pipe'],
      env: {
        ...process.env,
        ...childCapabilities.env,
        [SUBAGENT_ENV_VAR]: '1',
        [AWARENESS_AGENT_ENV_VAR]: awarenessAgentId,
        OCTOCODE_AWARENESS_DB: awarenessDatabase,
        // getPiInvocation() re-executes process.argv[1], which for any octocode-agent
        // process is bin/octocode-agent.mjs. Left unset, a worker spawned from a
        // parent running in the default SDK-embed mode would inherit that mode and
        // re-enter launchWithSdk() — whose arg parser does not understand
        // --tools/--exclude-tools/-e/--append-system-prompt/--skill, silently
        // dropping the curated allowlist buildPiArgs() just built. Force the
        // subprocess path, which forwards argv verbatim to the real Pi CLI.
        OCTOCODE_LAUNCHER_MODE: 'subprocess',
      },
    });
  } catch (error) {
    revokeWorkerCapabilities(id);
    // processFactory threw before the record was added to `agents`, so removePromptFiles()
    // (wired to the record's 'close'/'error' handlers) would never run. Clean up the temp
    // system-prompt files buildPiArgs wrote so a failing factory does not leak files.
    cleanupPromptFiles(promptFiles);
    if (worktree) removeAgentWorktree(worktree, { force: true });
    throw error;
  }

  const record: AgentRecord = {
    id,
    name,
    cwd,
    command: invocation.command,
    args: invocation.args,
    task: String(effectiveParams.task ?? '').trim(),
    planStep: effectiveParams.planStep?.trim() || undefined,
    planId: effectiveParams.planId,
    planScope: effectiveParams.planScope,
    process: proc,
    status: 'starting',
    startedAt: Date.now(),
    updatedAt: Date.now(),
    stderr: '',
    events: [],
    messages: [],
    responses: [],
    toolCalls: [],
    lastOutput: '',
    deltaSummary: undefined,
    handbackPath,
    pendingMessages: 0,
    normalizedResult: normalizeWorkerOutput(''),
    recoveryRisk: evaluateWorkerRecoveryRisk(''),
    ledgerEvents: [],
    policyWarnings: handback.warning
      ? [...policyResult.warnings, handback.warning]
      : policyResult.warnings,
    promptFiles,
    waiters: new Set(),
    activityListeners: new Set(),
    pendingProbes: new Map(),
    nextRequestId: 1,
    worktree,
    awarenessAgentId,
    awarenessWorkspace,
    awarenessDatabase,
    capabilityGrant: childCapabilities.grant,
  };
  pushLedgerEvent(record, 'spawned', `spawned ${name}`, { awarenessAgentId });
  if (record.worktree)
    pushLedgerEvent(
      record,
      'worktree',
      `created worktree ${record.worktree.branch}`,
      record.worktree
    );
  for (const warning of record.policyWarnings)
    pushLedgerEvent(record, 'policy', warning);
  agents.set(id, record);
  // Register the worker in the shared Awareness agent list (best-effort, advisory).
  syncWorkerRegistry('join', record);
  // Evict droppable agents to keep registry size ≤ MAX_AGENT_RECORDS.
  // The pre-spawn call (M7 cap check) runs before processFactory to avoid leaking
  // a process when the non-droppable cap is exceeded. This post-set call cleans up
  // droppable (exited/failed/killed) agents after the new record is in the map so
  // the total registry size stays bounded even when non-droppable count < cap.
  evictStaleAgents();

  let stdoutBuffer = '';
  // Decode incrementally: a multibyte UTF-8 sequence split across two `data`
  // chunks must not be turned into replacement chars — inside a JSON RPC line
  // that corruption makes JSON.parse throw and the event is silently dropped.
  const rpcDecoder = new StringDecoder('utf8');
  proc.stdout.on('data', chunk => {
    stdoutBuffer += rpcDecoder.write(chunk);
    const lines = stdoutBuffer.split('\n');
    stdoutBuffer = lines.pop() ?? '';
    for (const line of lines) processRpcLine(record, line);
    _refreshUi(ctx);
  });
  proc.stderr.on('data', chunk => {
    record.stderr += chunk.toString();
    pushLedgerEvent(record, 'status', 'stderr received');
    touch(record);
    _refreshUi(ctx);
  });
  proc.on('error', error => {
    revokeWorkerCapabilities(id);
    record.error = error instanceof Error ? error.message : String(error);
    pushLedgerEvent(record, 'error', record.error);
    // Dead process: no agent_start will ever arrive to drain queued turns, so
    // strand pendingMessages at zero or the record never becomes terminal.
    record.pendingMessages = 0;
    touch(record, 'failed');
    removePromptFiles(record);
    cleanupRecordWorktree(record);
    syncWorkerRegistry('leave', record);
    notifyWaiters(record);
    _refreshUi(ctx);
  });
  proc.on('close', (code, signal) => {
    revokeWorkerCapabilities(id);
    stdoutBuffer += rpcDecoder.end();
    if (stdoutBuffer.trim()) processRpcLine(record, stdoutBuffer);
    stdoutBuffer = '';
    record.exitCode = typeof code === 'number' ? code : undefined;
    record.signal = typeof signal === 'string' ? signal : undefined;
    // Process is gone: any queued turn that never reached agent_start is stranded,
    // so floor the counter or isTerminal() (and thus `wait`) never resolves and the
    // ledger keeps showing 'queued' against a dead worker.
    record.pendingMessages = 0;
    if (record.status !== 'killed')
      touch(record, code === 0 ? 'exited' : 'failed');
    record.awarenessInspection = inspectWorkerAwarenessAutomatically(record);
    pushLedgerEvent(
      record,
      record.status === 'failed' ? 'error' : 'exit',
      `process closed with code ${record.exitCode ?? 'unknown'}`
    );
    removePromptFiles(record);
    cleanupRecordWorktree(record);
    syncWorkerRegistry('leave', record);
    notifyWaiters(record);
    _refreshUi(ctx);
  });

  // A successful write queues startup; agent_start proves execution. If the
  // write fails, sendRpc already marks failure and wakes waiters.
  if (sendRpc(record, { type: 'prompt', message: task })) {
    pushLedgerEvent(record, 'message', 'initial prompt sent');
    touch(record);
  }
  // Make silent or slow-starting workers visible immediately. Event handlers will
  // keep the unified panel/footer fresh once stdout/stderr/close events arrive.
  _refreshUi(ctx);
  return record;
}
