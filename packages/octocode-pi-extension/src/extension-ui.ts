import {
  approvedClasses,
  getPermissionLevel,
} from './tools/approval.js';
import { getCachedAwarenessStatus } from './tools/awareness-status.js';
import {
  listWorkerLedgerEntries,
} from './tools/agent-tools.js';
import { recordSessionTitle } from './tools/desktop-notify.js';
import { getActiveDialLevel } from './tools/effort-dial.js';
import { peerWipCount } from './tools/peer-wip.js';
import { makeRenderer } from './tools/render-helpers.js';
import {
  runtimeStoreFor,
  setManagedFooter,
  setManagedStatus,
  setManagedWorkingIndicator,
  setManagedWorkingMessage,
} from './tools/runtime-renderer.js';
import type { RuntimeFooterState } from './tools/runtime-store.js';
import { renderFooterView } from './tui/footer-view.js';
import { contextGauge, paint } from './tui/palette.js';
import type { PiContext, PiInstance, PiTheme } from './types.js';
import {
  buildAgentFooterRows,
  buildFooterSegments,
  buildWorkingIndicator,
  buildWorkingMessage,
  deriveSessionName,
  formatBranchSegment,
  formatCompact,
} from './ui-extras.js';

/**
 * The `octocode-thinking` status text. Pi already renders `model: <id>` in the
 * same status row, so this never repeats the model id — it only carries the
 * thinking level (or its absence).
 */
export function getThinkingStatus(ctx: PiContext | undefined, level?: string): string {
  const model = ctx?.model;
  // Return empty string when the model doesn't support reasoning or no level is set;
  // the chip is hidden when empty, so it never shows 'thinking' permanently at idle.
  if (!model?.reasoning) return '';
  return level ?? '';
}

export type OctocodeMetricsState = RuntimeFooterState;

export function formatContextUsage(ctx: PiContext | undefined): { text: string; percent?: number } {
  const usage = ctx?.getContextUsage?.();
  // One placeholder for every "not measurable yet" case — n/a-style variants read as defects.
  if (!usage || usage.contextWindow <= 0) return { text: 'ctx …' };
  // tokens is null right after compaction ("unknown", per Pi's ContextUsage).
  if (usage.tokens == null) return { text: 'ctx …' };
  // Floor keeps the displayed boundary aligned with the exact >= 80% trigger:
  // 79.5% must not claim compaction is pending before the trigger can fire.
  const percent = Math.floor((usage.tokens / usage.contextWindow) * 100);
  const { bar } = contextGauge(percent, 10);
  return {
    // formatCompact from ui-extras — the footer's formatter, so /octocode-now
    // and the toolbar abbreviate numbers identically ("45M", "1.2k").
    text: `ctx ${bar} ${percent}% (${formatCompact(usage.tokens)}/${formatCompact(usage.contextWindow)})`,
    percent,
  };
}

interface WorkerFooterCounts {
  /** All worker records still tracked in this session. */
  total: number;
  /** Live workers (starting / running / idle). */
  active: number;
  /** Workers waiting on the lead (normalized [BLOCKED]). */
  blocked: number;
  /** Workers that failed / crashed. */
  failed: number;
}

function workerFooterCounts(): WorkerFooterCounts {
  const counts: WorkerFooterCounts = { total: 0, active: 0, blocked: 0, failed: 0 };
  try {
    for (const e of listWorkerLedgerEntries()) {
      counts.total += 1;
      // Buckets are mutually exclusive: a blocked-or-done idle worker must not
      // ALSO count as "live" — the footer would overstate active work and
      // disagree with the ledger's own status display.
      const live = e.status === 'running' || e.status === 'idle' || e.status === 'starting';
      if (e.status === 'failed' || e.normalizedStatus === 'failed') counts.failed += 1;
      // "Blocked" is an attention flag for a worker the lead can still unblock;
      // an exited process that last said [BLOCKED] is not actionable.
      else if (e.normalizedStatus === 'blocked' && live) counts.blocked += 1;
      else if (live && e.normalizedStatus !== 'done') counts.active += 1;
    }
  } catch {
    return { total: 0, active: 0, blocked: 0, failed: 0 };
  }
  return counts;
}

// Footer registration is idempotent per session context. Pi's documented
// contract (docs/tui.md "Custom Footer": setFooter ONCE + tui.requestRender for
// live updates) — the previous code re-called setFooter on every 1s ticker and
// every agent-ledger event during a turn, which churned the whole footer
// component and leaked a new onBranchChange subscription per call. That churn
// showed up as message-area flicker and scroll jumps mid-turn. Now the factory
// reads live state at render time; updateOctocodeMetricsUi only asks Pi to
// repaint. Keyed by ctx (WeakMap/WeakSet) so a new session re-registers and old
// contexts are GC'd; session_start deletes the entry so the current session
// always re-registers with its own tui/theme.
const footerRegisteredCtxs = new WeakSet<object>();
const footerRequestRenderByCtx = new WeakMap<object, () => void>();

function buildOctocodeFooterLines(
  ctx: PiContext,
  state: OctocodeMetricsState,
  width: number,
  theme: PiTheme,
  footerData: { getGitBranch?: () => string | null | undefined } | undefined,
): string[] {
  // now is read at render time so the active-turn duration ticks live without
  // re-registering the footer.
  const now = Date.now();
  const usage = state.usage ?? { tokens: undefined, contextWindow: 0 };
  const workers = workerFooterCounts();
  const cachedAwareness = getCachedAwarenessStatus(ctx.cwd ?? process.cwd());
  // ── Row 1: Identity (branch · model · github · perm) + /commands ──
  // The app already owns the Octocode brand; repeating it in every footer frame
  // wastes width and creates duplicate chrome.
  const branch = footerData?.getGitBranch?.();
  const identityParts: Array<{ text: string; token?: 'dim' | 'muted' | 'success' | 'error' | 'warning' | 'link'; attention?: boolean }> = [];
  if (branch) {
    identityParts.push({ text: formatBranchSegment(branch, state.gitDirty ?? false, state.gitDirtyFiles), token: 'dim' });
  }
  if (ctx.model?.id) {
    const modelLabel = ctx.model.provider ? `${ctx.model.provider}/${ctx.model.id}` : ctx.model.id;
    identityParts.push({ text: `model ${modelLabel}`, token: 'muted' });
  }
  if (state.githubAuth.status === 'authenticated') {
    identityParts.push({ text: 'github ✓', token: 'success' });
  } else if (state.githubAuth.status === 'missing') {
    identityParts.push({ text: 'github ✗ login', token: 'error', attention: true });
  } else if (state.githubAuth.status === 'error') {
    identityParts.push({ text: 'github ✗', token: 'error', attention: true });
  } else if (state.githubAuth.status === 'checking') {
    identityParts.push({ text: 'github …', token: 'dim' });
  }
  const permLevel = getPermissionLevel();
  if (permLevel) {
    const grants = approvedClasses().length > 0 ? ` +${approvedClasses().length}` : '';
    identityParts.push({ text: `perm ${permLevel}${grants}`, token: permLevel === 'relaxed' ? 'warning' : 'dim' });
  }
  identityParts.push({ text: '/settings', token: 'link' });

  // ── Row 2: Metrics (context · session · timing · overhead · agent counts) ──
  const metricsSegments = buildFooterSegments({
    tokens: usage?.tokens ?? 0,
    contextWindow: usage?.contextWindow ?? 0,
    completedTurns: state.completedTurns,
    activeTurnMs: state.activeTurnStartedAt !== undefined ? now - state.activeTurnStartedAt : undefined,
    lastTurnMs: state.lastTurnMs,
    sessionMs: now - state.sessionStartedAt,
    activeWorkers: workers.active,
    workerTotal: workers.total,
    agentDoing: undefined,
    awarenessPeers: cachedAwareness?.agentCount ?? 0,
    awarenessUnread: cachedAwareness?.unreadInbox ?? 0,
    peerDirty: peerWipCount(),
    blockedWorkers: workers.blocked,
    failedWorkers: workers.failed,
    dial: getActiveDialLevel(),
    permissionLevel: undefined,
    approvedClassCount: undefined,
    githubAuth: undefined,
    overhead: (() => {
      const context = runtimeStoreFor(ctx)?.getState().context;
      if (!context || context.status === 'pending') return undefined;
      return {
        totalChars: context.providerSubtotalChars,
        sysChars: context.systemPromptChars,
        mcpServers: context.mcpServers,
        mcpTools: context.mcpTools,
        skills: context.skills,
      };
    })(),
    branch: undefined,
    dirty: state.gitDirty ?? false,
    dirtyFiles: state.gitDirtyFiles,
  });
  const { rows: agentRows } = buildAgentFooterRows(listWorkerLedgerEntries(), now);
  return renderFooterView({
    identity: identityParts,
    metrics: metricsSegments,
    agents: agentRows,
    shortcuts: [],
  }, { width, theme });
}

export function updateOctocodeMetricsUi(ctx: PiContext | undefined, _now = Date.now()): void {
  if (!ctx?.hasUI) return;
  const store = runtimeStoreFor(ctx);
  if (!store) return;
  // Sample once per update (event or 1s tick); the render closure only reads.
  try {
    const usage = ctx.getContextUsage?.();
    if (usage) store.getState().setFooter({ usage: { tokens: usage.tokens ?? undefined, contextWindow: usage.contextWindow ?? 0 } });
  } catch { /* keep the last sample */ }

  // The consolidated branded footer is the SINGLE metrics surface — context /
  // tokens / turns / timing / agents / git. Plan stays in the below-editor panel.
  if (!footerRegisteredCtxs.has(ctx)) {
    footerRegisteredCtxs.add(ctx);
    setManagedFooter(ctx, (tui: unknown, theme, footerData) => {
      footerRequestRenderByCtx.set(ctx, () => (tui as { requestRender?: () => void } | undefined)?.requestRender?.());
      const renderer = makeRenderer((width) => buildOctocodeFooterLines(ctx, store.getState().footer, width, theme, footerData));
      const unsubscribe = footerData?.onBranchChange?.(() => {
        renderer.invalidate();
        footerRequestRenderByCtx.get(ctx)?.();
      });
      return {
        ...renderer,
        dispose: () => unsubscribe?.(),
      };
    });
  }
  // Live update: repaint the already-registered footer with fresh state instead
  // of re-registering it (which is what caused the flicker).
  footerRequestRenderByCtx.get(ctx)?.();
}

export function resetOctocodeFooterRegistration(ctx: PiContext | undefined): void {
  if (ctx) footerRegisteredCtxs.delete(ctx);
}

const REPO_STATE_TRIGGER = /\b(repo|git|status|staged|unstaged|changes?|diff|commit|branch|dirty|modified|working tree|worktree)\b/i;

export async function execGitSummary(pi: PiInstance, args: string[], timeout = 1200): Promise<string> {
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
export async function refreshFooterDirtyState(pi: PiInstance, ctx: PiContext | undefined): Promise<void> {
  const porcelain = await execGitSummary(pi, ['status', '--porcelain'], 600);
  runtimeStoreFor(ctx)?.getState().setFooter({
    gitDirty: porcelain !== '',
    gitDirtyFiles: porcelain === '' ? 0 : porcelain.split('\n').filter((line) => line.trim()).length,
  });
}

export async function buildRepoStateHint(
  pi: PiInstance,
  event: { text: string; source?: string; streamingBehavior?: string },
): Promise<string> {
  if (event.source === 'extension') return '';
  if (event.streamingBehavior === 'steer') return '';
  if (!REPO_STATE_TRIGGER.test(event.text)) return '';
  const status = await execGitSummary(pi, ['status', '--short', '--branch']);
  if (!status) return '';
  const [lastCommit, stagedStat, unstagedStat] = await Promise.all([
    execGitSummary(pi, ['log', '-1', '--oneline', '--decorate'], 800),
    execGitSummary(pi, ['diff', '--staged', '--stat'], 800),
    execGitSummary(pi, ['diff', '--stat'], 800),
  ]);
  return [
    '<repo_state>',
    'Auto-captured lightweight Git state. Treat as a hint; re-run git/status checks before edits or final claims.',
    '```',
    status,
    lastCommit ? `\nlast commit: ${lastCommit}` : '',
    stagedStat ? `\nstaged diffstat:\n${stagedStat}` : '',
    unstagedStat ? `\nunstaged diffstat:\n${unstagedStat}` : '',
    '```',
    '</repo_state>',
  ].filter(Boolean).join('\n');
}

/** CustomEntry type for the fresh-session banner card. */
export const OCTOCODE_BANNER_ENTRY_TYPE = 'octocode-banner';

/**
 * Per-context guard: setWorkingIndicator / setWorkingMessage / setHiddenThinkingLabel
 * never change within a session, so we only apply them once to avoid the micro-flicker
 * that repeated calls (model_select, thinking_level_select, input) would produce.
 */
const workingUiInitCtxs = new WeakSet<object>();

export function applyOctocodeUi(ctx: PiContext | undefined, level?: string, contextTitle?: string): void {
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
  // supports reasoning. Empty → chip is hidden. The chip becomes 'thinking…' while
  // a turn is active (turn_start hook), and restores here on every level/model change.
  const thinkingStatus = getThinkingStatus(ctx, level);
  setManagedStatus(ctx, 'octocode-thinking', thinkingStatus ? paint(ui.theme, 'dim', thinkingStatus) : undefined);
  // One-time per context: working indicator frames, branded message, and the hidden
  // thinking label. These never change within a session; re-applying them on every
  // model/thinking/input event would cause unnecessary redraws and micro-flicker.
  if (!workingUiInitCtxs.has(ctx)) {
    workingUiInitCtxs.add(ctx);
    ui.setHiddenThinkingLabel?.('Octocode thinking');
    // Glyph-only indicator + branded message: Pi renders these side-by-side,
    // so keeping "Octocode" out of the frames avoids "Octocode Octocode …".
    const theme = ui.theme;
    setManagedWorkingIndicator(ctx, buildWorkingIndicator(theme));
    // Message/visibility are runtime state rendered by runtime-renderer. Only the
    // immutable indicator component is installed directly on the UI context.
    setManagedWorkingMessage(ctx, buildWorkingMessage(theme));
  }
}
