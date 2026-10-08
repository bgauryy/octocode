type Payload = Record<string, any>;
type Handler = (...args: any[]) => unknown;
type Pending = {
  resolve: (value: Payload) => void;
  reject: (error: Error) => void;
  timer: NodeJS.Timeout;
};
type Session = {
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
    const drain = (reason: string) => {
      for (const entry of pending.values()) {
        clearTimeout(entry.timer);
        entry.reject(new Error(reason));
      }
      pending.clear();
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
        const meta = message.sessionId ? { sessionId: message.sessionId } : {};
        const dispatch = (handler: Handler, args: unknown[]) => {
          const failed = (error: unknown) => {
            console.error(
              `[CDP_HANDLER_ERROR] event=${message.method} ${error instanceof Error ? error.message : String(error)}`
            );
            process.exitCode = 1;
          };
          try {
            const returned = handler(...args);
            const promiseLike = returned as
              | { catch?: (reject: (error: unknown) => void) => unknown }
              | null
              | undefined;
            if (typeof promiseLike?.catch === 'function')
              promiseLike.catch(failed);
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
