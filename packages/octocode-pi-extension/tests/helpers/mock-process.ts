export type MockProcessHandlers = Record<string, Array<(...args: unknown[]) => void>>;

export interface MockAgentProcess {
  stdin: { write(d: string): void; end(): void };
  stdout: { on(e: string, cb: (b: Buffer) => void): void };
  stderr: { on(e: string, cb: (b: Buffer) => void): void };
  on(e: string, cb: (...a: unknown[]) => void): void;
  kill(signal?: NodeJS.Signals): boolean;
  exitCode: null | number;
  signalCode: null | NodeJS.Signals;
  writes: Array<Record<string, unknown>>;
  _emit(event: string, ...args: unknown[]): void;
}

export interface MockAgentProcessOptions {
  stdinThrows?: boolean;
  exitImmediately?: boolean;
  recordWrites?: boolean;
}

export function makeMockAgentProcess(opts: MockAgentProcessOptions = {}): MockAgentProcess {
  const handlers: MockProcessHandlers = {};
  const recordWrites = opts.recordWrites ?? true;

  const proc: MockAgentProcess = {
    writes: [],
    stdin: {
      write(d: string) {
        if (opts.stdinThrows) {
          throw Object.assign(new Error('write EPIPE'), { code: 'EPIPE' });
        }
        if (recordWrites) {
          proc.writes.push(JSON.parse(d) as Record<string, unknown>);
        }
      },
      end() {},
    },
    stdout: {
      on(e, cb) {
        (handlers[`stdout:${e}`] ??= []).push(cb as never);
      },
    },
    stderr: {
      on(e, cb) {
        (handlers[`stderr:${e}`] ??= []).push(cb as never);
      },
    },
    on(e, cb) {
      (handlers[e] ??= []).push(cb);
    },
    kill() {
      return true;
    },
    exitCode: null,
    signalCode: null,
    _emit(event, ...args) {
      for (const cb of handlers[event] ?? []) cb(...args);
    },
  };

  if (opts.exitImmediately) {
    setTimeout(() => {
      proc.exitCode = 0;
      proc._emit('close', 0, null);
    }, 0);
  }

  return proc;
}

/** Drive the worker to idle via a normal agent_end RPC event. */
export function emitAgentEnd(mock: MockAgentProcess): void {
  mock._emit('stdout:data', Buffer.from(`${JSON.stringify({ type: 'agent_end', messages: [] })}\n`));
}
