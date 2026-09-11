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
  private readonly cleanups: Array<{ active: boolean; dispose: SessionRuntimeDisposer }> = [];
  private readonly cleanupFailures: string[] = [];
  private cleanupClosed = false;
  private disposePromise?: Promise<void>;

  constructor(options: SessionRuntimeOptions = {}) {
    this.store = options.store ?? createRuntimeStore();
    this.generation = this.store.getState().begin('loading configuration');
    this.signal = this.controller.signal;
    this.rendererDisposer = (options.bindRenderer ?? bindRuntimeRenderer)(options.ctx, this.store);
    if (options.onDispose) this.addCleanup(options.onDispose);
  }

  addCleanup(dispose: SessionRuntimeDisposer): () => void {
    if (this.cleanupClosed) {
      void this.runCleanup(dispose, 'late-registration');
      return () => undefined;
    }
    const entry = { active: true, dispose };
    this.cleanups.push(entry);
    return () => { entry.active = false; };
  }

  getCleanupFailures(): readonly string[] {
    return [...this.cleanupFailures];
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
      // Stop UI delivery before awaiting teardown: late MCP/task publications from
      // the retiring generation must never paint through a replaced Pi context.
      this.rendererDisposer({ clearUi: reason === 'quit' });
      try {
        while (this.cleanups.length > 0) {
          const cleanup = this.cleanups.pop();
          if (cleanup?.active) await this.runCleanup(cleanup.dispose, reason);
        }
      } finally {
        this.cleanupClosed = true;
        this.store.getState().disposed();
      }
    })();
    return this.disposePromise;
  }

  private async runCleanup(dispose: SessionRuntimeDisposer, reason: string): Promise<void> {
    try {
      await dispose(reason);
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      this.cleanupFailures.push(message);
      this.store.getState().announce(`Octocode cleanup warning: ${message}`, 'warning');
    }
  }
}
