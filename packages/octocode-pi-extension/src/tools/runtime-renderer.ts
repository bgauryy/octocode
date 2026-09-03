import type { PiContext, PiFooterData, PiTheme, PiWorkingIndicator } from '../types.js';
import { createRuntimeStore, type ForegroundActivity, type ForegroundActivityInput, type RuntimeMcpState, type RuntimeState, type RuntimeStore } from './runtime-store.js';

interface RuntimeBinding {
  store: RuntimeStore;
  dispose: RuntimeRendererDisposer;
  widgets: Set<string>;
  footer: boolean;
  workingIndicator: boolean;
}

export type RuntimeRendererDisposer = (opts?: { clearUi?: boolean }) => void;

const bindings = new WeakMap<object, RuntimeBinding>();
interface RenderedRuntimeState {
  statuses: Map<string, string | undefined>;
  workingVisible?: boolean;
  workingMessage?: string;
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

export function activityPresentation(activity: ForegroundActivity): { visible: boolean; message?: string; status?: string } {
  switch (activity.kind) {
    case 'idle': return { visible: false };
    case 'thinking': return { visible: true, message: 'Thinking…', status: 'Thinking…' };
    case 'researching': return { visible: true, message: `Researching…${activity.detail ? ` ${activity.detail}` : ''}`, status: 'Researching…' };
    case 'awaiting_input': return { visible: false, status: 'Input needed' };
    case 'planning': return { visible: true, message: `Planning…${activity.detail ? ` ${activity.detail}` : ''}`, status: 'Planning…' };
    case 'reviewing': return { visible: false, status: 'RFC ready for review' };
    case 'awaiting_start': return { visible: false, status: 'Ready to start' };
    case 'ready_to_work': return { visible: false, status: `Ready · ${activity.label}` };
    case 'working': return { visible: true, message: `Working… ${activity.label}`, status: 'Working…' };
    case 'verifying': return { visible: true, message: `Verifying…${activity.label ? ` ${activity.label}` : ''}`, status: 'Verifying…' };
    case 'blocked': return { visible: false, status: `Blocked · ${activity.label}` };
    case 'complete': return { visible: false, status: activity.label ? `Complete · ${activity.label}` : 'Complete' };
    case 'failed': return { visible: false, status: `Failed · ${activity.label}` };
  }
}

function renderRuntime(ctx: PiContext, state: RuntimeState, rendered: RenderedRuntimeState): void {
  if (!ctx.hasUI || !ctx.ui) return;
  const statuses = { ...state.statuses };
  statuses['octocode-init'] = isLoading(state) ? `Octocode · ${state.stage}` : undefined;
  statuses['octocode-mcp-init'] = mcpStageText(state.mcp);
  const activity = activityPresentation(state.activity);
  statuses['octocode-activity'] = activity.status;
  const keys = new Set([...rendered.statuses.keys(), ...Object.keys(statuses)]);
  for (const key of keys) {
    const next = statuses[key];
    if (rendered.statuses.get(key) === next) continue;
    ctx.ui.setStatus?.(key, next);
    if (next === undefined) rendered.statuses.delete(key);
    else rendered.statuses.set(key, next);
  }
  const loading = isLoading(state);
  const workingVisible = loading || activity.visible || (state.activity.kind === 'idle' && state.working.visible);
  const workingMessage = loading
    ? `Octocode · ${state.stage}`
    : activity.message ?? (state.activity.kind === 'idle' ? state.working.message : undefined);
  if (rendered.workingMessage !== workingMessage) {
    ctx.ui.setWorkingMessage?.(workingMessage);
    rendered.workingMessage = workingMessage;
  }
  if (rendered.workingVisible !== workingVisible) {
    ctx.ui.setWorkingVisible?.(workingVisible);
    rendered.workingVisible = workingVisible;
  }
}

export function bindRuntimeRenderer(ctx: PiContext | undefined, store: RuntimeStore): RuntimeRendererDisposer {
  if (!ctx || typeof ctx !== 'object') return () => undefined;
  bindings.get(ctx)?.dispose();
  const rendered: RenderedRuntimeState = { statuses: new Map() };
  const binding: RuntimeBinding = {
    store,
    dispose: () => undefined,
    widgets: new Set(),
    footer: false,
    workingIndicator: false,
  };
  let noticeId = 0;
  const paint = (state: RuntimeState): void => {
    try {
      renderRuntime(ctx, state, rendered);
      if (state.notice && state.notice.id !== noticeId) {
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
      bindings.delete(ctx);
    }
    if (opts.clearUi !== false && !uiCleared) {
      uiCleared = true;
      for (const key of rendered.statuses.keys()) ctx.ui?.setStatus?.(key, undefined);
      for (const widget of binding.widgets) ctx.ui?.setWidget?.(widget, undefined);
      if (binding.footer) ctx.ui?.setFooter?.(undefined);
      if (binding.workingIndicator) ctx.ui?.setWorkingIndicator?.(undefined);
      ctx.ui?.setWorkingMessage?.(undefined);
      ctx.ui?.setWorkingVisible?.(false);
    }
    rendered.statuses.clear();
    binding.widgets.clear();
    binding.footer = false;
    binding.workingIndicator = false;
  };
  binding.dispose = dispose;
  bindings.set(ctx, binding);
  return dispose;
}

export function runtimeStoreFor(ctx: PiContext | undefined): RuntimeStore | undefined {
  return ctx && typeof ctx === 'object' ? bindings.get(ctx)?.store : undefined;
}

/**
 * UI adapters may be invoked just before Pi emits session_start (or by isolated
 * command hosts). Give that context a renderer-owned provisional store; the real
 * SessionRuntime atomically replaces and disposes it during initialization.
 */
function ensureRuntimeBinding(ctx: PiContext | undefined): RuntimeBinding | undefined {
  if (!ctx || typeof ctx !== 'object') return undefined;
  const existing = bindings.get(ctx);
  if (existing) return existing;
  bindRuntimeRenderer(ctx, createRuntimeStore());
  return bindings.get(ctx);
}

export function setManagedStatus(ctx: PiContext | undefined, name: string, text: string | undefined): void {
  const store = ensureRuntimeBinding(ctx)?.store;
  if (store) store.getState().setStatus(name, text);
}

export function setManagedWorking(ctx: PiContext | undefined, visible: boolean, message?: string): void {
  const store = ensureRuntimeBinding(ctx)?.store;
  if (store) store.getState().setWorking(visible, message);
}

export function setManagedActivity(ctx: PiContext | undefined, activity: ForegroundActivityInput): void {
  const store = ensureRuntimeBinding(ctx)?.store;
  if (store) store.getState().setActivity(activity);
}

export function setManagedWorkingMessage(ctx: PiContext | undefined, message?: string): void {
  const store = ensureRuntimeBinding(ctx)?.store;
  if (store) {
    const working = store.getState().working;
    store.getState().setWorking(working.visible, message);
  }
}

export function setManagedWidget(
  ctx: PiContext | undefined,
  name: string,
  content: string[] | ((tui: unknown, theme: PiTheme) => unknown) | undefined,
  opts?: { placement?: 'aboveEditor' | 'belowEditor' },
): void {
  if (!ctx || typeof ctx !== 'object') return;
  const binding = ensureRuntimeBinding(ctx);
  if (!binding) return;
  ctx.ui?.setWidget?.(name, content, opts);
  if (content === undefined) binding.widgets.delete(name);
  else binding.widgets.add(name);
}

export function setManagedFooter(
  ctx: PiContext | undefined,
  factory: ((tui: unknown, theme: PiTheme, footerData?: PiFooterData) => unknown) | undefined,
): void {
  if (!ctx || typeof ctx !== 'object') return;
  const binding = ensureRuntimeBinding(ctx);
  if (!binding) return;
  ctx.ui?.setFooter?.(factory);
  binding.footer = factory !== undefined;
}

export function setManagedWorkingIndicator(ctx: PiContext | undefined, indicator?: PiWorkingIndicator): void {
  if (!ctx || typeof ctx !== 'object') return;
  const binding = ensureRuntimeBinding(ctx);
  if (!binding) return;
  ctx.ui?.setWorkingIndicator?.(indicator);
  binding.workingIndicator = indicator !== undefined;
}

export function publishMcpRuntimeState(ctx: PiContext | undefined, patch: Partial<RuntimeMcpState>): void {
  runtimeStoreFor(ctx)?.getState().setMcp(patch);
}
