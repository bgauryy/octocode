import type { ReadonlyFooterDataProvider, WorkingIndicatorOptions } from '@earendil-works/pi-coding-agent';
import type { PiContext, PiTheme } from '../types.js';
import { createRuntimeStore, type ForegroundActivityInput, type RuntimeMcpState, type RuntimeState, type RuntimeStore } from './runtime-store.js';

import { runtimeActivityPresentation } from './activity-presentation.js';

interface RuntimeBinding {
  store: RuntimeStore;
  dispose: RuntimeRendererDisposer;
  footer: boolean;
  workingIndicator: boolean;
}

export type RuntimeRendererDisposer = (opts?: { clearUi?: boolean }) => void;

const bindings = new WeakMap<object, RuntimeBinding>();
// Pi creates a fresh context per event/tool call. The session manager is stable.
function bindingKey(ctx: PiContext | undefined): object | undefined {
  if (!ctx || typeof ctx !== 'object') return undefined;
  try {
    return ctx.sessionManager ?? ctx;
  } catch {
    return undefined; // Replaced contexts must not acquire another runtime.
  }
}
interface RenderedRuntimeState {
  statuses: Map<string, string | undefined>;
  workingVisible?: boolean;
}

function isLoading(state: RuntimeState): boolean {
  return state.phase === 'initializing';
}

function mcpStageText(mcp: RuntimeMcpState): string | undefined {
  if (mcp.status === 'idle') return undefined;
  const counts = mcp.servers > 0 || mcp.tools > 0 ? ` · ${mcp.servers} server${mcp.servers === 1 ? '' : 's'} · ${mcp.tools} tools` : '';
  const progress = mcp.totalServers > 0
    ? ` ${Math.min(mcp.completedServers, mcp.totalServers)}/${mcp.totalServers}`
    : '';
  const current = mcp.currentServer ? ` · ${mcp.currentServer}` : '';
  if (mcp.status === 'running') return `MCP${progress} · ${mcp.message ?? 'discovering'}${current}${counts}`;
  if (mcp.status === 'degraded' || mcp.status === 'failed') return `MCP · ${mcp.message ?? 'ready with warnings'}${counts}`;
  return `MCP · ready${counts}${mcp.source === 'cache' ? ' · cached' : ''}`;
}

function renderRuntime(ctx: PiContext, state: RuntimeState, rendered: RenderedRuntimeState): void {
  if (!ctx.hasUI || !ctx.ui) return;
  const statuses = { ...state.statuses };
  statuses['octocode-init'] = isLoading(state) ? `Octocode · ${state.stage}` : undefined;
  statuses['octocode-mcp-init'] = mcpStageText(state.mcp);
  const activity = runtimeActivityPresentation(state);
  // Foreground activity text has one persistent owner: the custom footer.
  // Keep Pi's motion indicator here, but do not repeat the same lifecycle label
  // in the status row or working-message row.
  statuses['octocode-activity'] = undefined;
  const keys = new Set([...rendered.statuses.keys(), ...Object.keys(statuses)]);
  for (const key of keys) {
    const next = statuses[key];
    if (rendered.statuses.get(key) === next) continue;
    ctx.ui.setStatus?.(key, next);
    if (next === undefined) rendered.statuses.delete(key);
    else rendered.statuses.set(key, next);
  }
  const loading = isLoading(state);
  // Show the motion indicator whenever the agent is in an active turn — even when plan
  // lifecycle has moved past 'idle' (ready_to_work, reviewing, awaiting_start, etc.).
  // activeTurnStartedAt is set on turn_start and cleared on turn_end.
  const inActiveTurn = !!state.footer.activeTurnStartedAt;
  const workingVisible = !activity.attention && (loading || activity.visible || inActiveTurn);
  if (rendered.workingVisible !== workingVisible) {
    ctx.ui.setWorkingVisible?.(workingVisible);
    rendered.workingVisible = workingVisible;
  }
}

export function bindRuntimeRenderer(ctx: PiContext | undefined, store: RuntimeStore): RuntimeRendererDisposer {
  const key = bindingKey(ctx);
  if (!ctx || !key) return () => undefined;
  bindings.get(key)?.dispose();
  const rendered: RenderedRuntimeState = { statuses: new Map() };
  const binding: RuntimeBinding = {
    store,
    dispose: () => undefined,
    footer: false,
    workingIndicator: false,
  };
  let noticeId = 0;
  const paint = (state: RuntimeState): void => {
    try {
      renderRuntime(ctx, state, rendered);
      if (ctx.hasUI && state.notice && state.notice.id !== noticeId) {
        noticeId = state.notice.id;
        ctx.ui?.notify?.(state.notice.message, state.notice.level);
      }
    } catch {
      // Pi invalidates old session contexts during /new, /resume, and /fork.
      // Late store publications remain harmless and are discarded by generation.
    }
  };
  const unsubscribe = store.subscribe(paint);
  paint(store.getState());
  let lifecycleDisposed = false;
  let uiCleared = false;
  const dispose: RuntimeRendererDisposer = (opts = {}) => {
    if (!lifecycleDisposed) {
      lifecycleDisposed = true;
      unsubscribe();
      if (bindings.get(key) === binding) bindings.delete(key);
    }
    if (opts.clearUi !== false && !uiCleared) {
      uiCleared = true;
      for (const key of rendered.statuses.keys()) ctx.ui?.setStatus?.(key, undefined);
      if (binding.footer) ctx.ui?.setFooter?.(undefined);
      if (binding.workingIndicator) ctx.ui?.setWorkingIndicator?.(undefined);
      ctx.ui?.setWorkingMessage?.(undefined);
      ctx.ui?.setWorkingVisible?.(false);
    }
    rendered.statuses.clear();
    binding.footer = false;
    binding.workingIndicator = false;
  };
  binding.dispose = dispose;
  bindings.set(key, binding);
  return dispose;
}

export function runtimeStoreFor(ctx: PiContext | undefined): RuntimeStore | undefined {
  const key = bindingKey(ctx);
  return key ? bindings.get(key)?.store : undefined;
}

/**
 * UI adapters may be invoked just before Pi emits session_start (or by isolated
 * command hosts). Give that context a renderer-owned provisional store; the real
 * SessionRuntime atomically replaces and disposes it during initialization.
 */
function ensureRuntimeBinding(ctx: PiContext | undefined): RuntimeBinding | undefined {
  const key = bindingKey(ctx);
  if (!key) return undefined;
  const existing = bindings.get(key);
  if (existing) return existing;
  bindRuntimeRenderer(ctx, createRuntimeStore());
  return bindings.get(key);
}

export function setManagedStatus(ctx: PiContext | undefined, name: string, text: string | undefined): void {
  const store = ensureRuntimeBinding(ctx)?.store;
  if (store) store.getState().setStatus(name, text);
}

export function setManagedActivity(ctx: PiContext | undefined, activity: ForegroundActivityInput): void {
  const store = ensureRuntimeBinding(ctx)?.store;
  if (store) store.getState().setActivity(activity);
}

export function setManagedFooter(
  ctx: PiContext | undefined,
  factory: ((tui: unknown, theme: PiTheme, footerData?: ReadonlyFooterDataProvider) => unknown) | undefined,
): void {
  if (!ctx || typeof ctx !== 'object') return;
  const binding = ensureRuntimeBinding(ctx);
  if (!binding) return;
  ctx.ui?.setFooter?.(factory);
  binding.footer = factory !== undefined;
}

export function setManagedWorkingIndicator(ctx: PiContext | undefined, indicator?: WorkingIndicatorOptions): void {
  if (!ctx || typeof ctx !== 'object') return;
  const binding = ensureRuntimeBinding(ctx);
  if (!binding) return;
  ctx.ui?.setWorkingIndicator?.(indicator);
  binding.workingIndicator = indicator !== undefined;
}

export function publishMcpRuntimeState(ctx: PiContext | undefined, patch: Partial<RuntimeMcpState>): void {
  runtimeStoreFor(ctx)?.getState().setMcp(patch);
}
