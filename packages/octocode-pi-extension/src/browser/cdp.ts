import { errorMessage, withTimeout } from '../shared/util.js';
import { abortError } from './abort.js';
import { checkDevtoolsUrl, type PageTarget } from './devtools.js';
import { browserWebSocketUrl } from './launch.js';

/**
 * Minimal Chrome DevTools Protocol client: one browser-level WebSocket (`CdpConnection`) with a session per page
 * (`CdpPage`). The state those pages share (console, origins, downloads) is `session-state.ts`; the browser's targets and
 * its DevTools URLs are `devtools.ts`; opening a session is `session.ts`; checking its requests is `page-network.ts`.
 */
interface Reply {
  id?: number;
  result?: unknown;
  error?: { message: string };
  method?: string;
  params?: Record<string, unknown>;
  sessionId?: string;
}

interface Pending {
  /** The session the command was sent on (none: the browser itself). Only a reply carrying the same one settles it. */
  sessionId: string | undefined;
  resolve: (value: unknown) => void;
  reject: (error: Error) => void;
}

type CdpListener = (method: string, params: Record<string, unknown>) => void;

/** What the browser says about a target it attached the session to. */
interface TargetInfo {
  targetId: string;
  /** `page`, `iframe`, `worker`, `service_worker`, `shared_worker`, `other`, … */
  type: string;
  url?: string;
  /** The page that opened this one (`window.open`, a link to a new window). */
  openerId?: string;
}

/** A target the browser attached the session to by itself (see `Target.setAutoAttach`): a popup, a frame in another process, a worker. */
export interface AttachedTarget {
  page: CdpPage;
  info: TargetInfo;
  /** The target is held before it runs anything until `Runtime.runIfWaitingForDebugger`. */
  waiting: boolean;
  /** The session the browser announced the attach on: the one of the page that owns a worker or frame. None: the browser itself (a popup, a service worker). */
  parentSessionId?: string;
}

/** What `Target.setAutoAttach` asks for: every related target is attached, held before its first instruction, and gets its own flat session. */
const AUTO_ATTACH = { autoAttach: true, waitForDebuggerOnStart: true, flatten: true };

/**
 * One browser-level DevTools WebSocket, shared by every page the session drives. Commands carry a `sessionId` (flat
 * mode: `Target.attachToTarget { flatten: true }`) and so do the events and replies that come back, which is how each
 * target's traffic reaches only its own `CdpPage`. The socket closes when the session ends, not when a page does.
 */
export class CdpConnection {
  readonly #socket: WebSocket;
  readonly #port: number;
  readonly #pending = new Map<number, Pending>();
  /** The attached sessions and their event listeners. */
  readonly #sessions = new Map<string, Set<CdpListener>>();
  /** Targets this client is attaching to itself, so the browser's `Target.attachedToTarget` for them is not taken for an auto-attach. */
  readonly #attaching = new Set<string>();
  readonly #attachedListeners = new Set<(attached: AttachedTarget) => void>();
  readonly #endedListeners = new Set<(sessionId: string) => void>();
  #nextId = 1;

  private constructor(socket: WebSocket, port: number) {
    this.#socket = socket;
    this.#port = port;
    socket.addEventListener('message', (event) => this.#receive(String(event.data)));
    socket.addEventListener('close', () => {
      const ended = [...this.#sessions.keys()];
      this.#sessions.clear();
      this.#attachedListeners.clear();
      this.#rejectPending(() => true);
      for (const sessionId of ended) this.#ended(sessionId);
      this.#endedListeners.clear();
    });
  }

  /** Connects to the browser-level DevTools socket of the browser on `port`; a URL that is not that browser's own is refused first (see `checkDevtoolsUrl`). */
  static async connect(port: number): Promise<CdpConnection> {
    const url = await browserWebSocketUrl(port, 2_000);
    checkDevtoolsUrl(url, port);
    const socket = new WebSocket(url);
    try {
      await withTimeout(
        new Promise<void>((resolve, reject) => {
          socket.addEventListener('open', () => resolve(), { once: true });
          socket.addEventListener('error', () => reject(new Error(`Cannot connect to ${url}`)), { once: true });
        }),
        10_000,
        'Browser connect',
      );
    } catch (error) {
      // A timed-out socket would otherwise keep connecting in the background.
      socket.close();
      throw error;
    }
    return new CdpConnection(socket, port);
  }

  get open(): boolean {
    return this.#socket.readyState === WebSocket.OPEN;
  }

  #receive(data: string): void {
    // A throwing EventTarget listener surfaces as an uncaught exception in Pi's process.
    let message: Reply;
    try {
      message = JSON.parse(data) as Reply;
    } catch {
      return;
    }
    if (message.id !== undefined) {
      const pending = this.#pending.get(message.id);
      if (!pending || pending.sessionId !== message.sessionId) return;
      this.#pending.delete(message.id);
      if (message.error) pending.reject(new Error(message.error.message));
      else pending.resolve(message.result);
    } else if (message.method === 'Target.attachedToTarget') {
      this.#autoAttached(message.params ?? {}, message.sessionId);
    } else if (message.method === 'Target.detachedFromTarget') {
      // A worker or frame that goes away is announced on its parent's session: only the session it names ends.
      if (typeof message.params?.['sessionId'] === 'string') this.#forget(message.params['sessionId']);
    } else if (message.method && message.sessionId !== undefined) {
      for (const listener of this.#sessions.get(message.sessionId) ?? []) {
        try {
          listener(message.method, message.params ?? {});
        } catch {
          // One bad event must not break the session.
        }
      }
    }
  }

  /**
   * The browser attached a session by itself (any parent session, or the browser's own). The session is routable at once, so
   * the reply to the first command on it is never lost. The attach this client asked for (`attach`) is announced the same
   * way and is not an auto-attach, unless the browser holds that target for the debugger.
   */
  #autoAttached(params: Record<string, unknown>, parentSessionId: string | undefined): void {
    const sessionId = params['sessionId'];
    const info = params['targetInfo'] as Partial<TargetInfo> | undefined;
    if (typeof sessionId !== 'string' || typeof info?.targetId !== 'string' || typeof info.type !== 'string') return;
    const waiting = params['waitingForDebugger'] === true;
    const known = this.#sessions.has(sessionId);
    if (!known) this.#sessions.set(sessionId, new Set());
    if (!waiting && (known || this.#attaching.has(info.targetId))) return;
    const attached: AttachedTarget = { page: new CdpPage(this, sessionId), info: info as TargetInfo, waiting, ...(parentSessionId === undefined ? {} : { parentSessionId }) };
    for (const listener of this.#attachedListeners) {
      try {
        listener(attached);
      } catch {
        // One bad listener must not break the connection.
      }
    }
  }

  #rejectPending(match: (pending: Pending) => boolean): void {
    for (const [id, pending] of this.#pending) {
      if (!match(pending)) continue;
      this.#pending.delete(id);
      pending.reject(new Error('Browser connection closed'));
    }
  }

  /** Drops a session whose target went away (or that was detached): what it still waits on is rejected. */
  #forget(sessionId: string): boolean {
    if (!this.#sessions.delete(sessionId)) return false;
    this.#rejectPending((pending) => pending.sessionId === sessionId);
    this.#ended(sessionId);
    return true;
  }

  #ended(sessionId: string): void {
    for (const listener of this.#endedListeners) {
      try {
        listener(sessionId);
      } catch {
        // One bad listener must not break the connection.
      }
    }
  }

  /** Calls `listener` with the id of each session that ends (its target closed, it was detached, or the socket closed). */
  onEnded(listener: (sessionId: string) => void): () => void {
    this.#endedListeners.add(listener);
    return () => this.#endedListeners.delete(listener);
  }

  /** Send a CDP command to the browser, or, with `sessionId`, to that attached target; `signal` rejects the wait (the command may still run). */
  send<T = Record<string, unknown>>(method: string, params: Record<string, unknown> = {}, signal?: AbortSignal, sessionId?: string): Promise<T> {
    const id = this.#nextId++;
    if (!this.open || (sessionId !== undefined && !this.#sessions.has(sessionId))) return Promise.reject(new Error('Browser connection closed'));
    if (signal?.aborted) return Promise.reject(abortError(signal));
    let onAbort: (() => void) | undefined;
    const result = new Promise<T>((resolve, reject) => {
      this.#pending.set(id, { sessionId, resolve: resolve as (value: unknown) => void, reject });
      if (signal) signal.addEventListener('abort', (onAbort = () => reject(abortError(signal))), { once: true });
    });
    try {
      this.#socket.send(JSON.stringify({ id, method, params, ...(sessionId === undefined ? {} : { sessionId }) }));
    } catch (error) {
      // The socket cannot take it (it closed between the check and now): nothing will answer, so fail now, not after 30 s.
      this.#pending.get(id)?.reject(new Error(`Could not send ${method}: ${errorMessage(error)}`));
    }
    return withTimeout(result, 30_000, method).finally(() => {
      this.#pending.delete(id);
      if (onAbort) signal?.removeEventListener('abort', onAbort);
    });
  }

  /**
   * A session on the page `target` (a target of this browser, listed by `/json/list`): the URL the browser gave for it is
   * checked first (see `checkDevtoolsUrl`), though the session itself goes through this connection.
   */
  async attach(target: PageTarget): Promise<CdpPage> {
    checkDevtoolsUrl(target.webSocketDebuggerUrl, this.#port);
    this.#attaching.add(target.id);
    try {
      const { sessionId } = await this.send<{ sessionId: string }>('Target.attachToTarget', { targetId: target.id, flatten: true });
      if (!this.#sessions.has(sessionId)) this.#sessions.set(sessionId, new Set());
      return new CdpPage(this, sessionId);
    } finally {
      this.#attaching.delete(target.id);
    }
  }

  /** Calls `listener` for each target the browser attaches to by itself once `autoAttach` is on (a popup, a worker, a frame in another process). */
  onAttached(listener: (attached: AttachedTarget) => void): () => void {
    this.#attachedListeners.add(listener);
    return () => this.#attachedListeners.delete(listener);
  }

  /**
   * Has the browser (no `sessionId`) or the session `sessionId` attach to the targets related to it: every new one is held
   * before it runs until `Runtime.runIfWaitingForDebugger`, so a listener from `onAttached` sets it up first.
   */
  autoAttach(sessionId?: string): Promise<unknown> {
    return this.send('Target.setAutoAttach', AUTO_ATTACH, undefined, sessionId);
  }

  isAttached(sessionId: string): boolean {
    return this.open && this.#sessions.has(sessionId);
  }

  listen(sessionId: string, listener: CdpListener): () => void {
    this.#sessions.get(sessionId)?.add(listener);
    return () => this.#sessions.get(sessionId)?.delete(listener);
  }

  /** Ends a session; the browser socket and the other sessions stay. */
  detach(sessionId: string): void {
    // Best effort: the target may be gone already, and the session is forgotten here either way.
    if (this.#forget(sessionId) && this.open) void this.send('Target.detachFromTarget', { sessionId }).catch(() => undefined);
  }

  /** Closes the browser socket: every session ends and every pending command is rejected. */
  close(): void {
    this.#socket.close();
  }
}

/** One page target of the browser connection: the session CDP commands, events and dialogs of a tab go through. */
export class CdpPage {
  readonly #connection: CdpConnection;
  readonly #sessionId: string;

  constructor(connection: CdpConnection, sessionId: string) {
    this.#connection = connection;
    this.#sessionId = sessionId;
  }

  get open(): boolean {
    return this.#connection.isAttached(this.#sessionId);
  }

  /** The flat session id of this target: what an attach announced on it names as `parentSessionId`. */
  get sessionId(): string {
    return this.#sessionId;
  }

  /** Send a CDP command; `signal` rejects the wait (the command may still run in the page). */
  send<T = Record<string, unknown>>(method: string, params: Record<string, unknown> = {}, signal?: AbortSignal): Promise<T> {
    return this.#connection.send<T>(method, params, signal, this.#sessionId);
  }

  /** Has the browser attach to the targets related to this page (frames in another process, its workers); see `CdpConnection.autoAttach`. */
  autoAttach(): Promise<unknown> {
    return this.#connection.autoAttach(this.#sessionId);
  }

  on(listener: CdpListener): () => void {
    return this.#connection.listen(this.#sessionId, listener);
  }

  /** Resolves on the first `method` event or after `timeoutMs`; rejects when `signal` aborts. */
  waitFor(method: string, timeoutMs: number, signal?: AbortSignal): Promise<void> {
    return new Promise((resolve, reject) => {
      if (signal?.aborted) return reject(abortError(signal));
      const timer = setTimeout(done, timeoutMs);
      const off = this.on((name) => name === method && done());
      const onAbort = () => {
        cleanup();
        reject(abortError(signal!));
      };
      signal?.addEventListener('abort', onAbort, { once: true });
      function cleanup() {
        clearTimeout(timer);
        off();
        signal?.removeEventListener('abort', onAbort);
      }
      function done() {
        cleanup();
        resolve();
      }
    });
  }

  /** Resolves `work`, or `undefined` as soon as a JavaScript dialog opens (the page then blocks until it is answered). */
  async untilDialog<T>(work: Promise<T>): Promise<T | undefined> {
    let off: (() => void) | undefined;
    const opened = new Promise<undefined>((resolve) => {
      off = this.on((method) => method === 'Page.javascriptDialogOpening' && resolve(undefined));
    });
    try {
      return await Promise.race([work, opened]);
    } finally {
      off?.();
    }
  }

  /**
   * Evaluate an expression in the page and return its JSON value. A result JSON cannot carry (NaN, ±Infinity, -0, a
   * BigInt, a function, a symbol) comes back as the text Chrome describes it with (`NaN`, `10n`, `Symbol(a)`), never as
   * `undefined`, which would read as "the expression returned nothing".
   */
  async evaluate<T = unknown>(expression: string, signal?: AbortSignal): Promise<T> {
    const response = await this.send<{ result?: { type?: string; value?: T; unserializableValue?: string; description?: string }; exceptionDetails?: { text?: string; exception?: { description?: string } } }>('Runtime.evaluate', {
      expression,
      returnByValue: true,
      awaitPromise: true,
    }, signal);
    if (response.exceptionDetails) throw new Error(response.exceptionDetails.exception?.description ?? response.exceptionDetails.text ?? 'Evaluation failed');
    const { value, type, unserializableValue, description } = response.result ?? {};
    return (value === undefined && type !== 'undefined' ? (unserializableValue ?? description) : value) as T;
  }

  /** Detaches this page's session; the tab stays open and the browser connection stays up. */
  close(): void {
    this.#connection.detach(this.#sessionId);
  }
}
