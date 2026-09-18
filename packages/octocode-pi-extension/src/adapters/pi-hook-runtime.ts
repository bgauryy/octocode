import { spawn } from 'node:child_process';
import path from 'node:path';
import type { HookHandlerDefinition } from '@octocodeai/agent-core';
import { getCapabilitySourceReview, reviewCapabilitySource, setCapabilitySourceEnabled } from '../contracts/capability-state.js';
import { isPersistentStorageEnabledForExtension } from '@octocodeai/config';
import type { PiContext } from '../types.js';
import { openOctocodeDb } from '../tools/storage-policy.js';
import { discoverPiHookSources, PI_HOOK_EVENT_ALIASES, type PiHookDiscoveryOptions, type PiHookDiscoveryResult } from './pi-hook-discovery.js';

export const PI_DECLARATIVE_HOOK_EVENTS = Object.freeze(Object.keys(PI_HOOK_EVENT_ALIASES));
const MAX_OUTPUT_BYTES = 64 * 1024;
const MAX_RECEIPTS = 100;

export interface PiHookReviewSource {
  readonly kind: 'hook';
  readonly sourceId: string;
  readonly revision: string;
  readonly name: string;
  readonly path: string;
}

export interface PiHookReviewStore {
  get(sourceId: string): { readonly revision: string; readonly enabled: boolean } | undefined;
  review(source: PiHookReviewSource): void;
  setEnabled(sourceId: string, enabled: boolean): void;
}

export interface PiHookRuntimeOptions extends PiHookDiscoveryOptions {
  readonly state?: PiHookReviewStore;
  readonly worker?: boolean;
}

export interface PiHookReceipt {
  readonly sourceId: string;
  readonly event: string;
  readonly outcome: 'success' | 'failed' | 'timeout' | 'cancelled';
  readonly durationMs: number;
  readonly blocked: boolean;
}

export interface PiHookSummary {
  readonly id: string;
  readonly name: string;
  readonly path: string;
  readonly scope: string;
  readonly revision: string;
  readonly events: readonly string[];
  readonly enabled: boolean;
  readonly status: 'active' | 'disabled' | 'pending-review' | 'untrusted' | 'shadowed' | 'unsupported' | 'unavailable';
  readonly diagnostics: readonly string[];
}

interface CommandResult {
  readonly outcome: PiHookReceipt['outcome'];
  readonly output: string;
  readonly exitCode: number | null;
}

function record(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function blockingReason(result: CommandResult): string | undefined {
  if (result.outcome !== 'success') return `Declarative hook ${result.outcome}`;
  if (result.exitCode === 2) return 'Blocked by declarative hook';
  try {
    const value: unknown = JSON.parse(result.output);
    if (!record(value)) return undefined;
    const specific = record(value['hookSpecificOutput']) ? value['hookSpecificOutput'] : {};
    if (value['decision'] === 'block' || value['decision'] === 'deny' || value['block'] === true || value['continue'] === false || specific['permissionDecision'] === 'deny') {
      return typeof value['reason'] === 'string' ? value['reason'].slice(0, 4096)
        : typeof specific['permissionDecisionReason'] === 'string' ? specific['permissionDecisionReason'].slice(0, 4096)
          : 'Blocked by declarative hook';
    }
  } catch { /* Plain text output carries no lifecycle authority. */ }
  return undefined;
}

function piBlock(event: string, reason: string): unknown {
  if (event === 'tool_call') return { block: true, reason };
  if (event === 'session_before_compact') return { cancel: true };
  if (event === 'input') return { action: 'handled' };
  return undefined;
}

function commandInput(event: string, payload: Record<string, unknown>, ctx: PiContext | undefined, workspace: string): string {
  return JSON.stringify({
    ...payload,
    hook_event_name: event,
    cwd: ctx?.cwd ?? workspace,
    ...(ctx?.sessionManager?.getSessionId?.() ? { session_id: ctx.sessionManager.getSessionId() } : {}),
    ...(typeof payload['toolName'] === 'string' ? { tool_name: payload['toolName'] } : {}),
    ...(payload['input'] !== undefined ? { tool_input: payload['input'] } : {}),
    ...(payload['result'] !== undefined ? { tool_response: payload['result'] } : {}),
    ...(typeof payload['text'] === 'string' ? { prompt: payload['text'] } : {}),
  });
}

async function executeCommand(
  handler: Extract<HookHandlerDefinition, { type: 'command' }>,
  input: string,
  cwd: string,
  signal: AbortSignal,
): Promise<CommandResult> {
  if (signal.aborted) return { outcome: 'cancelled', output: '', exitCode: null };
  const windows = process.platform === 'win32';
  const command = windows ? handler.commandWindows ?? handler.command : handler.command;
  return new Promise(resolve => {
    let output = '';
    let bytes = 0;
    let outcome: CommandResult['outcome'] = 'success';
    let finished = false;
    let child: ReturnType<typeof spawn>;
    try {
      child = spawn(windows ? 'cmd.exe' : '/bin/sh', windows ? ['/d', '/s', '/c', command] : ['-c', command], {
        cwd, stdio: ['pipe', 'pipe', 'pipe'], detached: !windows, windowsHide: true,
      });
    } catch {
      resolve({ outcome: 'failed', output: '', exitCode: null });
      return;
    }
    const kill = (reason: CommandResult['outcome']) => {
      outcome = reason;
      try { if (!windows && child.pid) process.kill(-child.pid, 'SIGKILL'); else child.kill('SIGKILL'); } catch { /* Already exited. */ }
    };
    const abort = () => kill('cancelled');
    const timer = setTimeout(() => kill('timeout'), Math.min(handler.timeoutSeconds * 1000, 2_147_483_647));
    const finish = (code: number | null) => {
      if (finished) return;
      finished = true;
      clearTimeout(timer);
      signal.removeEventListener('abort', abort);
      if (outcome === 'success' && code !== 0 && code !== 2) outcome = 'failed';
      resolve({ outcome, output, exitCode: code });
    };
    signal.addEventListener('abort', abort, { once: true });
    if (signal.aborted) abort();
    child.stdout?.on('data', (chunk: Buffer) => {
      bytes += chunk.length;
      if (bytes > MAX_OUTPUT_BYTES) { kill('failed'); return; }
      output += chunk.toString('utf8');
    });
    child.stderr?.on('data', (chunk: Buffer) => { bytes += chunk.length; if (bytes > MAX_OUTPUT_BYTES) kill('failed'); });
    child.on('error', () => { outcome = 'failed'; finish(null); });
    child.on('close', finish);
    child.stdin?.on('error', () => { /* A command may intentionally ignore input. */ });
    child.stdin?.end(`${input}\n`);
  });
}

export function createPiHookRuntime(options: PiHookRuntimeOptions) {
  const state = options.state ?? createPiHookReviewStore(options.workspace);
  let discovery: PiHookDiscoveryResult = discoverPiHookSources(options);
  let trusted = false;
  let disposed = false;
  const active = new Set<AbortController>();
  const bound = new WeakSet<object>();
  const receipts: PiHookReceipt[] = [];

  function summaries(): PiHookSummary[] {
    return discovery.definitions.map(definition => {
      const diagnostics = definition.configuration.unsupported.map(item => `Unsupported hook event: ${item.event}`);
      let saved: ReturnType<PiHookReviewStore['get']>;
      let stateUnavailable = false;
      try {
        saved = state.get(definition.source.id);
      } catch {
        // Unreadable review storage cannot grant execution or crash read-only configuration UI.
        stateUnavailable = true;
        diagnostics.push('Hook review state is unavailable; commands remain disabled');
      }
      const groups = Object.values(definition.configuration.hooks).flat();
      for (const group of groups) for (const handler of group?.handlers ?? []) {
        if (handler.type !== 'command') diagnostics.push('Only declarative command handlers execute in Pi');
        else if (handler.async) diagnostics.push('Asynchronous handlers are unsupported; remove async to run a bounded command');
      }
      const enabled = saved?.enabled ?? !stateUnavailable;
      const status: PiHookSummary['status'] = definition.status === 'shadowed' ? 'shadowed'
        : stateUnavailable ? 'unavailable' : !enabled ? 'disabled'
          : diagnostics.length > 0 ? 'unsupported'
            : saved?.revision !== definition.source.normalizedHash ? 'pending-review'
              : definition.source.scope === 'workspace' && !trusted ? 'untrusted' : 'active';
      return {
        id: definition.source.id, name: definition.name, path: definition.source.provenance, scope: definition.source.scope,
        revision: definition.source.normalizedHash, events: Object.keys(definition.configuration.hooks), enabled, status, diagnostics,
      };
    });
  }

  function refresh(input: { trusted?: boolean } = {}) {
    if (input.trusted !== undefined) trusted = input.trusted;
    discovery = discoverPiHookSources(options);
    const statuses = new Map(summaries().map(source => [source.id, source.status]));
    for (const definition of discovery.definitions) {
      const enabled = statuses.get(definition.source.id) === 'active';
      discovery.catalog.setEnabled(definition.source.id, enabled);
      if (enabled) discovery.catalog.review(definition.source.id, definition.source.normalizedHash);
    }
    return snapshot();
  }

  function snapshot() {
    const sources = summaries();
    const stateErrors = sources.filter(source => source.status === 'unavailable')
      .map(source => ({ path: source.path, message: 'Hook review state is unavailable; commands remain disabled' }));
    return { sources, errors: [...discovery.errors, ...stateErrors], receipts: [...receipts] };
  }

  function review(sourceId: string, expectedRevision: string): void {
    refresh();
    const source = summaries().find(source => source.id === sourceId);
    if (!source || source.revision !== expectedRevision) throw new Error('Hook review revision no longer matches the discovered definition');
    if (source.status === 'shadowed' || source.status === 'unsupported') throw new Error('Hook source cannot execute in its current state');
    state.review({ kind: 'hook', sourceId, revision: expectedRevision, name: source.name, path: source.path });
    refresh();
  }

  function setEnabled(sourceId: string, enabled: boolean): void {
    state.setEnabled(sourceId, enabled);
    refresh();
  }

  async function dispatch(piEvent: string, rawPayload: unknown, ctx?: PiContext, signal = ctx?.signal): Promise<unknown> {
    if (disposed) return undefined;
    if (ctx?.cwd && path.resolve(ctx.cwd) !== path.resolve(options.workspace)) return undefined;
    try { trusted = ctx?.isProjectTrusted?.() ?? false; } catch { trusted = false; }
    refresh();
    const payload = record(rawPayload) ? rawPayload : {};
    const event = PI_HOOK_EVENT_ALIASES[piEvent];
    if (!event) return undefined;
    const events = [event, ...(piEvent === 'tool_call' ? ['PermissionRequest'] : []),
      ...(options.worker && piEvent === 'session_start' ? ['SubagentStart'] : []),
      ...(options.worker && piEvent === 'session_shutdown' ? ['SubagentStop'] : [])];
    for (const entry of discovery.catalog.effective(trusted, false)) {
      if (!events.includes(entry.event)) continue;
      const matchValue = typeof payload['toolName'] === 'string' ? payload['toolName']
        : typeof payload['reason'] === 'string' ? payload['reason'] : '';
      if (entry.group.matcher && !new RegExp(entry.group.matcher).test(matchValue)) continue;
      for (const handler of entry.group.handlers) {
        if (handler.type !== 'command' || handler.async) continue;
        const controller = new AbortController();
        const abort = () => controller.abort();
        active.add(controller);
        signal?.addEventListener('abort', abort, { once: true });
        if (signal?.aborted) abort();
        const startedAt = Date.now();
        let result: CommandResult;
        try {
          result = await executeCommand(handler, commandInput(entry.event, payload, ctx, options.workspace), path.resolve(ctx?.cwd ?? options.workspace), controller.signal);
        } finally {
          signal?.removeEventListener('abort', abort);
          active.delete(controller);
        }
        const reason = blockingReason(result);
        const decision = reason ? piBlock(piEvent, reason) : undefined;
        receipts.push({ sourceId: entry.source.id, event: entry.event, outcome: result.outcome, durationMs: Date.now() - startedAt, blocked: decision !== undefined });
        if (receipts.length > MAX_RECEIPTS) receipts.splice(0, receipts.length - MAX_RECEIPTS);
        if (decision !== undefined) return decision;
        if (signal?.aborted || disposed) return undefined;
      }
    }
    return undefined;
  }

  function bind(composer: { on(event: string, name: string, handler: (...args: unknown[]) => unknown): void }): void {
    if (bound.has(composer)) return;
    bound.add(composer);
    for (const event of PI_DECLARATIVE_HOOK_EVENTS) composer.on(event, 'octocode-declarative-hooks', (payload, ctx) => dispatch(event, payload, ctx as PiContext | undefined));
  }

  function dispose(): void {
    disposed = true;
    for (const controller of active) controller.abort();
    active.clear();
  }

  return { refresh, snapshot, review, setEnabled, dispatch, bind, dispose };
}

export type PiHookRuntime = ReturnType<typeof createPiHookRuntime>;

/** Reviews share the extension state owner and honor its memory-storage policy. */
export function createPiHookReviewStore(workspace: string): PiHookReviewStore {
  const scope = path.resolve(workspace);
  const memory = new Map<string, { revision: string; enabled: boolean }>();
  return {
    get(sourceId) {
      if (!isPersistentStorageEnabledForExtension()) return memory.get(sourceId);
      return getCapabilitySourceReview(openOctocodeDb(), scope, sourceId);
    },
    review(source) {
      if (!isPersistentStorageEnabledForExtension()) { memory.set(source.sourceId, { revision: source.revision, enabled: true }); return; }
      reviewCapabilitySource(openOctocodeDb(), scope, source);
    },
    setEnabled(sourceId, enabled) {
      if (!isPersistentStorageEnabledForExtension()) {
        const review = memory.get(sourceId);
        if (!review) throw new Error('Review the hook before enabling it');
        memory.set(sourceId, { ...review, enabled });
        return;
      }
      setCapabilitySourceEnabled(openOctocodeDb(), scope, sourceId, enabled);
    },
  };
}
