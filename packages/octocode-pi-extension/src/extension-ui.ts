import { getPermissionLevel } from './tools/approval.js';
import {
  getAwarenessStatusHealth,
  getCachedAwarenessStatus,
} from './tools/awareness-status.js';
import { listVisibleWorkerLedgerEntries } from './tools/agents/ledger.js';
import { recordSessionTitle } from './tools/desktop-notify.js';
import { getActiveDialLevel } from './tools/effort-dial.js';
import { makeComponentRenderer } from './tools/render-helpers.js';
import {
  runtimeStoreFor,
  setManagedFooter,
  setManagedStatus,
  setManagedWorkingIndicator,
} from './tools/runtime-renderer.js';
import {
  expireExecutionInteractions,
  syncExecutionEntities,
} from './tools/execution-runtime.js';
import type {
  RuntimeFooterState,
  RuntimeState,
} from './tools/runtime-store.js';
import { renderFooterView } from './tui/footer-view.js';
import { paint } from './tui/palette.js';
import type { PiContext, PiInstance, PiTheme } from './types.js';
import {
  buildCapabilitySegments,
  buildWorkingIndicator,
  deriveSessionName,
  formatBranchSegment,
  formatDurationShort,
  elapsedSince,
  getFooterDensity,
} from './ui-extras.js';
import { activePlanScope } from './tools/planning/plan-store.js';
import {
  getCurrentPlanReadModel,
  type PlanReadModelV1,
} from './tools/plan-read-model.js';
import type { InlineSegment } from './tui/components.js';
import { deriveUxSnapshot } from './tools/ux-snapshot.js';
import { buildFooterDiagnostics } from './tools/footer-diagnostics.js';
import {
  selectStatusRows,
  type StatusDensity,
} from './tui/status-policy.js';

/**
 * The `octocode-thinking` status text. Pi already renders `model: <id>` in the
 * same status row, so this never repeats the model id — it only carries the
 * thinking level (or its absence).
 */
export function getThinkingStatus(
  ctx: PiContext | undefined,
  level?: string
): string {
  const model = ctx?.model;
  // Return empty string when the model doesn't support reasoning or no level is set;
  // the chip is hidden when empty, so it never shows 'thinking' permanently at idle.
  if (!model?.reasoning) return '';
  return level ?? '';
}

// Footer registration is idempotent per runtime store. Pi's documented
// contract (docs/tui.md "Custom Footer": setFooter ONCE + tui.requestRender for
// live updates) — the previous code re-called setFooter on every 1s ticker and
// every agent-ledger event during a turn, which churned the whole footer
// component and leaked a new onBranchChange subscription per call. That churn
// showed up as message-area flicker and scroll jumps mid-turn. Now the factory
// reads live state at render time; updateOctocodeMetricsUi only asks Pi to
// repaint. Keyed by store so fresh callback contexts share one footer and a new
// session re-registers; session_start deletes the entry so the current session
// always re-registers with its own tui/theme.
const footerRegisteredStores = new WeakSet<object>();
const footerRequestRenderByStore = new WeakMap<object, () => void>();
interface FooterFacts {
  plan: PlanReadModelV1;
  workers: ReturnType<typeof listVisibleWorkerLedgerEntries>;
  awareness: ReturnType<typeof getCachedAwarenessStatus>;
  awarenessHealth: ReturnType<typeof getAwarenessStatusHealth>;
  permissionLevel: ReturnType<typeof getPermissionLevel>;
  dial: ReturnType<typeof getActiveDialLevel>;
  density: StatusDensity;
}
const footerFactsByStore = new WeakMap<object, FooterFacts>();

function statusDensity(): StatusDensity {
  const density = getFooterDensity();
  return density === 'full'
    ? 'expanded'
    : density === 'default'
      ? 'automatic'
      : 'compact';
}

function viewportHeight(tui: unknown): number {
  const candidate = tui as
    | {
        height?: number;
        rows?: number;
        terminal?: { rows?: number };
        getSize?: () => { height?: number; rows?: number };
      }
    | undefined;
  const size = candidate?.getSize?.();
  const height =
    size?.height ??
    size?.rows ??
    candidate?.height ??
    candidate?.rows ??
    candidate?.terminal?.rows;
  return Number.isFinite(height) && (height ?? 0) > 0
    ? Math.floor(height!)
    : 40;
}

function buildOctocodeFooterLines(
  ctx: PiContext,
  state: RuntimeFooterState,
  width: number,
  height: number,
  theme: PiTheme,
  footerData: { getGitBranch?: () => string | null | undefined } | undefined
): string[] {
  const now = Date.now();
  const store = runtimeStoreFor(ctx);
  if (!store) return [];
  const runtimeState = store.getState();
  const facts = footerFactsByStore.get(store);
  if (!facts) return [];
  const { workers, plan, awareness: cachedAwareness } = facts;
  const currentTask =
    plan.tasks.find(task => task.status === 'doing') ??
    plan.tasks.find(task => task.status === 'todo') ??
    plan.tasks.find(task => task.status === 'blocked');
  const snapshot = deriveUxSnapshot({
    now,
    runtime: runtimeState,
    plan,
    agents: workers,
    goal: currentTask
      ? {
          text: currentTask.text,
          ...(currentTask.activeText
            ? { milestone: currentTask.activeText }
            : {}),
          nextAction:
            currentTask.status === 'blocked'
              ? 'Inspect plan'
              : 'Continue current task',
        }
      : {},
    ...(cachedAwareness
      ? {
          awareness: {
            unread: cachedAwareness.unreadInbox ?? 0,
            observedAt: cachedAwareness.observedAt,
            staleAfterMs: 16_000,
            latestSender: cachedAwareness.lastInbound?.from,
            latestSubject: cachedAwareness.lastInbound?.preview,
          },
        }
      : {}),
  });

  const branch = footerData?.getGitBranch?.();
  const execution = runtimeState.execution;
  const permissionLevel = facts.permissionLevel;
  const identity: InlineSegment[] = [
    ...(permissionLevel === 'relaxed'
      ? [{ text: 'perm relaxed', token: 'warning' as const, attention: true }]
      : []),
    { text: ctx.model?.id ?? 'model unavailable', token: 'muted' },
    {
      text: `${state.activeTurnStartedAt !== undefined ? 'turn' : 'session'} ${formatDurationShort(elapsedSince(state.activeTurnStartedAt ?? state.sessionStartedAt, now))}`,
      token: 'dim',
    },
    { text: `tools ${execution.toolCount}`, token: 'dim' },
    ...(branch
      ? [
          {
            text: formatBranchSegment(
              branch,
              state.gitDirty ?? false,
              state.gitDirtyFiles
            ),
            token: 'dim' as const,
          },
        ]
      : []),
    ...(state.gitAdditions !== undefined &&
    state.gitDeletions !== undefined &&
    (state.gitAdditions > 0 || state.gitDeletions > 0)
      ? [
          {
            text: `tree +${state.gitAdditions} -${state.gitDeletions}`,
            token: 'path' as const,
          },
        ]
      : []),
    ...(permissionLevel && permissionLevel !== 'relaxed'
      ? [{ text: `perm ${permissionLevel}`, token: 'dim' as const }]
      : []),
    { text: '/configuration', token: 'link' },
  ];

  const metrics = buildCapabilitySegments({
    dial: facts.dial,
    overhead:
      runtimeState.context.status === 'pending'
        ? undefined
        : {
            totalChars: runtimeState.context.providerSubtotalChars,
            sysChars: runtimeState.context.systemPromptChars,
            mcpServers: runtimeState.context.mcpServers,
            mcpTools: runtimeState.context.mcpTools,
            skills: runtimeState.context.skills,
          },
  });
  const diagnostics = buildFooterDiagnostics({
    identity,
    metrics,
    statuses: runtimeState.statuses,
    githubStatus: state.githubAuth.status,
    awareness: cachedAwareness,
    awarenessHealth: facts.awarenessHealth,
  });
  const selected = selectStatusRows(snapshot, {
    width,
    height,
    density: facts.density,
    diagnostics,
  });
  return renderFooterView({ rows: selected.rows }, { width, theme });
}
export function updateOctocodeMetricsUi(
  ctx: PiContext | undefined,
  _now = Date.now()
): void {
  if (!ctx) return;
  const store = runtimeStoreFor(ctx);
  if (!store) return;
  const plan = getCurrentPlanReadModel(ctx, activePlanScope(ctx));
  const workers = listVisibleWorkerLedgerEntries();
  footerFactsByStore.set(store, {
    plan,
    workers,
    awareness: getCachedAwarenessStatus(ctx.cwd ?? process.cwd()),
    awarenessHealth: getAwarenessStatusHealth(ctx.cwd ?? process.cwd()),
    permissionLevel: getPermissionLevel(ctx),
    dial: getActiveDialLevel(),
    density: statusDensity(),
  });
  syncExecutionEntities(ctx, plan, workers);
  expireExecutionInteractions(ctx, _now);
  if (!ctx.hasUI) return;
  // Sample once per update (event or 1s tick); the render closure only reads.
  try {
    const usage = ctx.getContextUsage?.();
    if (usage)
      store.getState().setFooter({
        usage: {
          tokens: usage.tokens ?? undefined,
          contextWindow: usage.contextWindow ?? 0,
        },
      });
  } catch {
    /* keep the last sample */
  }

  // The consolidated branded footer is the SINGLE metrics surface — context /
  // tokens / plan / task / agents / git. No second persistent state panel exists.
  if (!footerRegisteredStores.has(store)) {
    footerRegisteredStores.add(store);
    setManagedFooter(ctx, (tui: unknown, theme, footerData) => {
      footerRequestRenderByStore.set(store, () =>
        (tui as { requestRender?: () => void } | undefined)?.requestRender?.()
      );
      const renderer = makeComponentRenderer(
        (_props, { width }) =>
          buildOctocodeFooterLines(
            ctx,
            store.getState().footer,
            width,
            viewportHeight(tui),
            theme,
            footerData
          ),
        undefined
      );
      const repaint = () => {
        renderer.invalidate();
        footerRequestRenderByStore.get(store)?.();
      };
      const unsubscribeBranch = footerData?.onBranchChange?.(repaint);
      // Smart subscription: only repaint when slices the footer actually reads
      // have changed. Skips irrelevant mutations (tasks, mcp, phase, stage,
      // notice, generation) that never affect buildOctocodeFooterLines output.
      const unsubscribeRuntime = store.subscribe(
        (state: RuntimeState, prevState: RuntimeState) => {
          if (
            state.activity !== prevState.activity ||
            state.footer !== prevState.footer ||
            state.statuses !== prevState.statuses ||
            state.context !== prevState.context ||
            state.execution !== prevState.execution
          ) {
            repaint();
          }
        }
      );
      return {
        ...renderer,
        dispose: () => {
          unsubscribeBranch?.();
          unsubscribeRuntime();
        },
      };
    });
  }
  // Live update: repaint the already-registered footer with fresh state instead
  // of re-registering it (which is what caused the flicker).
  footerRequestRenderByStore.get(store)?.();
}

export function resetOctocodeFooterRegistration(
  ctx: PiContext | undefined
): void {
  const store = runtimeStoreFor(ctx);
  if (store) {
    footerRegisteredStores.delete(store);
    footerFactsByStore.delete(store);
    footerRequestRenderByStore.delete(store);
  }
}

export async function execGitSummary(
  pi: PiInstance,
  args: string[],
  timeout = 1200
): Promise<string> {
  if (!pi.exec) return '';
  try {
    const result = await pi.exec('git', args, { timeout });
    if (result.code !== 0) return '';
    return result.stdout.trim();
  } catch {
    return '';
  }
}

/**
 * Refresh the footer's dirty marker on turn/session boundaries. Pi's footerData
 * provider owns branch detection/watching, so this keeps our extra `*` marker
 * without duplicating branch probes.
 */
export async function refreshFooterDirtyState(
  pi: PiInstance,
  ctx: PiContext | undefined
): Promise<void> {
  const [porcelain, numstat] = await Promise.all([
    execGitSummary(pi, ['status', '--porcelain'], 600),
    execGitSummary(pi, ['diff', 'HEAD', '--numstat'], 600),
  ]);
  const totals = numstat.split('\n').reduce(
    (sum, line) => {
      const [added, removed] = line.split('\t');
      if (/^\d+$/.test(added ?? '')) sum.additions += Number(added);
      if (/^\d+$/.test(removed ?? '')) sum.deletions += Number(removed);
      return sum;
    },
    { additions: 0, deletions: 0 }
  );
  runtimeStoreFor(ctx)
    ?.getState()
    .setFooter({
      gitDirty: porcelain !== '',
      gitAdditions: totals.additions,
      gitDeletions: totals.deletions,
      gitDirtyFiles:
        porcelain === ''
          ? 0
          : porcelain.split('\n').filter(line => line.trim()).length,
    });
}

/** CustomEntry type for the fresh-session banner card. */
export const OCTOCODE_BANNER_ENTRY_TYPE = 'octocode-banner';

/**
 * Pi creates a fresh context for each event. Retain the applied indicator by
 * runtime identity so model/input events do not restart it, while theme changes
 * and replacement runtimes still receive their current frames.
 */
const workingIndicatorByRuntime = new WeakMap<object, string>();

export function applyOctocodeUi(
  ctx: PiContext | undefined,
  level?: string,
  contextTitle?: string
): void {
  // setStatus / setHiddenThinkingLabel are TUI-only; guard with hasUI.
  if (!ctx?.hasUI) return;
  const ui = ctx.ui;
  if (!ui) return;
  const title = deriveSessionName(contextTitle ?? '');
  const windowTitle = title ? `Octocode · ${title}` : 'Octocode';
  // The session name lives in the TERMINAL title only. It used to also be a
  // pi header line (`◆ <title>`) at the very top of the TUI content — any change
  // to line 0 is "above the viewport" for pi-tui's differential renderer, which
  // then full-redraws and CLEARS SCROLLBACK (tui-main-screen.js: firstChanged <
  // viewportTop → fullRender(true)). With the name re-derived on every prompt,
  // that wiped the scrollback on every message. The transcript banner card
  // already carries the brand; nothing Octocode-owned renders above the chat.
  ui.setTitle?.(windowTitle);
  // Title flashes (desktop-notify) restore to the live harness title, not a constant.
  recordSessionTitle(windowTitle);
  const label = paint(ui.theme, 'brand', '◆ Octocode');
  setManagedStatus(ctx, 'octocode', label);
  // Thinking-level chip: only show the level string (e.g. 'medium') when the model
  // supports reasoning. Empty → chip is hidden. The chip is hidden while a turn
  // is active (turn_start hook), then restores here on every level/model change.
  const thinkingStatus = getThinkingStatus(ctx, level);
  setManagedStatus(
    ctx,
    'octocode-thinking',
    thinkingStatus ? paint(ui.theme, 'dim', thinkingStatus) : undefined
  );
  const indicator = buildWorkingIndicator(ui.theme);
  const indicatorKey = JSON.stringify(indicator);
  const runtimeKey = runtimeStoreFor(ctx) ?? ctx.sessionManager ?? ctx;
  if (workingIndicatorByRuntime.get(runtimeKey) !== indicatorKey) {
    workingIndicatorByRuntime.set(runtimeKey, indicatorKey);
    ui.setHiddenThinkingLabel?.('Octocode thinking');
    // Glyph-only indicator + branded message: Pi renders these side-by-side,
    // so keeping "Octocode" out of the frames avoids "Octocode Octocode …".
    setManagedWorkingIndicator(ctx, indicator);
    // Visibility is derived from runtime phase/activity. The footer owns its
    // lifecycle text, so the working row contains only this animated glyph.
  }
}
