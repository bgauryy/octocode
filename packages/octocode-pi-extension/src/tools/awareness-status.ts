/**
 * awareness-status — a compact shared Awareness projection
 * (plans, tasks, verify debt, locks, manual work presence, and messages).
 *
 * Awareness is canonical SQLite behind the `octocode-awareness` bin shipped by
 * `@octocodeai/octocode-awareness`. Its
 * state previously only surfaced in chat when the agent ran a CLI command; this
 * module projects attention into the footer while commands retain full detail.
 *
 * Design:
 *   - `formatAwarenessPanel` is pure + unit-tested.
 *   - `refreshAwarenessPanel` reads the package API ASYNC + THROTTLED (never blocks a
 *     turn), caches the last result per workspace, and renders the widget.
 *   - Any failure degrades silently (no panel, never throws) — Awareness being
 *     unavailable must never break the agent.
 */

import {
  readExternalAwarenessStatus,
  type ExternalAwarenessStatus,
  type ExternalAwarenessTaskActivity,
} from '@octocodeai/octocode-awareness';
import type { PiContext, PiTheme } from '../types.js';
import { paint } from '../tui/cli-design.js';
import { SEP_WIDE } from '../tui/palette.js';
import { renderInlineRows, type InlineSegment } from '../tui/components.js';
import { truncateToWidth } from './render-helpers.js';
import { capMapSize } from '../utils.js';

export type AwarenessTaskActivity = ExternalAwarenessTaskActivity;
export type AwarenessStatus = ExternalAwarenessStatus;

/** True when there is any shared state worth showing a panel for. */
export function hasAwarenessSignal(s: AwarenessStatus): boolean {
  return (
    s.activePlans > 0 ||
    s.readyTasks > 0 ||
    s.inProgressTasks > 0 ||
    s.verifyTasks > 0 ||
    s.lockCount > 0 ||
    s.workCount > 0 ||
    s.agentCount > 0 ||
    s.messageCount > 0 ||
    (s.taskActivities?.length ?? 0) > 0
  );
}

/**
 * Build the compact `<awareness_signal>` text block for the unread peer-message count.
 *
 * NOT injected into the frozen system prompt — the count varies between sessions and
 * busts the provider prompt cache (~30k tokens re-billed per miss). The static
 * `<awareness>` section in SYSTEM_PROMPT.md already instructs the model to check
 * inbox when peer coordination may affect the next action; the TUI panel surfaces
 * the live count visually via `formatAwarenessPanel`.
 *
 * Kept as an exported utility in case a future non-frozen injection surface is added.
 */
export function renderAwarenessSignalAddendum(
  s: AwarenessStatus | null,
  _currentAgentId?: string,
): string {
  const unread = s?.unreadInbox ?? 0;
  if (unread === 0) return '';
  return [
    '<awareness_signal>',
    `Unread direct peer messages: ${unread}.`,
    'Use message inbox only when the peer input can change the current action. Message bodies are not injected here. Do not perform status polling or start/finish ceremony.',
    '</awareness_signal>',
  ].join('\n');
}

/**
 * Build explicit Awareness detail lines. Empty array when there is
 * nothing to show; lines clipped at the source when `width` is given.
 */
export function formatAwarenessPanel(s: AwarenessStatus, theme?: PiTheme, width?: number): string[] {
  if (!hasAwarenessSignal(s) && !(s.unreadInbox && s.unreadInbox > 0)) return [];
  const debt = s.verifyTasks;
  const segs: string[] = [];
  if (s.activePlans > 0) segs.push(`plans ${s.activePlans}`);
  if (s.readyTasks > 0) segs.push(`ready ${s.readyTasks}`);
  if (s.inProgressTasks > 0) segs.push(`doing ${s.inProgressTasks}`);

  const tail: string[] = [];
  if (s.lockCount > 0) tail.push(`locks ${s.lockCount}`);
  if (s.workCount > 0) tail.push(`work ${s.workCount}`);
  if (s.messageCount > 0) {
    tail.push(s.lastMessage
      ? `peer-msgs ${s.messageCount} (last ${s.lastMessage.from}→${s.lastMessage.to}: ${s.lastMessage.preview})`
      : `peer-msgs ${s.messageCount}`);
  }

  const attention: InlineSegment[] = [];
  // Unread inbound messages lead the panel — they are the one awareness event
  // that demands the operator's/agent's attention (a peer is talking to YOU).
  if (s.unreadInbox && s.unreadInbox > 0) {
    const preview = s.lastInbound ? ` (from ${s.lastInbound.from}: ${s.lastInbound.preview})` : '';
    attention.push({ text: `✉ ${s.unreadInbox} unread${preview}`, token: 'warning', attention: true });
  }
  if (debt > 0) attention.push({ text: `verify-debt ${debt}`, token: 'warning', attention: true });
  const chunks: InlineSegment[] = [
    ...attention,
    ...(segs.length ? [{ text: segs.join(SEP_WIDE), token: 'brand' as const }] : []),
    ...(tail.length ? [{ text: tail.join(SEP_WIDE), token: 'muted' as const }] : []),
  ];
  if (chunks.length === 0 && !(s.taskActivities?.length)) return [];
  const summaryLines = width
    ? renderInlineRows({ segments: [{ text: 'Awareness', token: 'title' }, ...chunks], separator: SEP_WIDE }, { width, theme })
    : [chunks.length > 0
      ? `${paint(theme, 'title', 'Awareness')}  ${chunks.map((chunk) => paint(theme, chunk.token ?? 'dim', chunk.text)).join(SEP_WIDE)}`
      : paint(theme, 'title', 'Awareness')];
  const taskLines = (s.taskActivities ?? []).map((task) => {
    const state = paint(theme, task.state === 'doing' ? 'brand' : 'link', task.state.toUpperCase());
    const owner = task.agentId ? `${SEP_WIDE}${paint(theme, 'muted', task.agentId)}` : '';
    const id = paint(theme, 'dim', task.taskId.slice(0, 6));
    return `${paint(theme, 'dim', '  task')}${SEP_WIDE}${state}${SEP_WIDE}${task.title}${owner}${SEP_WIDE}${id}`;
  });
  const lines = [...summaryLines, ...taskLines];
  return width ? lines.map((line) => truncateToWidth(line, width)) : lines;
}

// ─── Async, throttled refresh ────────────────────────────────────────────────

const MIN_REFRESH_MS = 8000;
/** Max distinct workspaces retained in the status cache before LRU eviction. */
const MAX_CACHED_CWDS = 32;

/** The Awareness section lines for the unified panel, from the cached status (empty when none). */
export function awarenessPanelLines(cwd: string, theme?: PiTheme, width?: number): string[] {
  const status = cache.get(cwd)?.status;
  return status ? formatAwarenessPanel(status, theme, width) : [];
}

/** Whether the cached Awareness status has anything worth showing for this workspace. */
export function hasCachedAwarenessSignal(cwd: string): boolean {
  const status = cache.get(cwd)?.status;
  return status ? hasAwarenessSignal(status) : false;
}

/** Return the last cached Awareness status for command dashboards. */
export function getCachedAwarenessStatus(cwd: string): AwarenessStatus | null {
  return cache.get(cwd)?.status ?? null;
}

interface CacheEntry {
  status: AwarenessStatus | null;
  lastRunAt: number;
  running: boolean;
}
const cache = new Map<string, CacheEntry>();

/** Typed package reader; injectable without serializing through CLI JSON. */
export type StatusRunner = (cwd: string, agentId?: string) => Promise<AwarenessStatus | null>;
const defaultRunner: StatusRunner = async (cwd, agentId) => {
  try {
    return readExternalAwarenessStatus({ workspace: cwd, agentId });
  } catch {
    return null;
  }
};
let runner: StatusRunner = defaultRunner;

export function setAwarenessStatusRunnerForTests(fn: StatusRunner): void {
  runner = fn;
}
export function resetAwarenessStatusStateForTests(): void {
  runner = defaultRunner;
  cache.clear();
}

/**
 * Drop a workspace's cached status so the next refresh re-polls from scratch. Called on
 * session_start so a new session in a cwd visited earlier this process does not paint the
 * previous session's stale task/lock counts on its first frame.
 */
export function clearAwarenessCacheEntry(cwd: string): void {
  cache.delete(cwd);
}
export function forceAwarenessStatusRefreshForTests(cwd: string): void {
  const entry = cache.get(cwd);
  if (entry) entry.lastRunAt = 0;
}

let awarenessMetricsRefresh: ((ctx?: PiContext) => void) | undefined;

/** Register the host footer refresher without coupling this data source to UI layout. */
export function setAwarenessMetricsRefreshForUi(cb: ((ctx?: PiContext) => void) | undefined): void {
  awarenessMetricsRefresh = cb;
}

function repaintFooter(ctx: PiContext): void {
  try {
    awarenessMetricsRefresh?.(ctx);
  } catch {
    // Awareness remains authoritative even if a stale UI context rejects repaint.
  }
}

/**
 * Refresh the Awareness projection: throttled + async. Renders the cached status
 * immediately (if any) and kicks off a background refresh at most every
 * MIN_REFRESH_MS. Never blocks the turn; never throws.
 */
// Set during session_shutdown so an async refresh from the replaced session
// cannot repaint its stale context.
let panelSuppressed = false;
export function suppressAwarenessPanel(): void {
  panelSuppressed = true;
}
export function resumeAwarenessPanel(): void {
  panelSuppressed = false;
}

export function refreshAwarenessPanel(ctx?: PiContext): void {
  if (!ctx?.hasUI || panelSuppressed) return;
  const cwd = ctx.cwd ?? process.cwd();
  const entry = cache.get(cwd) ?? { status: null, lastRunAt: 0, running: false };
  // delete-then-set keeps this cwd most-recently-used; cap so a long-lived process
  // visiting many workspaces cannot grow the cache without bound.
  cache.delete(cwd);
  cache.set(cwd, entry);
  capMapSize(cache, MAX_CACHED_CWDS);

  // Repaint immediately from the cached snapshot, then again if async state changes.
  repaintFooter(ctx);

  const now = Date.now();
  if (entry.running || now - entry.lastRunAt < MIN_REFRESH_MS) return;
  entry.running = true;
  entry.lastRunAt = now;
  void runner(cwd, process.env.OCTOCODE_AGENT_ID)
    .then((status) => {
      entry.running = false;
      if (status === null) {
        entry.status = null;
        repaintFooter(ctx);
        return;
      }
      entry.status = status;
      repaintFooter(ctx);
    })
    .catch(() => {
      entry.running = false;
      entry.status = null;
      repaintFooter(ctx);
    });
}
