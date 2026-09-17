/**
 * lifecycle.ts — Agent lifecycle operations and single-agent result rendering.
 *
 * Owns: renderSingleAgentResult, steerWorkerById, getWorkerTranscript,
 *       executeAgentLifecycle, executeSpawnQuery.
 *
 * No imports from agent-tools (that file is being eliminated; all concrete
 * dependencies are drawn directly from the agents/ sub-modules).
 */
import type { PiContext, ToolCallResult } from '../../types.js';
import { SUBAGENT_WORKER_CONTRACT } from '../../contracts/prompts/index.js';
import { setManagedStatus } from '../runtime-renderer.js';
import type { AgentRecord, SpawnAgentParams, WaitOutcome } from './types.js';
import {
  agents,
  isTerminal,
  isDroppable,
  isProcessAlive,
  findAgentByIdOrPrefix,
  getAgent,
} from './registry.js';
import {
  touch,
  enqueueWorkerTurn,
  recordMessageActivity,
  previewMessage,
  listWorkerLedgerEntries,
} from './ledger.js';
import { sendRpc, prepareSpawnAgentParams, spawnRpcAgent } from './process.js';
import { waitForAgent } from './wait.js';
import { killAgent } from './kill.js';
import {
  refreshAgentLedgerUi,
  renderAgentResult,
  summarizeAgent,
  formatElapsed,
  formatToolCalls,
} from './rendering.js';
import {
  type AgentProfile,
  PROFILE_TO_SUBAGENT,
  resolvePlanWorkerAssignment,
} from './plan-integration.js';
import {
  SUBAGENT_REGISTRY,
  loadSystemPrompt,
  resolveSubagentSkills,
  type SubagentConfig,
} from '../../subagents.js';
import { routeTask, buildSpawnConfig } from '../browser-agent-tool.js';
import { connectToChrome, cleanupConnection } from '../../chrome-debug.js';
import { SCHEME_REGISTRY } from '../../chrome-debug-schemes.js';
import type { ChromeDebugParams } from '../../chrome-debug-schemes.js';
import { getRandomAgentName } from '../../agentNames.js';
import type { QueryRecord } from '../query-envelope.js';
import { WorkerCapabilitySelectionSchema } from '../../contracts/capabilities.js';
import { configureWorkerCapabilities, getParentCapabilitySnapshot, getParentWorkerCapabilities, inspectWorkerCapabilityGrants } from '../worker-capabilities.js';
import { buildWorkerHandoff } from './packets.js';

// ─── Single-agent result rendering ────────────────────────────────────────────
// Exported so the inbox and agent inspect action can render
// the same view without duplicating the layout logic.

export function renderSingleAgentResult(record: AgentRecord, header: string, opts: { full?: boolean } = {}): ToolCallResult {
  const output = record.lastOutput || record.stderr || record.error || '';
  const summary = summarizeAgent(record, opts);
  const elapsed = formatElapsed(record.startedAt, isTerminal(record) ? record.updatedAt : undefined);
  const statusParts = [
    `status: ${record.status}`,
    record.exitCode !== undefined ? `exit: ${record.exitCode}` : '',
    `elapsed: ${elapsed}`,
    record.error ? `error: ${record.error}` : '',
  ].filter(Boolean).join(' \u00b7 ');
  const contentParts: string[] = [
    `${header} [${record.name}]`,
    `agentId: ${record.id}`,
    statusParts,
  ];
  const grant = record.capabilityGrant;
  if (grant) contentParts.push(`capabilities: grant revision ${grant.revision}, snapshot ${grant.snapshotRevision}; native: ${grant.nativeTools.join(', ') || 'none'}; skills: ${grant.skills.join(', ') || 'none'}; MCP: ${grant.mcpTools.map(item => `${item.server}/${item.tool}`).join(', ') || 'none'}`);
  const toolSummary = formatToolCalls(record.toolCalls, opts.full ? record.toolCalls.length : 3);
  if (toolSummary) contentParts.push(`tools: ${toolSummary}`);
  if (summary.policyWarnings?.length) contentParts.push(`policy: ${summary.policyWarnings.join(' | ')}`);
  if (summary.normalizedResult?.status && summary.normalizedResult.status !== 'unknown') {
    contentParts.push(`handback: ${summary.normalizedResult.status} \u00b7 confidence: ${summary.normalizedResult.confidence}`);
    if (summary.normalizedResult.result) contentParts.push(`result: ${summary.normalizedResult.result}`);
    if (summary.normalizedResult.evidence.length > 0) {
      const evidenceLimit = opts.full ? summary.normalizedResult.evidence.length : 3;
      contentParts.push(`evidence: ${summary.normalizedResult.evidence.slice(0, evidenceLimit).join('; ')}`);
    }
    if (summary.normalizedResult.verification) contentParts.push(`verification: ${summary.normalizedResult.verification}`);
    if (summary.normalizedResult.artifact) contentParts.push(`artifact: ${summary.normalizedResult.artifact}`);
    if (summary.normalizedResult.next) contentParts.push(`next: ${summary.normalizedResult.next}`);
  }
  const assignedHandback = summary.handback;
  contentParts.push(`handback file: ${assignedHandback.path}${assignedHandback.exists ? ` (${assignedHandback.bytes ?? 0} bytes)` : ' (not written yet)'}`);
  if (summary.recoveryRisk?.warnings.length) {
    contentParts.push(`recovery-risk: ${summary.recoveryRisk.warnings.join(' | ')}`);
  }
  if (summary.worktree) {
    contentParts.push(`worktree: ${summary.worktree.branch} @ ${summary.worktree.path} (+${summary.worktree.aheadCommits} commits, ~${summary.worktree.dirtyFiles} files, ${summary.worktree.mergeState})`);
  }
  if (output) contentParts.push('', output);
  return {
    content: [{ type: 'text', text: contentParts.join('\n') }],
    details: {
      agent: summary,
      ...(grant ? { capabilityGrant: grant } : {}),
    },
    isError: record.status === 'failed' || Boolean(record.error),
  };
}

// ─── Programmatic worker seams ─────────────────────────────────────────────────
// Thin exported wrappers over the exact code paths the agent tool and the
// /octocode-inbox command verbs use, so other features can steer/kill/inspect
// workers without going through the tool surface.

/**
 * Steer a live worker by id or prefix. Running workers get the steer RPC
 * (redirects the in-flight turn, same as agent type:"steer"); idle
 * workers have no turn to redirect, so the message is queued via the follow_up
 * path (same as agent type:"message", delivery:"followUp"). Returns false for unknown ids,
 * dead processes, or empty messages.
 */
export function steerWorkerById(idOrPrefix: string, message: string): boolean {
  const record = findAgentByIdOrPrefix(idOrPrefix);
  const text = String(message ?? '').trim();
  if (!record || !text || !isProcessAlive(record)) return false;
  if (record.status === 'running') {
    touch(record, 'running');
    const sent = sendRpc(record, { type: 'steer', message: text });
    if (sent) recordMessageActivity(record, 'to-agent', 'steer', text, `steer sent: ${previewMessage(text)}`);
    return sent;
  }
  const queued = sendRpc(record, { type: 'follow_up', message: text });
  if (queued) {
    enqueueWorkerTurn(record);
    recordMessageActivity(record, 'to-agent', 'follow-up', text, `follow-up queued: ${previewMessage(text)}`);
  }
  return queued;
}

/** Full retained worker status/output for the interactive inspector. */
export function getWorkerTranscript(idOrPrefix: string): string | undefined {
  const record = findAgentByIdOrPrefix(idOrPrefix);
  if (!record) return undefined;
  return (renderSingleAgentResult(record, 'Agent status', { full: true }).content[0] as { text?: string } | undefined)?.text ?? '';
}

/** Execute the public agent lifecycle operations against the worker registry. */
export async function executeAgentLifecycle(
  params: Record<string, unknown>,
  signal?: AbortSignal,
  ctx?: PiContext,
): Promise<ToolCallResult> {
  const action = String(params['type'] ?? '');
  if (!['inspect', 'configure', 'wait', 'message', 'steer', 'abort', 'kill'].includes(action)) throw new Error(`Unknown agent lifecycle operation: ${action}`);
  signal?.throwIfAborted();
  const renderOpts = { full: params['full'] === true };
  if (action === 'inspect' && !params['agentId']) {
    refreshAgentLedgerUi(ctx);
    const revision = getParentCapabilitySnapshot()?.revision;
    return renderAgentResult([...agents.values()], `Spawned agents${revision ? `\nParent capability snapshotRevision: ${revision}` : ''}`);
  }
  const record = getAgent(params['agentId']);
  if (action === 'configure') {
    if (!isProcessAlive(record)) throw new Error('Cannot configure an exited worker; spawn a fresh worker.');
    const snapshotRevision = String(params['snapshotRevision'] ?? '').trim();
    if (!snapshotRevision) throw new Error('agent configure requires the current parent snapshotRevision.');
    const selection = WorkerCapabilitySelectionSchema.parse(params['capabilities']);
    record.capabilityGrant = configureWorkerCapabilities(record.id, {
      snapshotRevision,
      selection,
      expectedGrantRevision: typeof params['grantRevision'] === 'number' ? params['grantRevision'] : undefined,
    });
    touch(record);
    return renderSingleAgentResult(record, 'Worker capabilities configured; removals apply now, additions before the next turn', renderOpts);
  }
  if (action === 'inspect') {
    record.capabilityGrant = inspectWorkerCapabilityGrants().find(item => item.grant.workerId === record.id)?.pendingGrant ?? getParentWorkerCapabilities(record.id)?.grant ?? record.capabilityGrant;
    refreshAgentLedgerUi(ctx);
    return renderSingleAgentResult(record, 'Agent status', renderOpts);
  }

  if (action === 'wait') {
    setManagedStatus(ctx, 'agent-wait', `\u29D7 Waiting for \u201C${record.name}\u201D\u2026`);
    // timeoutMs is the silence budget, not a rigid deadline: an actively
    // streaming worker keeps the wait alive indefinitely. On a genuine quiet
    // gap we probe liveness and return a truthful snapshot instead of erroring.
    let outcome: WaitOutcome;
    try {
      outcome = await waitForAgent(record, { maxSilenceMs: Number(params['timeoutMs'] ?? 300000), signal });
    } finally {
      setManagedStatus(ctx, 'agent-wait', undefined);
    }
    const header = outcome.reason === 'terminal'
      ? 'Agent turn completed'
      : outcome.probedAlive
        ? 'Agent still working (alive, no output during the wait window \u2014 call wait again to keep collecting)'
        : 'Agent unresponsive (no output and liveness probe unanswered \u2014 inspect with status or kill)';
    const waitResult = renderSingleAgentResult(record, header, renderOpts);
    if (params['remove'] === true) {
      // An idle (non-terminal) worker's process is still alive; deleting the
      // record would orphan it beyond the reach of shutdown cleanup.
      if (!isDroppable(record)) killAgent(record, { forceKillDelayMs: 0 });
      agents.delete(record.id);
    }
    refreshAgentLedgerUi(ctx);
    return waitResult;
  }

  if (action === 'kill') {
    killAgent(record);
    const result = renderSingleAgentResult(record, 'Agent killed', renderOpts);
    if (params['remove'] === true) agents.delete(record.id);
    refreshAgentLedgerUi(ctx);
    return result;
  }

  if (action === 'abort') {
    if (!isTerminal(record)) {
      // Graceful interrupt: the process stays alive and finishes aborting on its
      // own, then emits agent_end which resolves any pending wait via
      // notifyWaiters. We deliberately do NOT resolve waiters here \u2014 doing so
      // would report the turn as done while the worker is still unwinding.
      sendRpc(record, { type: 'abort' });
      touch(record);
    }
    refreshAgentLedgerUi(ctx);
    return renderSingleAgentResult(record, 'Agent aborted', renderOpts);
  }

  const message = String(params['message'] ?? '').trim();
  if (!message) throw new Error(`agent type:${action} requires message.`);
  // A dead worker's stdin is destroyed \u2014 writing to it throws EPIPE and would
  // wrongly flip the record back to 'running'. Reject with a clear error instead.
  if (!isProcessAlive(record)) {
    throw new Error(
      `agent type:${action} cannot reach agent "${record.name}" \u2014 it has ${record.status} (process exited). Spawn a fresh worker.`,
    );
  }
  // sendRpc self-handles a destroyed pipe (EPIPE): it sets status 'failed' and
  // notifies waiters internally, and isProcessAlive above already rejected the
  // dead-process case, so the boolean return needs no extra handling here.
  const wasRunning = record.status === 'running';
  if (action === 'steer') {
    // steer redirects an in-flight turn; on an idle worker there is no turn to
    // redirect yet, so it enqueues a turn instead \u2014 track it as pending rather
    // than faking 'running'.
    if (wasRunning) {
      touch(record, 'running');
      if (sendRpc(record, { type: 'steer', message })) {
        recordMessageActivity(record, 'to-agent', 'steer', message, `steer sent: ${previewMessage(message)}`);
      }
    } else if (sendRpc(record, { type: 'follow_up', message })) {
      // Idle workers have no in-flight turn to redirect \u2014 a bare `steer`
      // RPC would be dropped by Pi. Route through follow_up like
      // steerWorkerById so the message actually starts the next turn.
      enqueueWorkerTurn(record);
      recordMessageActivity(record, 'to-agent', 'steer', message, `steer queued: ${previewMessage(message)}`);
    }
  } else if (params['delivery'] === 'followUp') {
    // follow_up produces a turn that has not started yet (runs after the current
    // turn, or next when idle). Track it as pending so `wait` blocks and the
    // ledger shows 'queued' until the worker actually emits agent_start.
    if (sendRpc(record, { type: 'follow_up', message })) {
      enqueueWorkerTurn(record);
      recordMessageActivity(record, 'to-agent', 'follow-up', message, `follow-up queued: ${previewMessage(message)}`);
    }
  } else {
    // Default to followUp when the worker already has an in-flight or queued turn,
    // so back-to-back sends serialize behind it rather than racing.
    const busy = wasRunning || record.pendingMessages > 0;
    const streamingBehavior = busy ? 'followUp' : undefined;
    if (sendRpc(record, {
      type: 'prompt',
      message,
      streamingBehavior,
    })) {
      // Either a queued follow-up or a fresh prompt to an idle worker: in both
      // cases the turn has not started, so mark it pending and let agent_start
      // flip the record to 'running'.
      enqueueWorkerTurn(record);
      recordMessageActivity(
        record,
        'to-agent',
        streamingBehavior === 'followUp' ? 'follow-up' : 'send',
        message,
        `${streamingBehavior === 'followUp' ? 'message queued' : 'message sent'}: ${previewMessage(message)}`,
      );
    }
  }
  refreshAgentLedgerUi(ctx);
  return renderSingleAgentResult(record, 'Agent messaged', renderOpts);
}

// ─── Spawn dispatch ────────────────────────────────────────────────────────────

export async function executeSpawnQuery(
  query: QueryRecord,
  ctx?: PiContext,
  signal?: AbortSignal,
): Promise<ToolCallResult> {
  signal?.throwIfAborted();
  const profile = query['profile'] as AgentProfile | undefined;
  if (!profile) throw new Error('agent spawn requires an explicit profile.');
  const { packet, task } = buildWorkerHandoff(query);
  const name = query['name'] as string | undefined;
  const model = query['model'] as string | undefined;
  const provider = query['provider'] as string | undefined;
  const thinking = query['thinking'] as string | undefined;
  const cwd = query['cwd'] as string | undefined;
  const isolation = query['isolation'] as SpawnAgentParams['isolation'];
  const includeUncommitted = query['includeUncommitted'] as boolean | undefined;
  const requestedPlanStep = (query['planStep'] as string | undefined)?.trim() || undefined;
  const cohortId = (query['cohortId'] as string | undefined)?.trim() || undefined;

  const planStep = requestedPlanStep;
  const assignment = resolvePlanWorkerAssignment(planStep, task, ctx);
  const fullTask = assignment.task;

  let spawnParams: SpawnAgentParams;

  if (profile === 'browser') {
    const port = (query['port'] as number | undefined) ?? 9222;
    const url = query['url'] as string | undefined;
    const launch = query['launch'] === true;
    const headless = query['headless'] !== false;
    const runNow = query['runNow'] !== false;
    const durationMs = (query['durationMs'] as number | undefined) ?? 5000;
    const workspaceCwd = query['workspaceCwd'] as string | undefined;
    const { schemes, cdpDomains } = routeTask(packet.goal);
    const initialFindings: string[] = [];

    if (runNow) {
      let connection: Awaited<ReturnType<typeof connectToChrome>> | null = null;
      try {
        connection = await connectToChrome({ port, launch, headless, workspaceCwd });
        signal?.throwIfAborted();
        const { session } = connection;
        if (url) {
          await session.send('Page.enable', {});
          await session.send('Page.navigate', { url });
          await new Promise<void>((resolve) => {
            const timer = setTimeout(resolve, 4000);
            (session as unknown as { on(event: string, listener: () => void): void }).on(
              'Page.loadEventFired',
              () => { clearTimeout(timer); resolve(); },
            );
          });
          initialFindings.push(`[AGENT] navigated to ${url}`);
        }
        for (const scheme of schemes.slice(0, 4)) {
          signal?.throwIfAborted();
          const entry = SCHEME_REGISTRY[scheme as keyof typeof SCHEME_REGISTRY];
          if (!entry) continue;
          try {
            const result = await entry.recipe({
              session,
              params: {
                scheme: scheme as ChromeDebugParams['scheme'],
                url: undefined,
                durationMs,
                port,
              },
              screenshotDir: connection.screenshotDir,
              signal: signal ? AbortSignal.any([signal, AbortSignal.timeout(20_000)]) : AbortSignal.timeout(20_000),
            });
            if (result.evidenceLines.length > 0) {
              initialFindings.push(`\n[AGENT] === ${scheme.toUpperCase()} ===`);
              initialFindings.push(...result.evidenceLines);
            }
          } catch (error) {
            signal?.throwIfAborted();
            initialFindings.push(`[AGENT] ${scheme} error: ${(error as Error).message.slice(0, 100)}`);
          }
        }
      } catch (error) {
        signal?.throwIfAborted();
        initialFindings.push(`[AGENT] connect error: ${(error as Error).message}`);
      } finally {
        if (connection) {
          await cleanupConnection(connection.session, !launch, launch).catch(() => {});
        }
      }
    }

    const spawnConfig = buildSpawnConfig({
      task: fullTask,
      url,
      port,
      model,
      cdpDomains,
      skillContext: '',
      initialFindings,
    });
    spawnParams = {
      task: spawnConfig.task,
      name: name ?? `Browser Agent · ${getRandomAgentName()}`,
      cwd,
      tools: [...new Set([...spawnConfig.tools, 'MCPTool', 'skill', 'awareness', 'bash'])],
      skills: resolveSubagentSkills(
        SUBAGENT_REGISTRY['browser-agent'],
        cwd ?? ctx?.cwd ?? process.cwd(),
      ),
      systemPrompt: spawnConfig.systemPrompt,
      resourceMode: 'octocode',
      thinking: thinking ?? 'low',
      model: model ?? spawnConfig.model,
      provider: provider ?? ctx?.model?.provider,
      noSession: query['noSession'] !== false,
      isolation,
      includeUncommitted,
    };
  } else if (profile === 'custom') {
    // Custom workers retain the shared bounded-worker authority; callers supply
    // only the specialized role delta and least-capability tool set.
    const tools = query['tools'] as string[] | undefined;
    const rolePrompt = String(query['systemPrompt'] ?? '').trim();
    if (!rolePrompt) throw new Error('custom profile requires a non-empty systemPrompt describing the bounded role.');
    const systemPrompt = `${SUBAGENT_WORKER_CONTRACT}\n\n## Custom role\n\n${rolePrompt}`;
    const resourceMode =
      (query['resourceMode'] as SpawnAgentParams['resourceMode']) ?? 'octocode';
    const workerCwd = cwd ?? ctx?.cwd ?? process.cwd();
    spawnParams = {
      task: fullTask,
      name: name ?? `Worker · ${getRandomAgentName()}`,
      cwd,
      tools: tools ?? (resourceMode === 'lean' ? [] : ['MCPTool', 'skill', 'awareness', 'bash']),
      skills: resourceMode === 'octocode'
        ? resolveSubagentSkills({}, workerCwd)
        : undefined,
      systemPrompt,
      resourceMode,
      thinking,
      model,
      provider: provider ?? ctx?.model?.provider,
      noSession: query['noSession'] !== false,
      isolation,
      includeUncommitted,
    };
  } else {
    // Typed registry profile (researcher / planner / architect / implementer).
    const subagentName = PROFILE_TO_SUBAGENT[profile];
    const config = SUBAGENT_REGISTRY[subagentName] as SubagentConfig & {
      model?: string;
      provider?: string;
    };
    const systemPrompt = loadSystemPrompt(config);
    const skills = resolveSubagentSkills(config, cwd ?? ctx?.cwd ?? process.cwd());
    spawnParams = {
      task: fullTask,
      name: name ?? `${config.label} · ${getRandomAgentName()}`,
      cwd,
      tools: [...config.tools],
      skills,
      systemPrompt,
      resourceMode: config.resourceMode,
      thinking: thinking ?? config.thinking,
      model: model ?? config.model,
      provider: provider ?? config.provider ?? ctx?.model?.provider,
      noSession: query['noSession'] !== false,
      isolation,
      includeUncommitted,
    };
  }

  spawnParams.planStep = planStep;
  spawnParams.planId = assignment.planId;
  spawnParams.planScope = assignment.planScope;
  spawnParams.cohortId = cohortId;
  spawnParams.capabilityProfile = profile;
  spawnParams.capabilities = query['capabilities'] === undefined ? undefined : WorkerCapabilitySelectionSchema.parse(query['capabilities']);
  spawnParams.capabilitySnapshotRevision = typeof query['snapshotRevision'] === 'string' ? query['snapshotRevision'] : undefined;

  signal?.throwIfAborted();
  const approvedParams = await prepareSpawnAgentParams(spawnParams, ctx);
  signal?.throwIfAborted();
  signal?.throwIfAborted();
  const record = spawnRpcAgent(approvedParams, ctx);
  refreshAgentLedgerUi(ctx);

  const agentId: string = record.id;
  const ledgerEntry = listWorkerLedgerEntries().find((entry) => entry.agentId === agentId);
  const policyLines = record.policyWarnings.length > 0
    ? ['', '[POLICY]', ...record.policyWarnings.map((w: string) => `  ${w}`)] : [];

  const output = [
    `[SPAWNED] profile:${profile} \u00b7 agentId:${agentId}`,
    `[SPAWNED] name: ${record.name}`,
    `[SPAWNED] model: ${ledgerEntry?.provider ? `${ledgerEntry.provider}/` : ''}${ledgerEntry?.model ?? 'inherited'}`,
    `[SPAWNED] task: ${ledgerEntry?.task ?? task}`,
    ...(ledgerEntry?.planStep ? [`[SPAWNED] plan: ${ledgerEntry.planStep}`] : []),
    ...policyLines,
    '',
    `[USAGE] agent({queries:[{type:"wait", agentId:"${agentId}"}]})`,
  ].join('\n');

  return {
    content: [{ type: 'text', text: output }],
    details: { agentId, profile, name: record.name, model: ledgerEntry?.model, provider: ledgerEntry?.provider, task: ledgerEntry?.task ?? task, planStep: ledgerEntry?.planStep, cohortId: record.cohortId, capabilityGrant: record.capabilityGrant },
  } as unknown as ToolCallResult;
}
