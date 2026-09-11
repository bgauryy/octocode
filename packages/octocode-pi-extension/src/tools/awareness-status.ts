import { truncateToWidth } from '../tui/width.js';
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
} from '@octocodeai/octocode-awareness';
import { isPersistentStorageEnabledForExtension as isPersistentStorageEnabled } from '@octocodeai/config';
import type { PiContext, PiTheme } from '../types.js';

import { SEP_WIDE, paint } from '../tui/palette.js';
import { renderInlineRows, type InlineSegment } from '../tui/components.js';

import { capMapSize } from '../utils.js';

/** True when there is any shared state worth showing a panel for. */
export function hasAwarenessSignal(s: ExternalAwarenessStatus): boolean {
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
 * Build explicit Awareness detail lines. Empty array when there is
 * nothing to show; lines clipped at the source when `width` is given.
 */
export function formatAwarenessPanel(
  s: ExternalAwarenessStatus,
  theme?: PiTheme,
  width?: number
): string[] {
  if (!hasAwarenessSignal(s) && !(s.unreadInbox && s.unreadInbox > 0))
    return [];
  const debt = s.verifyTasks;
  const segs: string[] = [];
  if (s.activePlans > 0) segs.push(`plans ${s.activePlans}`);
  if (s.readyTasks > 0) segs.push(`ready ${s.readyTasks}`);
  if (s.inProgressTasks > 0) segs.push(`doing ${s.inProgressTasks}`);

  const tail: string[] = [];
  if (s.lockCount > 0) tail.push(`locks ${s.lockCount}`);
  if (s.workCount > 0) tail.push(`work ${s.workCount}`);
  if (s.messageCount > 0) {
    tail.push(
      s.lastMessage
        ? `peer-msgs ${s.messageCount} (last ${s.lastMessage.from}→${s.lastMessage.to}: ${s.lastMessage.preview})`
        : `peer-msgs ${s.messageCount}`
    );
  }

  const attention: InlineSegment[] = [];
  // Unread inbound messages lead the panel — they are the one awareness event
  // that demands the operator's/agent's attention (a peer is talking to YOU).
  if (s.unreadInbox && s.unreadInbox > 0) {
    const preview = s.lastInbound
      ? ` (from ${s.lastInbound.from}: ${s.lastInbound.preview})`
      : '';
    attention.push({
      text: `✉ ${s.unreadInbox} unread${preview}`,
      token: 'warning',
      attention: true,
    });
  }
  if (debt > 0)
    attention.push({
      text: `verify-debt ${debt}`,
      token: 'warning',
      attention: true,
    });
  const chunks: InlineSegment[] = [
    ...attention,
    ...(segs.length
      ? [{ text: segs.join(SEP_WIDE), token: 'brand' as const }]
      : []),
    ...(tail.length
      ? [{ text: tail.join(SEP_WIDE), token: 'muted' as const }]
      : []),
  ];
  if (chunks.length === 0 && !s.taskActivities?.length) return [];
  const summaryLines = width
    ? renderInlineRows(
        {
          segments: [{ text: 'Awareness', token: 'title' }, ...chunks],
          separator: SEP_WIDE,
        },
        { width, theme }
      )
    : [
        chunks.length > 0
          ? `${paint(theme, 'title', 'Awareness')}  ${chunks.map(chunk => paint(theme, chunk.token ?? 'dim', chunk.text)).join(SEP_WIDE)}`
          : paint(theme, 'title', 'Awareness'),
      ];
  const taskLines = (s.taskActivities ?? []).map(task => {
    const state = paint(
      theme,
      task.state === 'doing' ? 'brand' : 'link',
      task.state.toUpperCase()
    );
    const owner = task.agentId
      ? `${SEP_WIDE}${paint(theme, 'muted', task.agentId)}`
      : '';
    const id = paint(theme, 'dim', task.taskId.slice(0, 6));
    return `${paint(theme, 'dim', '  task')}${SEP_WIDE}${state}${SEP_WIDE}${task.title}${owner}${SEP_WIDE}${id}`;
  });
  const lines = [...summaryLines, ...taskLines];
  return width ? lines.map(line => truncateToWidth(line, width)) : lines;
}

// ─── Async, throttled refresh ────────────────────────────────────────────────

const MIN_REFRESH_MS = 8000;
/** Max distinct workspaces retained in the status cache before LRU eviction. */
const MAX_CACHED_CWDS = 32;

/** Return the last cached Awareness status for command dashboards. */
export type CachedAwarenessStatus = ExternalAwarenessStatus & { observedAt: number };

export type AwarenessStatusHealth =
  | { state: 'idle' | 'fresh' | 'disabled' }
  | { state: 'unavailable'; message: string };

export function getCachedAwarenessStatus(
  cwd: string
): CachedAwarenessStatus | null {
  return cache.get(cwd)?.status ?? null;
}

/** Distinguish a disabled source from a failed read without discarding known data. */
export function getAwarenessStatusHealth(cwd: string): AwarenessStatusHealth {
  return cache.get(cwd)?.health ?? { state: 'idle' };
}

interface CacheEntry {
  status: CachedAwarenessStatus | null;
  health: AwarenessStatusHealth;
  lastRunAt: number;
  running: boolean;
  generation: number;
}
const cache = new Map<string, CacheEntry>();
const generations = new Map<string, number>();

/** Typed package reader; injectable without serializing through CLI JSON. */
export type StatusRunner = (
  cwd: string,
  agentId?: string
) => Promise<ExternalAwarenessStatus | null>;
const defaultRunner: StatusRunner = async (cwd, agentId) => {
  if (!isPersistentStorageEnabled()) return null;
  return readExternalAwarenessStatus({ workspace: cwd, agentId });
};
let runner: StatusRunner = defaultRunner;

export function setAwarenessStatusRunnerForTests(fn: StatusRunner): void {
  runner = fn;
}
export function resetAwarenessStatusStateForTests(): void {
  runner = defaultRunner;
  cache.clear();
  generations.clear();
}

/**
 * Drop a workspace's cached status so the next refresh re-polls from scratch. Called on
 * session_start so a new session in a cwd visited earlier this process does not paint the
 * previous session's stale task/lock counts on its first frame.
 */
export function clearAwarenessCacheEntry(cwd: string): void {
  cache.delete(cwd);
  generations.set(cwd, (generations.get(cwd) ?? 0) + 1);
}
export function forceAwarenessStatusRefreshForTests(cwd: string): void {
  const entry = cache.get(cwd);
  if (entry) entry.lastRunAt = 0;
}

let awarenessMetricsRefresh: ((ctx?: PiContext) => void) | undefined;

/** Register the host footer refresher without coupling this data source to UI layout. */
export function setAwarenessMetricsRefreshForUi(
  cb: ((ctx?: PiContext) => void) | undefined
): void {
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
  const generation = generations.get(cwd) ?? 0;
  const entry = cache.get(cwd) ?? {
    status: null,
    health: { state: 'idle' } as AwarenessStatusHealth,
    lastRunAt: 0,
    running: false,
    generation,
  };
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
    .then(status => {
      if (
        panelSuppressed ||
        cache.get(cwd) !== entry ||
        entry.generation !== (generations.get(cwd) ?? 0)
      )
        return;
      entry.running = false;
      if (status === null) {
        entry.status = null;
        entry.health = { state: 'disabled' };
        repaintFooter(ctx);
        return;
      }
      entry.status = { ...status, observedAt: Date.now() };
      entry.health = { state: 'fresh' };
      repaintFooter(ctx);
    })
    .catch(error => {
      if (
        panelSuppressed ||
        cache.get(cwd) !== entry ||
        entry.generation !== (generations.get(cwd) ?? 0)
      )
        return;
      entry.running = false;
      entry.health = {
        state: 'unavailable',
        message: error instanceof Error ? error.message : String(error),
      };
      repaintFooter(ctx);
    });
}
