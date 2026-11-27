import { errorMessage } from './error-message.mjs';

// CDP messages are unvalidated protocol JSON; callers read the fields each method documents.
export type Payload = Record<string, any>;
export type Handler = (...args: any[]) => unknown;
type Pending = {
  resolve: (value: Payload) => void;
  reject: (error: Error) => void;
  timer: NodeJS.Timeout;
  sessionId?: string;
};
/** One CDP connection; every engine module that drives Chrome takes this as `cdp`. */
export type Session = {
  targetInfo: Payload;
  outputDir: string;
  send(
    method: string,
    params?: Payload,
    sessionId?: string,
    options?: { timeoutMs?: number }
  ): Promise<Payload>;
  on(event: string, handler: Handler): void;
  off(event: string, handler: Handler): void;
  log(...args: unknown[]): void;
  close(): void;
};

/** A handler's result that rejects asynchronously: a promise or any value with a `catch` method. */
const isCatchable = (
  value: unknown
): value is { catch(reject: (error: unknown) => void): unknown } =>
  (typeof value === 'object' || typeof value === 'function') &&
  value !== null &&
  'catch' in value &&
  typeof value.catch === 'function';

export function connectCDP(
  url: string,
  {
    targetInfo = {},
    timeoutMs = 30000,
  }: { targetInfo?: Payload; timeoutMs?: number } = {}
) {
  return new Promise<Session>((resolve, reject) => {
    const socket = new WebSocket(url),
      pending = new Map<number, Pending>(),
      handlers = new Map<string, Set<Handler>>();
    let nextId = 1,
      closed = false,
      opened = false;
    // A detached target never answers: fail its requests now instead of at their deadline.
    const drain = (reason: string, sessionId?: string) => {
      for (const [id, entry] of pending) {
        if (sessionId !== undefined && entry.sessionId !== sessionId) continue;
        clearTimeout(entry.timer);
        entry.reject(new Error(reason));
        pending.delete(id);
      }
    };
    function makeSession() {
      return {
        targetInfo,
        outputDir: '',
        send(
          method: string,
          params: Payload = {},
          sessionId?: string,
          options: { timeoutMs?: number } = {}
        ): Promise<Payload> {
          if (closed)
            return Promise.reject(new Error('Session already closed'));
          const deadline = Math.min(timeoutMs, options.timeoutMs ?? timeoutMs);
          if (!Number.isSafeInteger(deadline) || deadline < 1)
            return Promise.reject(new Error('Invalid CDP request timeout'));
          return new Promise((resolveRequest, rejectRequest) => {
            const id = nextId++,
              timer = setTimeout(() => {
                pending.delete(id);
                rejectRequest(
                  new Error(`CDP timeout (${deadline}ms) for: ${method}`)
                );
              }, deadline);
            pending.set(id, {
              resolve: resolveRequest,
              reject: rejectRequest,
              timer,
              sessionId,
            });
            try {
              socket.send(
                JSON.stringify({
                  id,
                  method,
                  params,
                  ...(sessionId ? { sessionId } : {}),
                })
              );
            } catch (error) {
              clearTimeout(timer);
              pending.delete(id);
              rejectRequest(error);
            }
          });
        },
        on(event: string, handler: Handler) {
          if (!handlers.has(event)) handlers.set(event, new Set());
          handlers.get(event)!.add(handler);
        },
        off(event: string, handler: Handler) {
          handlers.get(event)?.delete(handler);
        },
        log(...args: unknown[]) {
          console.log('[BROWSER]', ...args);
        },
        close() {
          if (closed) return;
          closed = true;
          drain('Session closed');
          handlers.clear();
          socket.close();
        },
      };
    }
    socket.onopen = () => {
      opened = true;
      resolve(makeSession());
    };
    socket.onmessage = event => {
      let message: Payload;
      try {
        message = JSON.parse(String(event.data));
      } catch {
        return;
      }
      const entry = pending.get(message.id);
      if (entry) {
        pending.delete(message.id);
        clearTimeout(entry.timer);
        if (message.error)
          entry.reject(
            Object.assign(
              new Error(
                `CDP error [${message.error.code}]: ${message.error.message}`
              ),
              { protocolError: message.error }
            )
          );
        else entry.resolve(message.result ?? {});
      } else if (message.method) {
        const detached = message.params?.sessionId;
        if (message.method === 'Target.detachedFromTarget' && detached)
          drain(`Session ${detached} detached from its target`, detached);
        const meta = message.sessionId ? { sessionId: message.sessionId } : {};
        const dispatch = (handler: Handler, args: unknown[]) => {
          const failed = (error: unknown) => {
            console.error(
              `[CDP_HANDLER_ERROR] event=${message.method} ${errorMessage(error)}`
            );
            process.exitCode = 1;
          };
          try {
            const returned: unknown = handler(...args);
            if (isCatchable(returned)) returned.catch(failed);
          } catch (error) {
            failed(error);
          }
        };
        for (const handler of handlers.get(message.method) ?? [])
          dispatch(handler, [message.params ?? {}, meta]);
        for (const handler of handlers.get('*') ?? [])
          dispatch(handler, [message.method, message.params ?? {}, meta]);
      }
    };
    socket.onerror = event => {
      const message =
        'message' in event ? String(event.message) : String(event);
      drain(`WebSocket error: ${message}`);
      if (!opened) reject(new Error(`WebSocket error: ${message}`));
    };
    socket.onclose = () => {
      closed = true;
      drain('WebSocket closed unexpectedly');
      handlers.clear();
      if (!opened) reject(new Error('WebSocket closed before opening'));
    };
  });
}
