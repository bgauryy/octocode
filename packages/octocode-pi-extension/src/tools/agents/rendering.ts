import { effectiveAgentStatus, isRecentAgentOutcome } from './display-state.js';
import { summarizeAgentCohorts } from './cohorts.js';
/**
 * rendering.ts — TUI rendering layer for the agent ledger.
 *
 * Owns: AgentDetails/getAgentDisplayState through refreshAgentLedgerUi,
 * including ticker/spinner/summarize/render/format functions.
 *
 * No imports from agent-tools (keeps dependency direction clean: process ← rendering ← agent-tools).
 * Calls wireProcessCallbacks once at module init so spawnRpcAgent's event handlers
 * can invoke refreshAgentLedgerUi without a circular import.
 */
import { OCTOCODE_SPINNER_FRAMES } from '../../ui-extras.js';
import { hasUiTickSubscriber, setUiTickSubscriber } from '../../tui/ui-ticker.js';
import { shortId } from '../ids.js';
import { inspectWorkerAwarenessAutomatically } from '../awareness-worker-audit.js';
import { truncateUserVisibleToolOutput } from '../../utils.js';
import type {
  PiContext,
  ToolCallResult,
  PiTheme,
} from '../../types.js';
import { paint } from '../../tui/palette.js';
import {
  type AgentRecord,
  type AgentToolCall,
  type AgentDisplayState,
  type AgentDisplaySource,
} from './types.js';
import {
  agents,
  getLedgerHidden,
  isTerminal,
} from './registry.js';
import { getArgCsv, getArgValue, statHandbackArtifact, worktreeSnapshot } from './worker-metadata.js';
import {
  getActiveAgentUi,
  wireProcessCallbacks,
} from './process.js';

// ─── AgentDetails ─────────────────────────────────────────────────────────────

interface AgentDetails {
  agents: Array<ReturnType<typeof summarizeAgent>>;
  cohorts: ReturnType<typeof summarizeAgentCohorts>;
}

// ─── TUI rendering helpers ────────────────────────────────────────────────────

function getAgentDisplayState(agent: AgentDisplaySource): AgentDisplayState {
  return effectiveAgentStatus({
    status: agent.status,
    normalizedStatus: agent.normalizedResult?.status,
    pendingMessages: agent.pendingMessages,
  });
}

// Live-progress spinner: advanced once per ledger tick while a worker runs.
// Shares the working-indicator frames so the extension has ONE spinner glyph
// set (the ledger just steps it at the panel's 1s cadence).
const LEDGER_SPINNER = OCTOCODE_SPINNER_FRAMES;
let ledgerSpinnerFrame = 0;
/** Shared-clock subscription key; live only while ≥1 worker is non-terminal and the UI is present. */
const LEDGER_TICK_KEY = 'octocode-ledger';
let agentLedgerMetricsRefresh: ((ctx?: PiContext) => void) | undefined;

function stopLedgerTicker(): void {
  setUiTickSubscriber(LEDGER_TICK_KEY, undefined);
}

/** Test hook: stop the ticker and report its state so tests never leak a real timer. */
export function stopLedgerTickerForTests(): void {
  stopLedgerTicker();
}
export function isLedgerTickerActiveForTests(): boolean {
  return hasUiTickSubscriber(LEDGER_TICK_KEY);
}

function agentDisplayMeta(state: AgentDisplayState, theme?: PiTheme, opts: { frozen?: boolean } = {}): { icon: string; label: string } {
  const raw: { icon: string; label: string; color: Parameters<typeof paint>[1] } = (() => {
    switch (state) {
      case 'done': return { icon: '✓', label: 'done', color: 'success' };
      case 'failed': return { icon: '✗', label: 'failed', color: 'error' };
      // killed is a neutral terminal state (dismissed), not act-on-me — gold is
      // reserved for blocked (the row you must act on). Muted so the two differ.
      case 'killed': return { icon: '✗', label: 'killed', color: 'muted' };
      case 'blocked': return { icon: '!', label: 'blocked', color: 'warning' };
      // Running is normal activity (brand), idle is quiet (muted) — warning and
      // success stay reserved for real attention/outcome states.
      case 'running': return { icon: opts.frozen ? LEDGER_SPINNER[0]! : LEDGER_SPINNER[ledgerSpinnerFrame % LEDGER_SPINNER.length], label: 'running', color: 'brand' };
      case 'idle': return { icon: '◎', label: 'idle', color: 'muted' };
      case 'queued': return { icon: '⇥', label: 'queued', color: 'link' };
      case 'starting': return { icon: '○', label: 'starting', color: 'dim' };
    }
  })();
  return {
    icon: paint(theme, raw.color, raw.icon),
    label: paint(theme, raw.color, raw.label),
  };
}

/**
 * `endedAt` freezes elapsed time at a terminal agent's last update instead of
 * letting it keep growing against Date.now() long after the agent finished —
 * pass it whenever the record/summary is terminal (see isTerminal()).
 */
export function formatElapsed(startedAt: number, endedAt?: number): string {
  const ms = (endedAt ?? Date.now()) - startedAt;
  if (ms < 1000) return `${ms}ms`;
  if (ms < 60_000) return `${Math.round(ms / 1000)}s`;
  const m = Math.floor(ms / 60_000);
  const s = Math.round((ms % 60_000) / 1000);
  return s > 0 ? `${m}m${s}s` : `${m}m`;
}

export function formatToolCalls(toolCalls: AgentToolCall[], limit = 3): string {
  const recent = toolCalls.slice(-limit);
  return recent.map((call) => `${call.toolName}:${call.status}`).join(', ');
}

export function summarizeAgent(record: AgentRecord, opts: { full?: boolean } = {}) {
  const normalized = record.normalizedResult;
  const summaryText = normalized?.result || normalized?.next || record.lastOutput || record.stderr || record.error || '';
  const preview = truncateUserVisibleToolOutput(summaryText, 1000);
  return {
    agentId: record.id,
    name: record.name,
    status: record.status,
    awarenessAgentId: record.awarenessAgentId,
    awarenessInspection: record.awarenessInspection,
    cwd: record.cwd,
    model: getArgValue(record.args, '--model'),
    provider: getArgValue(record.args, '--provider'),
    task: record.task,
    planStep: record.planStep,
    cohortId: record.cohortId,
    thinking: getArgValue(record.args, '--thinking'),
    tools: record.capabilityGrant?.nativeTools ?? getArgCsv(record.args, '--tools'),
    capabilityGrant: record.capabilityGrant,
    startedAt: new Date(record.startedAt).toISOString(),
    updatedAt: new Date(record.updatedAt).toISOString(),
    exitCode: record.exitCode,
    signal: record.signal,
    error: record.error,
    lastOutput: preview.text,
    outputTruncated: preview.truncated,
    normalizedResult: normalized,
    handback: statHandbackArtifact(record.handbackPath),
    recoveryRisk: record.recoveryRisk,
    pendingMessages: record.pendingMessages,
    lastMessage: record.lastMessage,
    policyWarnings: [...record.policyWarnings],
    ledgerEvents: opts.full ? [...record.ledgerEvents] : record.ledgerEvents.slice(-10),
    toolCalls: opts.full ? [...record.toolCalls] : record.toolCalls.slice(-10),
    activeTool: [...record.toolCalls].reverse().find((call) => call.status === 'running')?.toolName,
    worktree: worktreeSnapshot(record.worktree),
  };
}

function formatAgentModelLine(summary: ReturnType<typeof summarizeAgent>): string {
  const model = summary.model ?? 'default model';
  const provider = summary.provider ? `${summary.provider}/` : '';
  const thinking = summary.thinking ? ` · think:${summary.thinking}` : '';
  const tools = summary.tools?.length ? ` · tools:${summary.tools.length}` : '';
  return `${provider}${model}${thinking}${tools}`;
}

// ─── Result rendering ─────────────────────────────────────────────────────────

export function renderAgentResult(records: AgentRecord[], header: string): ToolCallResult {
  for (const record of records) {
    if (isTerminal(record)) record.awarenessInspection = inspectWorkerAwarenessAutomatically(record);
  }
  const summaries = records.map((record) => summarizeAgent(record));
  const cohorts = summarizeAgentCohorts(records);
  const lines: string[] = [`${header} (${records.length}):`];
  for (const s of summaries) {
    const exit = s.exitCode !== undefined ? ` (exit ${s.exitCode})` : '';
    const elapsed = formatElapsed(
      new Date(s.startedAt).getTime(),
      isTerminal(s) ? new Date(s.updatedAt).getTime() : undefined,
    );
    const state = getAgentDisplayState(s);
    const meta = agentDisplayMeta(state);
    const handback = s.normalizedResult?.status && s.normalizedResult.status !== 'unknown'
      ? ` · ${s.normalizedResult.status}/${s.normalizedResult.confidence}`
      : '';
    const latestEvent = s.ledgerEvents.at(-1)?.message;
    const result = s.normalizedResult?.result ?? s.normalizedResult?.next ?? s.lastOutput ?? latestEvent;
    const preview = result ? ` — ${result.slice(0, 60).replace(/\n/g, ' ')}${s.outputTruncated ? '…' : ''}` : '';
    const toolInfo = typeof s.activeTool === 'string' ? ` · active:${s.activeTool}` : '';
    const modelInfo = ` · ${formatAgentModelLine(s)}`;
    lines.push(`  ${meta.icon} ${s.name} (${shortId(s.agentId)}) · ${meta.label}${exit}${handback}${modelInfo} · ${elapsed}${toolInfo}${preview}`);
    lines.push(`    agentId: ${s.agentId}`);
    if (s.awarenessInspection) {
      const inspection = s.awarenessInspection;
      lines.push(`    Awareness ${inspection.agentId ?? 'identity unavailable'}: ${inspection.status === 'unavailable' ? 'debt unknown' : `${inspection.pendingCount} pending checks, ${inspection.staleActiveCount} stale active`}; inspection is not verification.`);
    }
  }
  return {
    content: [{ type: 'text', text: lines.join('\n') }],
    details: { agents: summaries, cohorts } satisfies AgentDetails,
  };
}

export function setAgentLedgerMetricsRefreshForUi(cb: ((ctx?: PiContext) => void) | undefined): void {
  agentLedgerMetricsRefresh = cb;
}

function refreshAgentFooterMetrics(ctx?: PiContext): void {
  try {
    agentLedgerMetricsRefresh?.(ctx);
  } catch {
    // Footer refresh is best-effort UI work; ledger state must remain authoritative.
  }
}

/** Repaint the single persistent agent projection in the unified footer. */
export function refreshAgentLedgerUi(ctx?: PiContext): void {
  const records = [...agents.values()];
  if (records.length === 0 || getLedgerHidden()) {
    stopLedgerTicker();
    refreshAgentFooterMetrics(ctx);
    return;
  }
  if (!getActiveAgentUi(ctx)) return;
  refreshAgentFooterMetrics(ctx);
  // Live refresh: while any worker is active, advance the spinner and re-render
  // every second on the shared ui-ticker clock (one timer process-wide).
  const anyActive = records.some((r) => !isTerminal(r));
  const hasRecentOutcome = records.some(record =>
    isRecentAgentOutcome(
      {
        status: record.status,
        normalizedStatus: record.normalizedResult?.status,
        pendingMessages: record.pendingMessages,
        updatedAt: record.updatedAt,
      },
      Date.now(),
    )
  );
  if ((anyActive || hasRecentOutcome) && !hasUiTickSubscriber(LEDGER_TICK_KEY)) {
    setUiTickSubscriber(LEDGER_TICK_KEY, () => {
      ledgerSpinnerFrame = (ledgerSpinnerFrame + 1) % LEDGER_SPINNER.length;
      // Refresh once at expiry too, so a completed worker cannot stick forever
      // after the final process event.
      refreshAgentLedgerUi(ctx);
    });
  } else if (!anyActive && !hasRecentOutcome) {
    stopLedgerTicker();
  }
}

// Wire rendering callbacks into the process module so spawnRpcAgent's
// event handlers can call refreshAgentLedgerUi without a circular import.
// Function declarations are hoisted, so this is safe even though
// refreshAgentLedgerUi and stopLedgerTicker are defined above.
wireProcessCallbacks(refreshAgentLedgerUi, stopLedgerTicker);
