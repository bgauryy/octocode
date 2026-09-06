/**
 * agent-inbox — a 2-stage worker inbox overlay + desktop notifications for
 * spawned Octocode workers.
 *
 * Required index.ts wiring (this module never edits index.ts itself):
 *
 *   import { registerAgentInbox } from './tools/agent-inbox.js';
 *   // after registerUnifiedAgentTool(...), inside activate():
 *   const agentInbox = registerAgentInbox(pi, notify);
 *   // in the session_shutdown hook (before/alongside cleanupSpawnedAgentsForShutdown):
 *   agentInbox.shutdown();
 *
 * The extension lifecycle owns the single session_shutdown hook and MUST call
 * shutdown() before cleanupSpawnedAgentsForShutdown(). A second hook here would
 * receive Pi's already-invalid replacement context and duplicate teardown.
 *
 * Surfaces:
 *  - '/octocode-inbox' command → stage 1 pick a worker (select overlay), stage 2
 *    pick an action: view transcript / steer / kill / dismiss.
 *  - Worker ledger listener → OSC 9 desktop notification + terminal-title flash
 *    + one-line ctx.ui.notify when a worker finishes/fails while no turn is
 *    active, or when a worker that ran > 30s completes (even mid-turn).
 */

import type { PiContext, PiInstance, WorkerLedgerEntry, WorkerLedgerEventType, NotifyFn } from '../types.js';
import type { SelectOverlayItem, SelectOverlayOptions } from './ui-overlays.js';
import { truncatePlainToWidth } from './render-helpers.js';
import { shortId } from './ids.js';
import {
  formatElapsed,
  registerWorkerLedgerListener,
} from './agent-tools.js';
import {
  clearTitleFlashTimer,
  desktopNotificationsSuppressed,
  emitOsc9,
  flashTerminalTitle,
  notificationsEnabled,
  suppressDesktopNotifications,
  resumeDesktopNotifications,
} from './desktop-notify.js';

export const OCTOCODE_INBOX_COMMAND = 'octocode-inbox';
/** Completed workers that ran longer than this always notify, even mid-turn. */
export const LONG_RUN_NOTIFY_MS = 30_000;
const TRANSCRIPT_MAX_LINES = 40;
const SUMMARY_MAX_CHARS = 90;

// ─── Inbox items (pure) ───────────────────────────────────────────────────────

type InboxDisplayState = 'starting' | 'running' | 'idle' | 'done' | 'blocked' | 'failed' | 'killed';

const STATE_GLYPHS: Record<InboxDisplayState, string> = {
  starting: '○', // ○
  running: '⟳', //  ⟳
  idle: '◎', //     ◎
  done: '✓', //     ✓
  blocked: '!',
  failed: '✗', //   ✗
  // The inbox is COLORLESS (glyph + spelled-out state word, no paint), so killed
  // keeps its own ⊘ rather than reusing failed's ✗ — a distinct glyph is the only
  // visual signal here. (⊘ also means 'blocked' in the plan surfaces, but that is a
  // separate, colored domain, so the reuse doesn't confuse in practice.)
  killed: '⊘', //   ⊘
};

/** Mirror of agent-tools' display-state derivation (that helper is not exported). */
export function inboxDisplayState(entry: Pick<WorkerLedgerEntry, 'status' | 'normalizedStatus'>): InboxDisplayState {
  if (entry.status === 'killed') return 'killed';
  if (entry.status === 'failed' || entry.normalizedStatus === 'failed') return 'failed';
  if (entry.status === 'running') return 'running';
  // Exited beats blocked (mirrors agent-tools): a dead [BLOCKED] worker is not
  // actionable, so the inbox must not offer it as steerable.
  if (entry.status === 'exited') return 'done';
  if (entry.normalizedStatus === 'blocked') return 'blocked';
  if (entry.normalizedStatus === 'done') return 'done';
  if (entry.status === 'idle') return 'idle';
  return 'starting';
}

function isTerminalState(state: InboxDisplayState): boolean {
  return state === 'done' || state === 'failed' || state === 'killed';
}

/** A live worker's process still accepts steer/followUp/kill RPCs. */
function isLiveEntry(entry: Pick<WorkerLedgerEntry, 'status'>): boolean {
  return entry.status === 'starting' || entry.status === 'running' || entry.status === 'idle';
}

function oneLine(text: string, maxChars = SUMMARY_MAX_CHARS): string {
  const flat = text.replace(/\s+/g, ' ').trim();
  // Cell-width aware: emoji/CJK summaries must not be sliced mid-surrogate.
  return truncatePlainToWidth(flat, maxChars);
}

/** Last-result summary line: live delta note, else handback result/next, else the latest ledger message. */
export function inboxSummaryLine(entry: WorkerLedgerEntry): string {
  const live = isLiveEntry(entry) ? entry.deltaSummary : undefined;
  const lastEvent = entry.recentEvents.at(-1)?.message;
  const text = live ?? entry.result ?? entry.next ?? lastEvent ?? '';
  return oneLine(String(text));
}

/**
 * Build the stage-1 select items — one per worker: status glyph, name, id
 * prefix, age, and a last-result summary as the description. Pure: pass `now`
 * for deterministic ages.
 */
export function buildInboxItems(entries: WorkerLedgerEntry[], now: number = Date.now()): SelectOverlayItem[] {
  return entries.map((entry) => {
    const state = inboxDisplayState(entry);
    const startedAt = Date.parse(entry.startedAt);
    const endedAt = isTerminalState(state) ? Date.parse(entry.updatedAt) : now;
    const age = Number.isFinite(startedAt) ? formatElapsed(startedAt, endedAt) : '?';
    const summary = inboxSummaryLine(entry);
    return {
      value: entry.agentId,
      label: `${STATE_GLYPHS[state]} ${entry.name} (${shortId(entry.agentId)}) · ${state} · ${age}`,
      description: summary || undefined,
    };
  });
}

/** Stage-2 action items for one worker. Steer/kill only offered while the process is live. */
export function buildInboxActionItems(entry: Pick<WorkerLedgerEntry, 'status'>): SelectOverlayItem[] {
  const items: SelectOverlayItem[] = [
    { value: 'view', label: 'View transcript', description: 'Show the worker’s current state and latest output' },
  ];
  if (isLiveEntry(entry)) {
    items.push(
      { value: 'steer', label: 'Steer', description: 'Send a mid-flight course correction to this worker' },
      { value: 'kill', label: 'Kill', description: 'Stop this worker now (SIGTERM, then SIGKILL)' },
    );
  }
  items.push({ value: 'dismiss', label: 'Dismiss', description: 'Close the inbox' });
  return items;
}

// ─── Overlay flow (deps-injected, testable) ───────────────────────────────────

export interface AgentInboxDeps {
  ctx: PiContext | undefined;
  listEntries(): WorkerLedgerEntry[];
  runOverlay(ctx: PiContext | undefined, opts: SelectOverlayOptions): Promise<string | null | undefined>;
  steer(idOrPrefix: string, message: string): boolean;
  kill(idOrPrefix: string): boolean;
  transcript(idOrPrefix: string, opts?: { maxLines?: number }): string | undefined;
  notify: NotifyFn;
  now?(): number;
}

/**
 * Run the 2-stage inbox: stage 1 picks a worker, stage 2 picks an action.
 * Every external effect flows through `deps` so tests drive it with fakes.
 */
export async function runAgentInboxOverlay(deps: AgentInboxDeps): Promise<void> {
  const { ctx, notify } = deps;
  const entries = deps.listEntries();
  if (entries.length === 0) {
    notify(ctx, 'Octocode inbox: no spawned workers this session. Use agent with type:"spawn" to delegate work.', 'info');
    return;
  }

  const agentId = await deps.runOverlay(ctx, {
    title: 'Octocode agent inbox',
    items: buildInboxItems(entries, deps.now?.() ?? Date.now()),
  });
  if (!agentId) return; // cancelled or no UI

  const entry = entries.find((e) => e.agentId === agentId);
  if (!entry) return;

  const action = await deps.runOverlay(ctx, {
    title: `${entry.name} (${shortId(entry.agentId)})`,
    items: buildInboxActionItems(entry),
    filter: false,
  });
  if (!action || action === 'dismiss') return;

  if (action === 'view') {
    const text = deps.transcript(entry.agentId, { maxLines: TRANSCRIPT_MAX_LINES });
    notify(ctx, text ?? `No transcript for worker ${entry.name}.`, 'info');
    return;
  }

  if (action === 'steer') {
    // Prefer the one-line input dialog; fall back to the editor for hosts without input().
    const title = `Steer ${entry.name}`;
    const message = typeof ctx?.ui?.input === 'function'
      ? await ctx.ui.input(title, 'course correction for this worker…')
      : await ctx?.ui?.editor?.(title, '');
    const text = String(message ?? '').trim();
    if (!text) {
      notify(ctx, `Steer cancelled for ${entry.name}.`, 'info');
      return;
    }
    const ok = deps.steer(entry.agentId, text);
    notify(
      ctx,
      ok
        ? `Steer sent to ${entry.name} (${shortId(entry.agentId)}).`
        : `Could not steer ${entry.name} — the worker process is no longer accepting messages.`,
      ok ? 'info' : 'warning',
    );
    return;
  }

  if (action === 'kill') {
    const ok = deps.kill(entry.agentId);
    notify(
      ctx,
      ok ? `Killed worker ${entry.name} (${shortId(entry.agentId)}).` : `No worker found for ${shortId(entry.agentId)}.`,
      ok ? 'warning' : 'error',
    );
  }
}

// ─── Completion notifications (pure decision + registration) ─────────────────

export interface NotifyDecisionOptions {
  turnActive: boolean;
  suppressed: boolean;
  alreadyNotified: boolean;
  now?: number;
}

/**
 * Whether a ledger event warrants a desktop notification.
 *
 * Rules:
 *  - NEVER after shutdown suppress — killed/exit events from teardown are ignored entirely.
 *  - 'killed' never notifies (it is always operator-initiated).
 *  - Only completion events count: 'exit', or 'error' once the worker is in status 'failed'
 *    (mid-run 'error' ledger events fire for recoverable turn errors while the worker keeps going).
 *  - At most one notification per worker (alreadyNotified).
 *  - Fire when no turn is active (the operator is idle at the prompt), OR when the worker
 *    ran longer than LONG_RUN_NOTIFY_MS — a long-running worker completing is worth a ping
 *    even while the parent agent is mid-turn.
 */
export function shouldNotifyWorkerEvent(
  entry: Pick<WorkerLedgerEntry, 'status' | 'startedAt'>,
  type: WorkerLedgerEventType,
  opts: NotifyDecisionOptions,
): boolean {
  if (opts.suppressed) return false;
  if (opts.alreadyNotified) return false;
  // Enforce the documented "'killed' never notifies" rule by STATUS, not just
  // event type: killAgent sets status 'killed' but the process close handler
  // still emits a type:'exit' ledger event, which would otherwise flash a
  // misleading "finished" notification for a worker the operator just killed.
  if (entry.status === 'killed') return false;
  const failedError = type === 'error' && entry.status === 'failed';
  if (type !== 'exit' && !failedError) return false;
  if (!opts.turnActive) return true;
  const startedAt = Date.parse(entry.startedAt);
  const ranMs = (opts.now ?? Date.now()) - startedAt;
  return Number.isFinite(startedAt) && ranMs > LONG_RUN_NOTIFY_MS;
}

/** Injectable seams so tests can drive registerAgentInbox without real workers/terminals. */
export interface AgentInboxSeams {
  registerListener?: typeof registerWorkerLedgerListener;
  listEntries?: () => WorkerLedgerEntry[];
  runOverlay?: AgentInboxDeps['runOverlay'];
  steer?: AgentInboxDeps['steer'];
  kill?: AgentInboxDeps['kill'];
  transcript?: AgentInboxDeps['transcript'];
  emitOsc9?: (message: string) => void;
  flashTitle?: (ctx: PiContext | undefined, text: string) => void;
  notificationsEnabled?: () => boolean;
  now?: () => number;
}

export interface AgentInboxRegistration {
  /** Detach the ledger listener (idempotent). */
  unsubscribe(): void;
  /** Full shutdown: set the suppress flag FIRST, then detach. Call before killing workers. */
  shutdown(options?: { restoreTitle?: boolean }): void;
  /**
   * Undo shutdown()'s suppression + detach so a following session can notify
   * again (idempotent). Must be called on session_start — the registration is
   * once-per-process, so without this a single shutdown kills notifications
   * permanently. Mirrors resumeStatusPanel/resumeAwarenessPanel.
   */
  resume(): void;
}

/**
 * Register the '/octocode-inbox' command and the worker-completion desktop
 * notifications. Returns the unsubscribe/shutdown handle used at session_shutdown.
 */
export function registerAgentInbox(
  pi: PiInstance,
  notify?: NotifyFn,
  seams: AgentInboxSeams = {},
): AgentInboxRegistration {
  const notifier: NotifyFn = notify ?? ((ctx, message, level) => { ctx?.ui?.notify?.(message, level); });
  const registerListener = seams.registerListener ?? registerWorkerLedgerListener;
  const osc9 = seams.emitOsc9 ?? emitOsc9;
  const flashTitle = seams.flashTitle ?? ((ctx: PiContext | undefined, text: string) => flashTerminalTitle(ctx, text));
  const enabled = seams.notificationsEnabled ?? notificationsEnabled;
  const now = seams.now ?? Date.now;

  // Registration-local state (closures, not module globals, so each session/test is isolated).
  let localSuppressed = false;
  let turnActive = false;
  let lastCtx: PiContext | undefined;
  const notifiedAgents = new Set<string>();

  const onLedgerEvent = (entry: WorkerLedgerEntry, type: WorkerLedgerEventType): void => {
    // Shutdown-ordering race: once the suppress flag is set, teardown emits a burst of
    // killed/exit events for every live worker — ignore them ENTIRELY, don't just mute the OSC.
    if (localSuppressed || desktopNotificationsSuppressed()) return;
    const decision = shouldNotifyWorkerEvent(entry, type, {
      turnActive,
      suppressed: false,
      alreadyNotified: notifiedAgents.has(entry.agentId),
      now: now(),
    });
    if (!decision) return;
    notifiedAgents.add(entry.agentId);

    const failed = entry.status === 'failed' || entry.normalizedStatus === 'failed';
    const startedAt = Date.parse(entry.startedAt);
    const elapsed = Number.isFinite(startedAt) ? ` · ${formatElapsed(startedAt, now())}` : '';
    const verdict = failed ? 'failed' : 'finished';
    const message = `Octocode worker ${entry.name} ${verdict}${elapsed}`;
    if (enabled()) {
      osc9(message);
      flashTitle(lastCtx, message);
    }
    const summary = inboxSummaryLine(entry);
    notifier(lastCtx, summary ? `${message} — ${summary}` : message, failed ? 'warning' : 'info');
  };

  let unsubscribeLedger = registerListener(onLedgerEvent);
  let detached = false;
  const unsubscribe = (): void => {
    if (detached) return;
    detached = true;
    unsubscribeLedger();
  };
  const shutdown = (options: { restoreTitle?: boolean } = {}): void => {
    // Order matters: suppress BEFORE detaching so any event already in flight is ignored,
    // and BEFORE workers are killed so teardown killed/exit events never notify.
    localSuppressed = true;
    suppressDesktopNotifications(options);
    clearTitleFlashTimer();
    unsubscribe();
  };
  const resume = (): void => {
    // session_start counterpart to shutdown(): clear both suppress flags and
    // re-attach the ledger listener that shutdown() detached. Without this a
    // single /new or /resume leaves the once-per-process registration muted +
    // detached forever, so no later worker ever notifies. Idempotent: on the
    // first session (never shut down) this is a harmless re-arm.
    localSuppressed = false;
    resumeDesktopNotifications();
    if (detached) {
      unsubscribeLedger = registerListener(onLedgerEvent);
      detached = false;
    }
  };

  // Track whether a turn is active + capture the freshest ctx for notifications.
  pi.on('agent_start', async (_event, ctx) => { turnActive = true; lastCtx = ctx ?? lastCtx; });
  pi.on('agent_end', async (_event, ctx) => { turnActive = false; lastCtx = ctx ?? lastCtx; });
  pi.on('session_start', async (_event, ctx) => { lastCtx = ctx ?? lastCtx; });

  return { unsubscribe, shutdown, resume };
}
