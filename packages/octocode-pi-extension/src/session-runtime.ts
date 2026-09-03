import type { PiContext } from './types.js';
import { bindRuntimeRenderer, type RuntimeRendererDisposer } from './tools/runtime-renderer.js';
import { createRuntimeStore, runRuntimeTask, type RuntimeStore } from './tools/runtime-store.js';

export type SessionRuntimeDisposer = (reason?: string) => void | Promise<void>;

export interface SessionRuntimeTask<T = unknown> {
  name: string;
  message: string;
  critical?: boolean;
  readyMessage?: string;
  run(signal: AbortSignal, generation: number): T | Promise<T>;
}

export interface SessionRuntimeOptions {
  ctx?: PiContext;
  store?: RuntimeStore;
  bindRenderer?: (ctx: PiContext | undefined, store: RuntimeStore) => RuntimeRendererDisposer;
  onDispose?: SessionRuntimeDisposer;
}

export interface SessionRuntimeSettlement {
  readyMessage?: string;
  degradedMessage?: string;
}

export class SessionRuntime {
  readonly store: RuntimeStore;
  readonly generation: number;
  readonly signal: AbortSignal;
  private readonly controller = new AbortController();
  private readonly rendererDisposer: RuntimeRendererDisposer;
  private readonly onDispose?: SessionRuntimeDisposer;
  private disposePromise?: Promise<void>;

  constructor(options: SessionRuntimeOptions = {}) {
    this.store = options.store ?? createRuntimeStore();
    this.generation = this.store.getState().begin('loading configuration');
    this.signal = this.controller.signal;
    this.rendererDisposer = (options.bindRenderer ?? bindRuntimeRenderer)(options.ctx, this.store);
    this.onDispose = options.onDispose;
  }

  isCurrent(): boolean {
    const state = this.store.getState();
    return !this.signal.aborted
      && state.generation === this.generation
      && state.phase !== 'disposing'
      && state.phase !== 'disposed';
  }

  async runTask<T>(task: SessionRuntimeTask<T>): Promise<T | undefined> {
    return runRuntimeTask(this.store, task.name, task.message, () => task.run(this.signal, this.generation), {
      critical: task.critical,
      readyMessage: task.readyMessage,
      signal: this.signal,
      generation: this.generation,
    });
  }

  async runTasks(tasks: SessionRuntimeTask[], settlement: SessionRuntimeSettlement = {}): Promise<PromiseSettledResult<unknown>[]> {
    const receipts = await Promise.allSettled(tasks.map((task) => this.runTask(task)));
    this.settleInitialization(settlement);
    return receipts;
  }

  settleInitialization(settlement: SessionRuntimeSettlement = {}): void {
    if (!this.isCurrent()) return;
    const tasks = Object.values(this.store.getState().tasks);
    const criticalFailure = tasks.find((task) => task.status === 'failed' && task.critical);
    if (criticalFailure) {
      this.store.getState().failed(criticalFailure.error ?? criticalFailure.message ?? 'critical runtime task failed');
      return;
    }
    const degradedCount = tasks.filter((task) => task.status === 'degraded' || task.status === 'failed').length;
    if (degradedCount > 0) {
      this.store.getState().degraded(settlement.degradedMessage ?? `Octocode ready with ${degradedCount} warning${degradedCount === 1 ? '' : 's'}`);
      return;
    }
    this.store.getState().ready(settlement.readyMessage);
  }

  dispose(reason = 'shutdown'): Promise<void> {
    if (this.disposePromise) return this.disposePromise;
    this.disposePromise = (async () => {
      this.store.getState().disposing();
      this.controller.abort(new Error(`Session runtime disposed: ${reason}`));
      try {
        await this.onDispose?.(reason);
      } finally {
        this.rendererDisposer({ clearUi: reason === 'quit' });
        this.store.getState().disposed();
      }
    })();
    return this.disposePromise;
  }
}
